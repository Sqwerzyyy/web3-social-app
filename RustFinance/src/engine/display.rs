use super::{PnlSnapshot, PositionPnl};
use std::fmt;

impl fmt::Display for PositionPnl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  {:<14} {:>10.2} {:>12.2} {:>+11.2} {:>+11.2} {:>+9.2}% {:>6}d",
            self.symbol,
            self.cost_basis.amount,
            self.market_value.amount,
            self.unrealized_pnl.amount,
            self.realized_pnl.amount,
            self.total_pnl.amount,
            self.holding_days,
        )
    }
}

impl fmt::Display for PnlSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let line = "─".repeat(75);
        let dline = "━".repeat(75);

        writeln!(f, "{}", dline)?;
        writeln!(
            f,
            "  PnL  │  {}  │  {}  │  {}",
            self.timeframe,
            self.as_of.format("%Y-%m-%d %H:%M UTC"),
            self.currency,
        )?;
        writeln!(f, "{}", dline)?;
        writeln!(
            f,
            "  {:<14} {:>10} {:>12} {:>11} {:>11} {:>9} {:>7}",
            "Symbol", "Cost basis", "Mkt value", "Unrealized", "Realized", "Total", "Days"
        )?;
        writeln!(f, "{}", line)?;

        let mut by_class: std::collections::HashMap<String, Vec<&PositionPnl>> =
            std::collections::HashMap::new();
        for p in &self.positions {
            by_class
                .entry(p.asset_class.to_string())
                .or_default()
                .push(p);
        }

        let mut classes: Vec<&String> = by_class.keys().collect();
        classes.sort();

        for class in classes {
            if let Some(grp) = self.by_class.get(class) {
                writeln!(f, "  ▸ {} ({:.1}% of portfolio)", class, grp.weight_pct)?;
            }
            let mut sorted = by_class[class].clone();
            sorted.sort_by(|a, b| b.market_value.amount.cmp(&a.market_value.amount));
            for p in sorted {
                writeln!(f, "{}", p)?;
            }
        }

        writeln!(f, "{}", line)?;
        writeln!(
            f,
            "  {:<14} {:>10.2} {:>12.2} {:>+11.2} {:>+11.2} {:>+9.2}",
            "TOTAL",
            self.total_cost_basis.amount,
            self.total_market_value.amount,
            self.total_unrealized.amount,
            self.total_realized.amount,
            self.total_pnl.amount,
        )?;
        writeln!(f, "{}", line)?;
        writeln!(
            f,
            "  Unrealized: {:>+.2} {}   Realized: {:>+.2} {}   Income: {:>+.2} {}",
            self.total_unrealized.amount,
            self.currency,
            self.total_realized.amount,
            self.currency,
            self.total_income.amount,
            self.currency,
        )?;
        write!(f, "{}", dline)
    }
}
