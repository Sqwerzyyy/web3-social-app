use super::proto::financial_engine_server::FinancialEngine as FinancialEngineRpc;
use super::proto::{DecimalString, GetBalanceRequest, LockFundsRequest};
use super::*;
use crate::financial_engine::{AccountType, AssetKind, FinancialEngine};
use rust_decimal_macros::dec;

#[tokio::test]
async fn get_balance_returns_decimal_string() {
    let ledger = FinancialEngine::new();
    let usd = ledger.register_asset("USD", AssetKind::Fiat, 2).unwrap();
    let system = ledger
        .create_account_with_type(None, usd.id, AccountType::System)
        .unwrap();
    let user = ledger.create_account(Uuid::new_v4(), usd.id).unwrap();
    ledger
        .execute_transfer(system.id, user.id, dec!(10.50))
        .unwrap();

    let bridge = FinancialGrpcBridge::new(ledger);
    let response = bridge
        .get_balance(Request::new(GetBalanceRequest {
            account_id: user.id.0,
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.balance.unwrap().value, "10.5");
}

#[tokio::test]
async fn lock_funds_is_idempotent() {
    let ledger = FinancialEngine::new();
    let usd = ledger.register_asset("USD", AssetKind::Fiat, 2).unwrap();
    let user_id = Uuid::new_v4();
    let system = ledger
        .create_account_with_type(None, usd.id, AccountType::System)
        .unwrap();
    let user = ledger.create_account(user_id, usd.id).unwrap();
    let escrow = ledger
        .create_account_with_type(Some(user_id), usd.id, AccountType::Escrow)
        .unwrap();
    ledger
        .execute_transfer(system.id, user.id, dec!(50.00))
        .unwrap();

    let bridge = FinancialGrpcBridge::new(ledger.clone());
    let key = Uuid::new_v4().to_string();
    let request = LockFundsRequest {
        idempotency_key: key,
        from_account_id: user.id.0,
        escrow_account_id: escrow.id.0,
        amount: Some(DecimalString {
            value: "25.00".to_string(),
        }),
    };

    let first = bridge
        .lock_funds(Request::new(request.clone()))
        .await
        .unwrap()
        .into_inner();
    let replay = bridge
        .lock_funds(Request::new(request))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(first.tx_id, replay.tx_id);
    assert!(!first.idempotent_replay);
    assert!(replay.idempotent_replay);
    assert_eq!(ledger.account_balance(user.id).unwrap(), dec!(25.00));
    assert_eq!(ledger.account_balance(escrow.id).unwrap(), dec!(25.00));
}
