use chrono::{Duration, Utc};
use pnl_engine::{
    assets::position::{EquityPosition, RwaPosition},
    AssetClass, AssetPosition, Currency, InMemoryPriceProvider, PnlEngine, PositionFilter,
    PositionId, Tag, Timeframe, Units,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let positions: Vec<Box<dyn AssetPosition>> = vec![
        Box::new(RwaPosition {
            id: PositionId::new(),
            token_address: "0xA1B2C3".into(),
            symbol: "REALTY-TOKEN".into(),
            total_supply: 1_000_000,
            units: units(dec!(500))?,
            purchase_price_usd: dec!(100),
            rental_income_usd: dec!(2500),
            purchase_date: Utc::now() - Duration::days(365),
            tags: [Tag::new("real-estate")].into(),
        }),
        Box::new(RwaPosition {
            id: PositionId::new(),
            token_address: "0xD4E5F6".into(),
            symbol: "DUBAI-OFFICE".into(),
            total_supply: 500_000,
            units: units(dec!(200))?,
            purchase_price_usd: dec!(250),
            rental_income_usd: dec!(4000),
            purchase_date: Utc::now() - Duration::days(90),
            tags: [Tag::new("real-estate"), Tag::new("uae")].into(),
        }),
        Box::new(EquityPosition {
            id: PositionId::new(),
            ticker: "AAPL".into(),
            exchange: "NASDAQ".into(),
            units: units(dec!(50))?,
            purchase_price: dec!(150),
            currency: Currency::usd(),
            dividends: dec!(120),
            purchase_date: Utc::now() - Duration::days(400),
            tags: [Tag::new("us-tech")].into(),
        }),
        Box::new(EquityPosition {
            id: PositionId::new(),
            ticker: "MSFT".into(),
            exchange: "NASDAQ".into(),
            units: units(dec!(20))?,
            purchase_price: dec!(300),
            currency: Currency::usd(),
            dividends: dec!(80),
            purchase_date: Utc::now() - Duration::days(200),
            tags: [Tag::new("us-tech")].into(),
        }),
    ];

    let mut pp = InMemoryPriceProvider::new();
    pp.set("REALTY-TOKEN", &Currency::usd(), dec!(118));
    pp.set("DUBAI-OFFICE", &Currency::usd(), dec!(280));
    pp.set("AAPL", &Currency::usd(), dec!(185));
    pp.set("MSFT", &Currency::usd(), dec!(425));

    let engine = PnlEngine::new(pp);
    let usd = Currency::usd();

    let snap = engine
        .compute(
            &positions,
            &PositionFilter::new(),
            &Timeframe::AllTime,
            &usd,
        )
        .await?;
    println!("{}", snap);

    let snap = engine
        .compute(
            &positions,
            &PositionFilter::new().asset_class(AssetClass::RWA),
            &Timeframe::Last(90),
            &usd,
        )
        .await?;
    println!("\n{}", snap);

    let snap = engine
        .compute(
            &positions,
            &PositionFilter::new().tag("us-tech"),
            &Timeframe::Ytd,
            &usd,
        )
        .await?;
    println!("\n{}", snap);

    Ok(())
}

fn units(value: Decimal) -> anyhow::Result<Units> {
    Units::new(value).ok_or_else(|| anyhow::anyhow!("units must be positive"))
}
