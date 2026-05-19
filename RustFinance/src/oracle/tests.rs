use super::*;
use rust_decimal_macros::dec;

#[tokio::test]
async fn consensus_accepts_close_sources() {
    let chainlink = Arc::new(ChainlinkPriceAdapter::new());
    let pyth = Arc::new(PythPriceAdapter::new());
    let asset_id = AssetId(7);
    chainlink.set_price(asset_id, dec!(100.00), 1_000).unwrap();
    pyth.set_price(asset_id, dec!(101.00), 1_000).unwrap();

    let oracle = OracleAggregator::new(600, 200)
        .with_source(chainlink)
        .with_source(pyth);
    let feed = oracle.consensus_price(asset_id, 1_010).await.unwrap();

    assert_eq!(feed.price, dec!(100.50));
}

#[tokio::test]
async fn stale_data_halts_risk_engine() {
    let chainlink = Arc::new(ChainlinkPriceAdapter::new());
    let asset_id = AssetId(7);
    chainlink.set_price(asset_id, dec!(100.00), 1_000).unwrap();

    let oracle = Arc::new(OracleAggregator::new(10, 200).with_source(chainlink));
    let risk = RiskEngine::new(oracle);
    let result = risk.guarded_price(asset_id, 1_020).await;

    assert!(matches!(result, Err(OracleError::StaleData { .. })));
    assert_eq!(risk.mode().unwrap(), RiskMode::Halt);
    assert!(matches!(
        risk.ensure_trading_open(),
        Err(OracleError::Halted)
    ));
}

#[tokio::test]
async fn outlier_rejects_consensus() {
    let chainlink = Arc::new(ChainlinkPriceAdapter::new());
    let pyth = Arc::new(PythPriceAdapter::new());
    let asset_id = AssetId(7);
    chainlink.set_price(asset_id, dec!(100.00), 1_000).unwrap();
    pyth.set_price(asset_id, dec!(104.00), 1_000).unwrap();

    let oracle = OracleAggregator::new(600, 200)
        .with_source(chainlink)
        .with_source(pyth);
    let result = oracle.consensus_price(asset_id, 1_010).await;

    assert!(matches!(result, Err(OracleError::Outlier { .. })));
}

#[test]
fn twap_uses_time_weights() {
    let oracle = OracleAggregator::default();
    let asset_id = AssetId(7);
    oracle
        .push_history(PriceFeed {
            asset_id,
            price: dec!(100),
            timestamp: 100,
            source: "test".to_string(),
        })
        .unwrap();
    oracle
        .push_history(PriceFeed {
            asset_id,
            price: dec!(110),
            timestamp: 700,
            source: "test".to_string(),
        })
        .unwrap();

    let feed = oracle.twap(asset_id, 1_000, 900).unwrap();
    assert_eq!(feed.price.round_dp(4), dec!(103.3333));
}
