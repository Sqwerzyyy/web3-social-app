use crate::assets::{AssetClass, AssetPosition, Currency, Tag};
use chrono::{DateTime, Datelike, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Timeframe {
    Last(u32),
    Range {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
    Ytd,
    AllTime,
}

impl Timeframe {
    pub fn resolve(
        &self,
        now: DateTime<Utc>,
        earliest: Option<DateTime<Utc>>,
    ) -> (DateTime<Utc>, DateTime<Utc>) {
        match self {
            Self::Last(days) => (now - Duration::days(*days as i64), now),
            Self::Range { from, to } => (*from, *to),
            Self::Ytd => {
                let jan1 = now
                    .with_month(1)
                    .unwrap()
                    .with_day(1)
                    .unwrap()
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc();
                (jan1, now)
            }
            Self::AllTime => (earliest.unwrap_or(now), now),
        }
    }
}

impl std::fmt::Display for Timeframe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Last(1) => write!(f, "1D"),
            Self::Last(7) => write!(f, "7D"),
            Self::Last(30) => write!(f, "30D"),
            Self::Last(90) => write!(f, "90D"),
            Self::Last(d) => write!(f, "{}D", d),
            Self::Range { from, to } => {
                write!(f, "{} → {}", from.format("%Y-%m-%d"), to.format("%Y-%m-%d"))
            }
            Self::Ytd => write!(f, "YTD"),
            Self::AllTime => write!(f, "All-time"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PositionFilter {
    pub asset_class: Option<AssetClass>,
    pub tags: HashSet<Tag>,
    pub currency: Option<Currency>,
    pub min_cost_basis_usd: Option<rust_decimal::Decimal>,
    pub symbols: HashSet<String>,
}

impl PositionFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn asset_class(mut self, ac: AssetClass) -> Self {
        self.asset_class = Some(ac);
        self
    }

    pub fn tag(mut self, t: impl Into<String>) -> Self {
        self.tags.insert(Tag::new(t));
        self
    }

    pub fn symbol(mut self, s: impl Into<String>) -> Self {
        self.symbols.insert(s.into());
        self
    }

    pub fn currency(mut self, c: Currency) -> Self {
        self.currency = Some(c);
        self
    }

    pub fn min_cost_basis(mut self, v: rust_decimal::Decimal) -> Self {
        self.min_cost_basis_usd = Some(v);
        self
    }

    pub fn matches(&self, pos: &dyn AssetPosition) -> bool {
        if let Some(ref ac) = self.asset_class {
            if pos.asset_class() != *ac {
                return false;
            }
        }
        if !self.tags.is_empty() && !self.tags.iter().any(|t| pos.tags().contains(t)) {
            return false;
        }
        if let Some(ref cur) = self.currency {
            if pos.currency() != *cur {
                return false;
            }
        }
        if !self.symbols.is_empty() && !self.symbols.contains(pos.symbol()) {
            return false;
        }
        if let Some(min) = self.min_cost_basis_usd {
            if pos
                .checked_cost_basis()
                .is_none_or(|cost_basis| cost_basis.amount < min)
            {
                return false;
            }
        }
        true
    }
}
