use crate::assets::position::{EquityPosition, RwaPosition};
use crate::assets::{AssetClass, Currency, Tag, Units};
use crate::engine::{EngineError, PnlEngine, RealizedPnlEvent};
use crate::filters::{PositionFilter, Timeframe};
use crate::financial_engine::{AccountType, AssetKind, FinancialEngine};
use crate::price::InMemoryPriceProvider;
use crate::{AssetPosition, PositionId};
use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

    fn usd() -> Currency {
        Currency::usd()
    }

    fn prices() -> InMemoryPriceProvider {
        let mut pp = InMemoryPriceProvider::new();
        pp.set("REALTY-TOKEN", &usd(), dec!(120));
        pp.set("OFFICE-TOKEN", &usd(), dec!(95));
        pp.set("AAPL", &usd(), dec!(185));
        pp.set("MSFT", &usd(), dec!(420));
        pp
    }

    fn rwa(
        symbol: &str,
        units: Decimal,
        price: Decimal,
        rental: Decimal,
        days: i64,
        tags: &[&str],
    ) -> Box<dyn AssetPosition> {
        Box::new(RwaPosition {
            id: PositionId::new(),
            token_address: format!("0x{}", symbol),
            symbol: symbol.to_string(),
            total_supply: 1_000_000,
            units: Units::new(units).unwrap(),
            purchase_price_usd: price,
            rental_income_usd: rental,
            purchase_date: Utc::now() - Duration::days(days),
            tags: tags.iter().map(|t| Tag::new(*t)).collect(),
        })
    }

    fn equity(
        ticker: &str,
        units: Decimal,
        price: Decimal,
        divs: Decimal,
        days: i64,
        tags: &[&str],
    ) -> Box<dyn AssetPosition> {
        Box::new(EquityPosition {
            id: PositionId::new(),
            ticker: ticker.to_string(),
            exchange: "NASDAQ".into(),
            units: Units::new(units).unwrap(),
            purchase_price: price,
            currency: usd(),
            dividends: divs,
            purchase_date: Utc::now() - Duration::days(days),
            tags: tags.iter().map(|t| Tag::new(*t)).collect(),
        })
    }

    #[tokio::test]
    async fn totals_correct() {
        let positions = vec![
            rwa("REALTY-TOKEN", dec!(100), dec!(100), dec!(500), 365, &[]),
            rwa("OFFICE-TOKEN", dec!(50), dec!(100), dec!(200), 180, &[]),
            equity("AAPL", dec!(10), dec!(150), dec!(50), 400, &[]),
            equity("MSFT", dec!(5), dec!(300), dec!(30), 200, &[]),
        ];
        let snap = PnlEngine::new(prices())
            .compute(&positions, &PositionFilter::new(), &Timeframe::AllTime, &usd())
            .await
            .unwrap();

        assert_eq!(snap.total_cost_basis.amount, dec!(18_000));
        assert_eq!(snap.total_market_value.amount, dec!(20_700));
        assert_eq!(snap.total_unrealized.amount, dec!(2_700));
        assert_eq!(snap.total_realized.amount, Decimal::ZERO);
        assert_eq!(snap.total_income.amount, dec!(780));
        assert_eq!(snap.total_pnl.amount, dec!(3_480));
    }

    #[tokio::test]
    async fn realized_pnl_is_ledger_backed_and_separate() {
        let positions = vec![rwa(
            "REALTY-TOKEN",
            dec!(100),
            dec!(100),
            Decimal::ZERO,
            365,
            &[],
        )];

        let ledger = FinancialEngine::new();
        let usd_asset = ledger.register_asset("USD", AssetKind::Fiat, 2).unwrap();
        let system = ledger
            .create_account_with_type(None, usd_asset.id, AccountType::System)
            .unwrap();
        let user_account = ledger
            .create_account(uuid::Uuid::new_v4(), usd_asset.id)
            .unwrap();
        let tx_id = ledger
            .execute_transfer(system.id, user_account.id, dec!(130.00))
            .unwrap();
        let tx = ledger.transaction(tx_id).unwrap();
        let event = RealizedPnlEvent::from_posted_ledger(
            &tx,
            "REALTY-TOKEN",
            AssetClass::RWA,
            usd(),
            dec!(130),
            dec!(100),
        )
        .unwrap();

        let snap = PnlEngine::new(prices())
            .compute_with_realized(
                &positions,
                &[event],
                &PositionFilter::new(),
                &Timeframe::AllTime,
                &usd(),
            )
            .await
            .unwrap();

        assert_eq!(snap.total_unrealized.amount, dec!(2_000));
        assert_eq!(snap.total_realized.amount, dec!(30));
        assert_eq!(snap.total_income.amount, Decimal::ZERO);
        assert_eq!(snap.total_pnl.amount, dec!(2_030));
        assert_eq!(snap.positions[0].unrealized_pnl.amount, dec!(2_000));
        assert_eq!(snap.positions[0].realized_pnl.amount, dec!(30));
    }

    #[tokio::test]
    async fn filter_by_class() {
        let positions = vec![
            rwa("REALTY-TOKEN", dec!(100), dec!(100), Decimal::ZERO, 365, &[]),
            equity("AAPL", dec!(10), dec!(150), Decimal::ZERO, 200, &[]),
        ];
        let snap = PnlEngine::new(prices())
            .compute(
                &positions,
                &PositionFilter::new().asset_class(AssetClass::RWA),
                &Timeframe::AllTime,
                &usd(),
            )
            .await
            .unwrap();

        assert_eq!(snap.positions.len(), 1);
        assert_eq!(snap.positions[0].symbol, "REALTY-TOKEN");
    }

    #[tokio::test]
    async fn filter_by_tag() {
        let positions = vec![
            rwa(
                "REALTY-TOKEN",
                dec!(100),
                dec!(100),
                Decimal::ZERO,
                365,
                &["real-estate"],
            ),
            equity("AAPL", dec!(10), dec!(150), Decimal::ZERO, 200, &["us-tech"]),
        ];
        let snap = PnlEngine::new(prices())
            .compute(
                &positions,
                &PositionFilter::new().tag("real-estate"),
                &Timeframe::AllTime,
                &usd(),
            )
            .await
            .unwrap();

        assert_eq!(snap.positions.len(), 1);
    }

    #[tokio::test]
    async fn filter_by_symbol() {
        let positions = vec![
            equity("AAPL", dec!(10), dec!(150), dec!(50), 200, &[]),
            equity("MSFT", dec!(5), dec!(300), dec!(30), 200, &[]),
        ];
        let snap = PnlEngine::new(prices())
            .compute(
                &positions,
                &PositionFilter::new().symbol("MSFT"),
                &Timeframe::AllTime,
                &usd(),
            )
            .await
            .unwrap();

        assert_eq!(snap.positions.len(), 1);
        assert_eq!(snap.total_cost_basis.amount, dec!(1500));
        assert_eq!(snap.total_market_value.amount, dec!(2100));
    }

    #[tokio::test]
    async fn roi_one_year() {
        let positions = vec![rwa(
            "REALTY-TOKEN",
            dec!(100),
            dec!(100),
            Decimal::ZERO,
            365,
            &[],
        )];
        let snap = PnlEngine::new(prices())
            .compute(&positions, &PositionFilter::new(), &Timeframe::AllTime, &usd())
            .await
            .unwrap();

        assert_eq!(snap.positions[0].roi_pct, dec!(20));
        assert_eq!(snap.positions[0].annualized_roi.round_dp(2), dec!(20.00));
    }

    #[tokio::test]
    async fn math_overflow_is_reported() {
        let mut pp = InMemoryPriceProvider::new();
        pp.set("REALTY-TOKEN", &usd(), dec!(2));

        let positions = vec![rwa(
            "REALTY-TOKEN",
            Decimal::MAX,
            dec!(1),
            Decimal::ZERO,
            365,
            &[],
        )];
        let result = PnlEngine::new(pp)
            .compute(&positions, &PositionFilter::new(), &Timeframe::AllTime, &usd())
            .await;

        assert!(matches!(result, Err(EngineError::MathOverflow)));
    }

    #[tokio::test]
    async fn pnl_engine_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PnlEngine<InMemoryPriceProvider>>();
    }

    #[tokio::test]
    async fn no_positions_error() {
        let positions = vec![rwa(
            "REALTY-TOKEN",
            dec!(100),
            dec!(100),
            Decimal::ZERO,
            365,
            &[],
        )];
        let result = PnlEngine::new(prices())
            .compute(
                &positions,
                &PositionFilter::new().asset_class(AssetClass::Equity),
                &Timeframe::AllTime,
                &usd(),
            )
            .await;

        assert!(matches!(result, Err(EngineError::NoPositions)));
    }
