mod display;
#[cfg(test)]
mod tests;

use crate::assets::{AssetClass, AssetPosition, Currency, Money, DAYS_YEAR};
use crate::filters::{PositionFilter, Timeframe};
use crate::financial_engine::{Transaction, TransactionStatus, TxId};
use crate::price::{PriceError, PriceProvider};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PnlError {
    #[error("price error for '{symbol}': {source}")]
    Price {
        symbol: String,
        #[source]
        source: PriceError,
    },

    #[error("no positions or realized events match the given filter")]
    NoPositions,

    #[error("arithmetic overflow")]
    MathOverflow,

    #[error("currency mismatch: expected {expected}, got {actual}")]
    CurrencyMismatch {
        expected: Currency,
        actual: Currency,
    },

    #[error("realized PnL amounts must be non-negative")]
    InvalidRealizedAmount,

    #[error("ledger transaction {0} is not posted")]
    LedgerTransactionNotPosted(TxId),

    #[error("ledger transaction {0} has no double-entry confirmation")]
    LedgerTransactionNotDoubleEntry(TxId),
}

pub use PnlError as EngineError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealizedPnlEvent {
    pub ledger_tx_id: TxId,
    pub symbol: String,
    pub asset_class: AssetClass,
    pub currency: Currency,
    pub proceeds: Money,
    pub cost_basis: Money,
    pub realized_pnl: Money,
    pub realized_at: DateTime<Utc>,
}

impl RealizedPnlEvent {
    pub fn from_posted_ledger(
        tx: &Transaction,
        symbol: impl Into<String>,
        asset_class: AssetClass,
        currency: Currency,
        proceeds: Decimal,
        cost_basis: Decimal,
    ) -> Result<Self, PnlError> {
        if tx.status != TransactionStatus::Posted {
            return Err(PnlError::LedgerTransactionNotPosted(tx.id));
        }
        if tx.entries.len() < 2 {
            return Err(PnlError::LedgerTransactionNotDoubleEntry(tx.id));
        }
        if proceeds < Decimal::ZERO || cost_basis < Decimal::ZERO {
            return Err(PnlError::InvalidRealizedAmount);
        }

        let realized = checked_sub(proceeds, cost_basis)?;
        Ok(Self {
            ledger_tx_id: tx.id,
            symbol: symbol.into(),
            asset_class,
            currency: currency.clone(),
            proceeds: Money::new(proceeds, currency.clone()),
            cost_basis: Money::new(cost_basis, currency.clone()),
            realized_pnl: Money::new(realized, currency),
            realized_at: tx.timestamp,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionPnl {
    pub symbol: String,
    pub asset_class: AssetClass,
    pub currency: Currency,
    pub units: Decimal,
    pub cost_basis: Money,
    pub realized_cost_basis: Money,
    pub market_value: Money,
    pub unrealized_pnl: Money,
    pub realized_pnl: Money,
    pub income: Money,
    pub total_pnl: Money,
    pub roi_pct: Decimal,
    pub annualized_roi: Decimal,
    pub holding_days: i64,
    pub price_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupPnl {
    pub cost_basis: Money,
    pub realized_cost_basis: Money,
    pub market_value: Money,
    pub unrealized_pnl: Money,
    pub realized_pnl: Money,
    pub income: Money,
    pub total_pnl: Money,
    pub roi_pct: Decimal,
    pub weight_pct: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlSnapshot {
    pub timeframe: Timeframe,
    pub as_of: DateTime<Utc>,
    pub currency: Currency,
    pub total_cost_basis: Money,
    pub total_realized_cost_basis: Money,
    pub total_market_value: Money,
    pub total_unrealized: Money,
    pub total_realized: Money,
    pub total_income: Money,
    pub total_pnl: Money,
    pub roi_pct: Decimal,
    pub annualized_roi: Decimal,
    pub positions: Vec<PositionPnl>,
    pub realized_events: Vec<RealizedPnlEvent>,
    pub by_class: HashMap<String, GroupPnl>,
}

fn checked_add(a: Decimal, b: Decimal) -> Result<Decimal, PnlError> {
    a.checked_add(b).ok_or(PnlError::MathOverflow)
}

fn checked_sub(a: Decimal, b: Decimal) -> Result<Decimal, PnlError> {
    a.checked_sub(b).ok_or(PnlError::MathOverflow)
}

fn checked_mul(a: Decimal, b: Decimal) -> Result<Decimal, PnlError> {
    a.checked_mul(b).ok_or(PnlError::MathOverflow)
}

fn checked_div(a: Decimal, b: Decimal) -> Result<Decimal, PnlError> {
    a.checked_div(b).ok_or(PnlError::MathOverflow)
}

fn add_assign(target: &mut Decimal, amount: Decimal) -> Result<(), PnlError> {
    *target = checked_add(*target, amount)?;
    Ok(())
}

fn ensure_currency(money: &Money, expected: &Currency) -> Result<(), PnlError> {
    if money.currency != *expected {
        return Err(PnlError::CurrencyMismatch {
            expected: expected.clone(),
            actual: money.currency.clone(),
        });
    }
    Ok(())
}

fn calc_roi(total_pnl: Decimal, basis: Decimal) -> Result<Decimal, PnlError> {
    if basis.is_zero() {
        return Ok(Decimal::ZERO);
    }
    checked_mul(checked_div(total_pnl, basis)?, dec!(100))
}

fn calc_annualized(roi_pct: Decimal, holding_days: i64) -> Result<Decimal, PnlError> {
    if holding_days <= 0 {
        return Ok(Decimal::ZERO);
    }
    checked_div(
        checked_mul(roi_pct, DAYS_YEAR)?,
        Decimal::from(holding_days),
    )
}

#[derive(Debug, Clone, Default)]
struct DecimalTotals {
    cost_basis: Decimal,
    realized_cost_basis: Decimal,
    market_value: Decimal,
    unrealized: Decimal,
    realized: Decimal,
    income: Decimal,
}

impl DecimalTotals {
    fn total_pnl(&self) -> Result<Decimal, PnlError> {
        checked_add(checked_add(self.unrealized, self.realized)?, self.income)
    }

    fn roi_basis(&self) -> Result<Decimal, PnlError> {
        checked_add(self.cost_basis, self.realized_cost_basis)
    }
}

pub struct PnlEngine<P: PriceProvider> {
    price_provider: P,
}

impl<P: PriceProvider> PnlEngine<P> {
    pub fn new(price_provider: P) -> Self {
        Self { price_provider }
    }

    pub async fn compute(
        &self,
        positions: &[Box<dyn AssetPosition>],
        filter: &PositionFilter,
        timeframe: &Timeframe,
        report_ccy: &Currency,
    ) -> Result<PnlSnapshot, PnlError> {
        self.compute_with_realized(positions, &[], filter, timeframe, report_ccy)
            .await
    }

    pub async fn compute_with_realized(
        &self,
        positions: &[Box<dyn AssetPosition>],
        realized_events: &[RealizedPnlEvent],
        filter: &PositionFilter,
        timeframe: &Timeframe,
        report_ccy: &Currency,
    ) -> Result<PnlSnapshot, PnlError> {
        let now = Utc::now();
        let earliest = positions
            .iter()
            .map(|p| p.purchase_date())
            .chain(realized_events.iter().map(|event| event.realized_at))
            .min();
        let (tf_from, tf_to) = timeframe.resolve(now, earliest);

        let filtered: Vec<&Box<dyn AssetPosition>> = positions
            .iter()
            .filter(|p| p.purchase_date() <= tf_to && filter.matches(p.as_ref()))
            .collect();

        let selected_symbols: HashSet<&str> = filtered.iter().map(|p| p.symbol()).collect();
        let realized = self.filter_realized_events(
            realized_events,
            filter,
            report_ccy,
            tf_from,
            tf_to,
            &selected_symbols,
        )?;

        if filtered.is_empty() && realized.is_empty() {
            return Err(PnlError::NoPositions);
        }

        let mut realized_by_symbol: HashMap<&str, DecimalTotals> = HashMap::new();
        for event in &realized {
            let totals = realized_by_symbol.entry(event.symbol.as_str()).or_default();
            add_assign(&mut totals.realized, event.realized_pnl.amount)?;
            add_assign(&mut totals.realized_cost_basis, event.cost_basis.amount)?;
        }

        let mut pos_pnls = Vec::with_capacity(filtered.len());
        for pos in &filtered {
            let realized_totals = realized_by_symbol
                .get(pos.symbol())
                .cloned()
                .unwrap_or_default();
            pos_pnls.push(
                self.compute_position(pos.as_ref(), tf_from, tf_to, report_ccy, realized_totals)
                    .await?,
            );
        }

        self.aggregate(pos_pnls, realized, timeframe.clone(), now, report_ccy)
    }

    fn filter_realized_events(
        &self,
        realized_events: &[RealizedPnlEvent],
        filter: &PositionFilter,
        report_ccy: &Currency,
        tf_from: DateTime<Utc>,
        tf_to: DateTime<Utc>,
        selected_symbols: &HashSet<&str>,
    ) -> Result<Vec<RealizedPnlEvent>, PnlError> {
        let mut events = Vec::new();
        for event in realized_events {
            ensure_currency(&event.realized_pnl, report_ccy)?;
            ensure_currency(&event.cost_basis, report_ccy)?;
            ensure_currency(&event.proceeds, report_ccy)?;

            let in_timeframe = event.realized_at >= tf_from && event.realized_at <= tf_to;
            let class_ok = filter
                .asset_class
                .as_ref()
                .is_none_or(|asset_class| event.asset_class == *asset_class);
            let currency_ok = filter
                .currency
                .as_ref()
                .is_none_or(|currency| event.currency == *currency);
            let symbol_ok = filter.symbols.is_empty() || filter.symbols.contains(&event.symbol);
            let min_basis_ok = filter
                .min_cost_basis_usd
                .is_none_or(|min| event.cost_basis.amount >= min);
            let tags_ok =
                filter.tags.is_empty() || selected_symbols.contains(event.symbol.as_str());

            if in_timeframe && class_ok && currency_ok && symbol_ok && min_basis_ok && tags_ok {
                events.push(event.clone());
            }
        }
        Ok(events)
    }

    async fn compute_position(
        &self,
        pos: &dyn AssetPosition,
        tf_from: DateTime<Utc>,
        tf_to: DateTime<Utc>,
        report_ccy: &Currency,
        realized_totals: DecimalTotals,
    ) -> Result<PositionPnl, PnlError> {
        let pos_currency = pos.currency();
        if pos_currency != *report_ccy {
            return Err(PnlError::CurrencyMismatch {
                expected: report_ccy.clone(),
                actual: pos_currency,
            });
        }

        let price = self
            .price_provider
            .current_price(pos.symbol(), report_ccy)
            .await
            .map_err(|e| PnlError::Price {
                symbol: pos.symbol().to_string(),
                source: e,
            })?;

        let income = pos.accumulated_income();
        ensure_currency(&income, report_ccy)?;

        let units = pos.units().0;
        let cost_amount = checked_mul(units, pos.purchase_price_per_unit())?;
        let market_amount = checked_mul(units, price.value)?;
        let unrealized_amount = checked_sub(market_amount, cost_amount)?;
        let total_amount = checked_add(
            checked_add(unrealized_amount, realized_totals.realized)?,
            income.amount,
        )?;
        let roi_basis = checked_add(cost_amount, realized_totals.realized_cost_basis)?;
        let roi_pct = calc_roi(total_amount, roi_basis)?;
        let holding_days = (tf_to - pos.purchase_date().max(tf_from)).num_days().max(0);

        Ok(PositionPnl {
            symbol: pos.symbol().to_string(),
            asset_class: pos.asset_class(),
            currency: report_ccy.clone(),
            units,
            cost_basis: Money::new(cost_amount, report_ccy.clone()),
            realized_cost_basis: Money::new(
                realized_totals.realized_cost_basis,
                report_ccy.clone(),
            ),
            market_value: Money::new(market_amount, report_ccy.clone()),
            unrealized_pnl: Money::new(unrealized_amount, report_ccy.clone()),
            realized_pnl: Money::new(realized_totals.realized, report_ccy.clone()),
            income,
            total_pnl: Money::new(total_amount, report_ccy.clone()),
            roi_pct,
            annualized_roi: calc_annualized(roi_pct, holding_days)?,
            holding_days,
            price_at: price.timestamp,
        })
    }

    fn aggregate(
        &self,
        positions: Vec<PositionPnl>,
        realized_events: Vec<RealizedPnlEvent>,
        timeframe: Timeframe,
        as_of: DateTime<Utc>,
        currency: &Currency,
    ) -> Result<PnlSnapshot, PnlError> {
        let mut totals = DecimalTotals::default();
        let mut by_class: HashMap<String, DecimalTotals> = HashMap::new();

        for p in &positions {
            add_assign(&mut totals.cost_basis, p.cost_basis.amount)?;
            add_assign(&mut totals.market_value, p.market_value.amount)?;
            add_assign(&mut totals.unrealized, p.unrealized_pnl.amount)?;
            add_assign(&mut totals.income, p.income.amount)?;

            let entry = by_class.entry(p.asset_class.to_string()).or_default();
            add_assign(&mut entry.cost_basis, p.cost_basis.amount)?;
            add_assign(&mut entry.market_value, p.market_value.amount)?;
            add_assign(&mut entry.unrealized, p.unrealized_pnl.amount)?;
            add_assign(&mut entry.income, p.income.amount)?;
        }

        for event in &realized_events {
            add_assign(&mut totals.realized, event.realized_pnl.amount)?;
            add_assign(&mut totals.realized_cost_basis, event.cost_basis.amount)?;

            let entry = by_class.entry(event.asset_class.to_string()).or_default();
            add_assign(&mut entry.realized, event.realized_pnl.amount)?;
            add_assign(&mut entry.realized_cost_basis, event.cost_basis.amount)?;
        }

        let total_pnl = totals.total_pnl()?;
        let roi_pct = calc_roi(total_pnl, totals.roi_basis()?)?;
        let avg_days = average_holding_days(&positions)?;
        let total_mv = totals.market_value;

        let mut by_class_out = HashMap::new();
        for (class, group) in by_class {
            let group_total = group.total_pnl()?;
            let roi_pct = calc_roi(group_total, group.roi_basis()?)?;
            let weight_pct = if total_mv.is_zero() {
                Decimal::ZERO
            } else {
                checked_mul(checked_div(group.market_value, total_mv)?, dec!(100))?
            };

            by_class_out.insert(
                class,
                GroupPnl {
                    cost_basis: Money::new(group.cost_basis, currency.clone()),
                    realized_cost_basis: Money::new(group.realized_cost_basis, currency.clone()),
                    market_value: Money::new(group.market_value, currency.clone()),
                    unrealized_pnl: Money::new(group.unrealized, currency.clone()),
                    realized_pnl: Money::new(group.realized, currency.clone()),
                    income: Money::new(group.income, currency.clone()),
                    total_pnl: Money::new(group_total, currency.clone()),
                    roi_pct,
                    weight_pct,
                },
            );
        }

        Ok(PnlSnapshot {
            timeframe,
            as_of,
            currency: currency.clone(),
            total_cost_basis: Money::new(totals.cost_basis, currency.clone()),
            total_realized_cost_basis: Money::new(totals.realized_cost_basis, currency.clone()),
            total_market_value: Money::new(totals.market_value, currency.clone()),
            total_unrealized: Money::new(totals.unrealized, currency.clone()),
            total_realized: Money::new(totals.realized, currency.clone()),
            total_income: Money::new(totals.income, currency.clone()),
            total_pnl: Money::new(total_pnl, currency.clone()),
            roi_pct,
            annualized_roi: calc_annualized(roi_pct, avg_days)?,
            positions,
            realized_events,
            by_class: by_class_out,
        })
    }
}

fn average_holding_days(positions: &[PositionPnl]) -> Result<i64, PnlError> {
    if positions.is_empty() {
        return Ok(0);
    }

    let mut total = 0_i64;
    for position in positions {
        total = total
            .checked_add(position.holding_days)
            .ok_or(PnlError::MathOverflow)?;
    }

    let len = i64::try_from(positions.len()).map_err(|_| PnlError::MathOverflow)?;
    total.checked_div(len).ok_or(PnlError::MathOverflow)
}
