use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Currency(String);

impl Currency {
    pub fn usd() -> Self { Self("USD".into()) }
    pub fn eur() -> Self { Self("EUR".into()) }

    pub fn new(ticker: impl AsRef<str>) -> Option<Self> {
        let s = ticker.as_ref().trim().to_uppercase();
        if s.len() >= 2 && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            Some(Self(s))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetClass {
    RWA,
    Equity,
}

impl fmt::Display for AssetClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RWA    => write!(f, "RWA"),
            Self::Equity => write!(f, "Equity"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tag(pub String);

impl Tag {
    pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PositionId(pub Uuid);

impl PositionId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

impl Default for PositionId {
    fn default() -> Self { Self::new() }
}

impl fmt::Display for PositionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Units(pub Decimal);

impl Units {
    pub fn new(d: Decimal) -> Option<Self> {
        if d > Decimal::ZERO { Some(Self(d)) } else { None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub value:     Decimal,
    pub currency:  Currency,
    pub timestamp: DateTime<Utc>,
}

impl Price {
    pub fn new(value: Decimal, currency: Currency, timestamp: DateTime<Utc>) -> Option<Self> {
        if value > Decimal::ZERO { Some(Self { value, currency, timestamp }) } else { None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount:   Decimal,
    pub currency: Currency,
}

impl Money {
    pub fn zero(currency: Currency) -> Self { Self { amount: Decimal::ZERO, currency } }
    pub fn new(amount: Decimal, currency: Currency) -> Self { Self { amount, currency } }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} {}", self.amount, self.currency)
    }
}

pub const BPS: Decimal = dec!(10_000);
pub const DAYS_YEAR: Decimal = dec!(365);
