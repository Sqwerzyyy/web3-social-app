#[cfg(test)]
mod tests;

use crate::assets::{Currency, Price};
use crate::financial_engine::AssetId;
use crate::price::{PriceError, PriceProvider};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use thiserror::Error;

pub const DEFAULT_MAX_AGE_SECS: u64 = 600;
pub const DEFAULT_MAX_SPREAD_BPS: u32 = 200;
pub const DEFAULT_TWAP_WINDOW_SECS: u64 = 900;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceFeed {
    pub asset_id: AssetId,
    pub price: Decimal,
    pub timestamp: u64,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskMode {
    Normal,
    Halt,
}

pub type CircuitBreaker = RiskMode;

#[derive(Debug, Error)]
pub enum OracleError {
    #[error("source unavailable: {0}")]
    SourceUnavailable(String),

    #[error("no price consensus for asset {0}")]
    NoConsensus(AssetId),

    #[error("stale data from {source_name}: age {age_secs}s exceeds {max_age_secs}s")]
    StaleData {
        source_name: String,
        age_secs: u64,
        max_age_secs: u64,
    },

    #[error("oracle spread {spread_bps} bps exceeds limit {max_spread_bps} bps")]
    Outlier {
        spread_bps: Decimal,
        max_spread_bps: u32,
    },

    #[error("invalid price from {source_name}")]
    InvalidPrice { source_name: String },

    #[error("risk engine halted")]
    Halted,

    #[error("arithmetic overflow")]
    MathOverflow,

    #[error("state lock poisoned")]
    LockPoisoned,

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("parse error: {0}")]
    Parse(String),
}

#[async_trait]
pub trait PriceSource: Send + Sync {
    async fn latest_price(&self, asset_id: AssetId) -> Result<PriceFeed, OracleError>;
}

#[derive(Debug, Clone)]
pub struct ChainlinkPriceAdapter {
    feeds: Arc<RwLock<HashMap<AssetId, PriceFeed>>>,
}

impl ChainlinkPriceAdapter {
    pub fn new() -> Self {
        Self {
            feeds: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_price(
        &self,
        asset_id: AssetId,
        price: Decimal,
        timestamp: u64,
    ) -> Result<(), OracleError> {
        set_adapter_price(&self.feeds, asset_id, price, timestamp, "chainlink")
    }
}

impl Default for ChainlinkPriceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PriceSource for ChainlinkPriceAdapter {
    async fn latest_price(&self, asset_id: AssetId) -> Result<PriceFeed, OracleError> {
        get_adapter_price(&self.feeds, asset_id, "chainlink")
    }
}

#[derive(Debug, Clone)]
pub struct PythPriceAdapter {
    feeds: Arc<RwLock<HashMap<AssetId, PriceFeed>>>,
}

impl PythPriceAdapter {
    pub fn new() -> Self {
        Self {
            feeds: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_price(
        &self,
        asset_id: AssetId,
        price: Decimal,
        timestamp: u64,
    ) -> Result<(), OracleError> {
        set_adapter_price(&self.feeds, asset_id, price, timestamp, "pyth")
    }
}

impl Default for PythPriceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PriceSource for PythPriceAdapter {
    async fn latest_price(&self, asset_id: AssetId) -> Result<PriceFeed, OracleError> {
        get_adapter_price(&self.feeds, asset_id, "pyth")
    }
}

#[derive(Debug, Clone)]
pub struct RestPriceAdapter {
    client: reqwest::Client,
    base_url: String,
    price_field: String,
    timestamp_field: String,
    source: String,
}

impl RestPriceAdapter {
    pub fn new(
        base_url: impl Into<String>,
        price_field: impl Into<String>,
        timestamp_field: impl Into<String>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            price_field: price_field.into(),
            timestamp_field: timestamp_field.into(),
            source: "rest".to_string(),
        }
    }
}

#[async_trait]
impl PriceSource for RestPriceAdapter {
    async fn latest_price(&self, asset_id: AssetId) -> Result<PriceFeed, OracleError> {
        let url = format!("{}/prices/{}", self.base_url, asset_id.0);
        let value: Value = self.client.get(url).send().await?.json().await?;
        let price = parse_decimal_value(&value[&self.price_field], &self.price_field)?;
        let timestamp = value[&self.timestamp_field]
            .as_u64()
            .ok_or_else(|| OracleError::Parse(self.timestamp_field.clone()))?;

        Ok(PriceFeed {
            asset_id,
            price,
            timestamp,
            source: self.source.clone(),
        })
    }
}

pub struct OracleAggregator {
    sources: Vec<Arc<dyn PriceSource>>,
    max_age_secs: u64,
    max_spread_bps: u32,
    history: Arc<RwLock<HashMap<AssetId, VecDeque<PriceFeed>>>>,
}

impl OracleAggregator {
    pub fn new(max_age_secs: u64, max_spread_bps: u32) -> Self {
        Self {
            sources: Vec::new(),
            max_age_secs,
            max_spread_bps,
            history: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_source(mut self, source: Arc<dyn PriceSource>) -> Self {
        self.sources.push(source);
        self
    }

    pub async fn consensus_price(
        &self,
        asset_id: AssetId,
        now_ts: u64,
    ) -> Result<PriceFeed, OracleError> {
        let mut feeds = Vec::new();
        for source in &self.sources {
            let feed = source.latest_price(asset_id).await?;
            validate_feed(&feed, now_ts, self.max_age_secs)?;
            feeds.push(feed);
        }

        if feeds.is_empty() {
            return Err(OracleError::NoConsensus(asset_id));
        }

        validate_spreads(&feeds, self.max_spread_bps)?;
        let price = average_price(&feeds)?;
        let feed = PriceFeed {
            asset_id,
            price,
            timestamp: now_ts,
            source: "consensus".to_string(),
        };
        self.push_history(feed.clone())?;
        Ok(feed)
    }

    pub fn push_history(&self, feed: PriceFeed) -> Result<(), OracleError> {
        validate_positive(&feed)?;
        let mut history = self
            .history
            .write()
            .map_err(|_| OracleError::LockPoisoned)?;
        let entries = history.entry(feed.asset_id).or_default();
        entries.push_back(feed);
        while entries.len() > 2048 {
            entries.pop_front();
        }
        Ok(())
    }

    pub fn twap(
        &self,
        asset_id: AssetId,
        now_ts: u64,
        window_secs: u64,
    ) -> Result<PriceFeed, OracleError> {
        let history = self.history.read().map_err(|_| OracleError::LockPoisoned)?;
        let entries = history
            .get(&asset_id)
            .ok_or(OracleError::NoConsensus(asset_id))?;
        let price = calculate_twap(entries, now_ts, window_secs)?;
        Ok(PriceFeed {
            asset_id,
            price,
            timestamp: now_ts,
            source: "twap".to_string(),
        })
    }
}

impl Default for OracleAggregator {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_AGE_SECS, DEFAULT_MAX_SPREAD_BPS)
    }
}

pub struct RiskEngine {
    oracle: Arc<OracleAggregator>,
    mode: Arc<RwLock<RiskMode>>,
}

impl RiskEngine {
    pub fn new(oracle: Arc<OracleAggregator>) -> Self {
        Self {
            oracle,
            mode: Arc::new(RwLock::new(RiskMode::Normal)),
        }
    }

    pub fn mode(&self) -> Result<RiskMode, OracleError> {
        self.mode
            .read()
            .map(|mode| *mode)
            .map_err(|_| OracleError::LockPoisoned)
    }

    pub fn ensure_trading_open(&self) -> Result<(), OracleError> {
        match self.mode()? {
            RiskMode::Normal => Ok(()),
            RiskMode::Halt => Err(OracleError::Halted),
        }
    }

    pub async fn guarded_price(
        &self,
        asset_id: AssetId,
        now_ts: u64,
    ) -> Result<PriceFeed, OracleError> {
        match self.oracle.consensus_price(asset_id, now_ts).await {
            Ok(feed) => {
                self.set_mode(RiskMode::Normal)?;
                Ok(feed)
            }
            Err(error) => {
                self.set_mode(RiskMode::Halt)?;
                Err(error)
            }
        }
    }

    pub fn twap(
        &self,
        asset_id: AssetId,
        now_ts: u64,
        window_secs: u64,
    ) -> Result<PriceFeed, OracleError> {
        self.oracle.twap(asset_id, now_ts, window_secs)
    }

    fn set_mode(&self, next: RiskMode) -> Result<(), OracleError> {
        let mut mode = self.mode.write().map_err(|_| OracleError::LockPoisoned)?;
        *mode = next;
        Ok(())
    }
}

#[derive(Clone)]
pub struct OraclePriceProvider {
    risk: Arc<RiskEngine>,
    symbol_map: Arc<HashMap<String, AssetId>>,
    currency: Currency,
}

impl OraclePriceProvider {
    pub fn new(
        risk: Arc<RiskEngine>,
        symbol_map: HashMap<String, AssetId>,
        currency: Currency,
    ) -> Self {
        Self {
            risk,
            symbol_map: Arc::new(symbol_map),
            currency,
        }
    }
}

#[async_trait]
impl PriceProvider for OraclePriceProvider {
    async fn current_price(&self, symbol: &str, currency: &Currency) -> Result<Price, PriceError> {
        if currency != &self.currency {
            return Err(PriceError::Parse(format!(
                "unsupported oracle currency: {}",
                currency
            )));
        }

        let asset_id =
            self.symbol_map
                .get(symbol)
                .copied()
                .ok_or_else(|| PriceError::NotFound {
                    symbol: symbol.to_string(),
                    currency: currency.as_str().to_string(),
                })?;
        let now_ts = u64::try_from(Utc::now().timestamp())
            .map_err(|_| PriceError::Parse("current timestamp before unix epoch".to_string()))?;
        let feed = self
            .risk
            .guarded_price(asset_id, now_ts)
            .await
            .map_err(|error| PriceError::Parse(error.to_string()))?;
        let feed_ts = i64::try_from(feed.timestamp)
            .map_err(|_| PriceError::Parse("oracle timestamp out of range".to_string()))?;
        let timestamp = Utc
            .timestamp_opt(feed_ts, 0)
            .single()
            .ok_or_else(|| PriceError::Parse("invalid oracle timestamp".to_string()))?;

        Price::new(feed.price, currency.clone(), timestamp)
            .ok_or(PriceError::NonPositive(feed.price))
    }
}

fn set_adapter_price(
    feeds: &RwLock<HashMap<AssetId, PriceFeed>>,
    asset_id: AssetId,
    price: Decimal,
    timestamp: u64,
    source: &str,
) -> Result<(), OracleError> {
    let feed = PriceFeed {
        asset_id,
        price,
        timestamp,
        source: source.to_string(),
    };
    validate_positive(&feed)?;
    feeds
        .write()
        .map_err(|_| OracleError::LockPoisoned)?
        .insert(asset_id, feed);
    Ok(())
}

fn get_adapter_price(
    feeds: &RwLock<HashMap<AssetId, PriceFeed>>,
    asset_id: AssetId,
    source: &str,
) -> Result<PriceFeed, OracleError> {
    feeds
        .read()
        .map_err(|_| OracleError::LockPoisoned)?
        .get(&asset_id)
        .cloned()
        .ok_or_else(|| OracleError::SourceUnavailable(source.to_string()))
}

fn parse_decimal_value(value: &Value, field: &str) -> Result<Decimal, OracleError> {
    match value {
        Value::Number(number) => number
            .to_string()
            .parse::<Decimal>()
            .map_err(|_| OracleError::Parse(field.to_string())),
        Value::String(text) => text
            .parse::<Decimal>()
            .map_err(|_| OracleError::Parse(field.to_string())),
        _ => Err(OracleError::Parse(field.to_string())),
    }
}

fn validate_feed(feed: &PriceFeed, now_ts: u64, max_age_secs: u64) -> Result<(), OracleError> {
    validate_positive(feed)?;
    let age_secs = now_ts
        .checked_sub(feed.timestamp)
        .ok_or(OracleError::MathOverflow)?;
    if age_secs > max_age_secs {
        return Err(OracleError::StaleData {
            source_name: feed.source.clone(),
            age_secs,
            max_age_secs,
        });
    }
    Ok(())
}

fn validate_positive(feed: &PriceFeed) -> Result<(), OracleError> {
    if feed.price <= Decimal::ZERO {
        return Err(OracleError::InvalidPrice {
            source_name: feed.source.clone(),
        });
    }
    Ok(())
}

fn validate_spreads(feeds: &[PriceFeed], max_spread_bps: u32) -> Result<(), OracleError> {
    for left in feeds {
        for right in feeds {
            let spread = spread_bps(left.price, right.price)?;
            if spread > Decimal::from(max_spread_bps) {
                return Err(OracleError::Outlier {
                    spread_bps: spread,
                    max_spread_bps,
                });
            }
        }
    }
    Ok(())
}

fn spread_bps(left: Decimal, right: Decimal) -> Result<Decimal, OracleError> {
    if left.is_zero() {
        return Err(OracleError::MathOverflow);
    }
    let diff = left
        .checked_sub(right)
        .ok_or(OracleError::MathOverflow)?
        .abs();
    diff.checked_div(left)
        .and_then(|ratio| ratio.checked_mul(dec!(10_000)))
        .ok_or(OracleError::MathOverflow)
}

fn average_price(feeds: &[PriceFeed]) -> Result<Decimal, OracleError> {
    let mut total = Decimal::ZERO;
    for feed in feeds {
        total = total
            .checked_add(feed.price)
            .ok_or(OracleError::MathOverflow)?;
    }
    total
        .checked_div(Decimal::from(feeds.len()))
        .ok_or(OracleError::MathOverflow)
}

fn calculate_twap(
    entries: &VecDeque<PriceFeed>,
    now_ts: u64,
    window_secs: u64,
) -> Result<Decimal, OracleError> {
    if window_secs == 0 {
        return Err(OracleError::MathOverflow);
    }

    let window_start = now_ts
        .checked_sub(window_secs)
        .ok_or(OracleError::MathOverflow)?;
    let mut ordered: BTreeMap<u64, Decimal> = BTreeMap::new();
    for feed in entries {
        validate_positive(feed)?;
        if feed.timestamp <= now_ts {
            ordered.insert(feed.timestamp, feed.price);
        }
    }

    let mut active_price = ordered
        .range(..=window_start)
        .next_back()
        .map(|(_, price)| *price)
        .or_else(|| {
            ordered
                .range(window_start..=now_ts)
                .next()
                .map(|(_, price)| *price)
        })
        .ok_or(OracleError::NoConsensus(AssetId(0)))?;

    let mut cursor = window_start;
    let mut weighted_sum = Decimal::ZERO;
    let mut weighted_secs = 0_u64;

    // TWAP weights each active price by the seconds it governed inside the window.
    for (timestamp, price) in ordered.range(window_start..=now_ts) {
        if *timestamp > cursor {
            let span = timestamp
                .checked_sub(cursor)
                .ok_or(OracleError::MathOverflow)?;
            weighted_sum = weighted_sum
                .checked_add(
                    active_price
                        .checked_mul(Decimal::from(span))
                        .ok_or(OracleError::MathOverflow)?,
                )
                .ok_or(OracleError::MathOverflow)?;
            weighted_secs = weighted_secs
                .checked_add(span)
                .ok_or(OracleError::MathOverflow)?;
            cursor = *timestamp;
        }
        active_price = *price;
    }

    if now_ts > cursor {
        let span = now_ts
            .checked_sub(cursor)
            .ok_or(OracleError::MathOverflow)?;
        weighted_sum = weighted_sum
            .checked_add(
                active_price
                    .checked_mul(Decimal::from(span))
                    .ok_or(OracleError::MathOverflow)?,
            )
            .ok_or(OracleError::MathOverflow)?;
        weighted_secs = weighted_secs
            .checked_add(span)
            .ok_or(OracleError::MathOverflow)?;
    }

    if weighted_secs == 0 {
        return Err(OracleError::NoConsensus(AssetId(0)));
    }

    weighted_sum
        .checked_div(Decimal::from(weighted_secs))
        .ok_or(OracleError::MathOverflow)
}
