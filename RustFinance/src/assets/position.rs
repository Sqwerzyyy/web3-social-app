use crate::assets::types::{AssetClass, Currency, Money, PositionId, Tag, Units};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub trait AssetPosition: Send + Sync {
    fn id(&self)                      -> PositionId;
    fn asset_class(&self)             -> AssetClass;
    fn symbol(&self)                  -> &str;
    fn units(&self)                   -> Units;
    fn purchase_date(&self)           -> DateTime<Utc>;
    fn currency(&self)                -> Currency;
    fn tags(&self)                    -> &HashSet<Tag>;
    fn purchase_price_per_unit(&self) -> Decimal;

    fn checked_cost_basis(&self) -> Option<Money> {
        self.units()
            .0
            .checked_mul(self.purchase_price_per_unit())
            .map(|amount| Money::new(amount, self.currency()))
    }

    fn cost_basis(&self) -> Money {
        self.checked_cost_basis()
            .unwrap_or_else(|| Money::zero(self.currency()))
    }

    fn accumulated_income(&self) -> Money {
        Money::zero(self.currency())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RwaPosition {
    pub id:                 PositionId,
    pub token_address:      String,
    pub symbol:             String,
    pub total_supply:       u64,
    pub units:              Units,
    pub purchase_price_usd: Decimal,
    pub rental_income_usd:  Decimal,
    pub purchase_date:      DateTime<Utc>,
    pub tags:               HashSet<Tag>,
}

impl AssetPosition for RwaPosition {
    fn id(&self)                      -> PositionId    { self.id }
    fn asset_class(&self)             -> AssetClass    { AssetClass::RWA }
    fn symbol(&self)                  -> &str          { &self.symbol }
    fn units(&self)                   -> Units         { self.units }
    fn purchase_date(&self)           -> DateTime<Utc> { self.purchase_date }
    fn currency(&self)                -> Currency      { Currency::usd() }
    fn tags(&self)                    -> &HashSet<Tag> { &self.tags }
    fn purchase_price_per_unit(&self) -> Decimal       { self.purchase_price_usd }

    fn accumulated_income(&self) -> Money {
        Money::new(self.rental_income_usd, Currency::usd())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPosition {
    pub id:             PositionId,
    pub ticker:         String,
    pub exchange:       String,
    pub units:          Units,
    pub purchase_price: Decimal,
    pub currency:       Currency,
    pub dividends:      Decimal,
    pub purchase_date:  DateTime<Utc>,
    pub tags:           HashSet<Tag>,
}

impl AssetPosition for EquityPosition {
    fn id(&self)                      -> PositionId    { self.id }
    fn asset_class(&self)             -> AssetClass    { AssetClass::Equity }
    fn symbol(&self)                  -> &str          { &self.ticker }
    fn units(&self)                   -> Units         { self.units }
    fn purchase_date(&self)           -> DateTime<Utc> { self.purchase_date }
    fn currency(&self)                -> Currency      { self.currency.clone() }
    fn tags(&self)                    -> &HashSet<Tag> { &self.tags }
    fn purchase_price_per_unit(&self) -> Decimal       { self.purchase_price }

    fn accumulated_income(&self) -> Money {
        Money::new(self.dividends, self.currency.clone())
    }
}
