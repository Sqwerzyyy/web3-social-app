use crate::assets::{Currency, Price};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PriceError {
    #[error("no price for '{symbol}' in '{currency}'")]
    NotFound { symbol: String, currency: String },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("price must be positive, got {0}")]
    NonPositive(Decimal),
}

#[async_trait]
pub trait PriceProvider: Send + Sync {
    async fn current_price(&self, symbol: &str, currency: &Currency) -> Result<Price, PriceError>;

    async fn historical_price(
        &self,
        symbol: &str,
        currency: &Currency,
        _at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Price, PriceError> {
        self.current_price(symbol, currency).await
    }
}

#[derive(Default)]
pub struct InMemoryPriceProvider {
    prices: HashMap<(String, String), Decimal>,
}

impl InMemoryPriceProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, symbol: impl Into<String>, currency: &Currency, price: Decimal) {
        self.prices
            .insert((symbol.into(), currency.as_str().to_string()), price);
    }
}

#[async_trait]
impl PriceProvider for InMemoryPriceProvider {
    async fn current_price(&self, symbol: &str, currency: &Currency) -> Result<Price, PriceError> {
        let key = (symbol.to_string(), currency.as_str().to_string());
        let &value = self.prices.get(&key).ok_or_else(|| PriceError::NotFound {
            symbol: symbol.to_string(),
            currency: currency.as_str().to_string(),
        })?;
        Price::new(value, currency.clone(), Utc::now()).ok_or(PriceError::NonPositive(value))
    }
}

pub enum ResponseFormat {
    CoinGecko,
    FlatPrice { field: String },
}

pub struct HttpPriceProvider {
    client: reqwest::Client,
    base_url: String,
    format: ResponseFormat,
    symbol_map: HashMap<String, String>,
}

impl HttpPriceProvider {
    pub fn new(base_url: impl Into<String>, format: ResponseFormat) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            format,
            symbol_map: HashMap::new(),
        }
    }

    pub fn map_symbol(mut self, symbol: impl Into<String>, api_id: impl Into<String>) -> Self {
        self.symbol_map
            .insert(symbol.into().to_uppercase(), api_id.into());
        self
    }

    fn resolve_symbol(&self, symbol: &str) -> String {
        self.symbol_map
            .get(&symbol.to_uppercase())
            .cloned()
            .unwrap_or_else(|| symbol.to_string())
    }
}

fn parse_decimal_value(value: &Value, label: &str) -> Result<Decimal, PriceError> {
    match value {
        Value::Number(number) => number
            .to_string()
            .parse::<Decimal>()
            .map_err(|_| PriceError::Parse(label.to_string())),
        Value::String(text) => text
            .parse::<Decimal>()
            .map_err(|_| PriceError::Parse(label.to_string())),
        _ => Err(PriceError::Parse(label.to_string())),
    }
}

#[async_trait]
impl PriceProvider for HttpPriceProvider {
    async fn current_price(&self, symbol: &str, currency: &Currency) -> Result<Price, PriceError> {
        let api_id = self.resolve_symbol(symbol);
        let cur_lc = currency.as_str().to_lowercase();

        let value = match &self.format {
            ResponseFormat::CoinGecko => {
                let url = format!(
                    "{}/simple/price?ids={}&vs_currencies={}",
                    self.base_url, api_id, cur_lc
                );
                let resp: serde_json::Value = self.client.get(&url).send().await?.json().await?;
                parse_decimal_value(
                    &resp[&api_id][&cur_lc],
                    &format!("CoinGecko: {}/{}", symbol, currency),
                )?
            }
            ResponseFormat::FlatPrice { field } => {
                let url = format!("{}/price/{}?currency={}", self.base_url, api_id, currency);
                let resp: serde_json::Value = self.client.get(&url).send().await?.json().await?;
                parse_decimal_value(&resp[field], &format!("field '{}' missing", field))?
            }
        };

        Price::new(value, currency.clone(), Utc::now()).ok_or(PriceError::NonPositive(value))
    }
}
