use super::*;
use chrono::{TimeZone, Utc};
use rust_decimal_macros::dec;
use uuid::Uuid;

#[derive(Clone)]
struct FixedClock(DateTime<Utc>);

impl LedgerClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn engine() -> FinancialEngine<FixedClock> {
    let now = Utc.with_ymd_and_hms(2026, 5, 19, 12, 0, 0).unwrap();
    FinancialEngine::with_clock(FixedClock(now))
}

fn user() -> Uuid {
    Uuid::new_v4()
}

#[test]
fn successful_transfer_posts_balanced_entries() {
    let engine = engine();
    let usd = engine.register_asset("USD", AssetKind::Fiat, 2).unwrap();
    let system = engine
        .create_account_with_type(None, usd.id, AccountType::System)
        .unwrap();
    let alice = engine.create_account(user(), usd.id).unwrap();
    let bob = engine.create_account(user(), usd.id).unwrap();

    engine
        .execute_transfer(system.id, alice.id, dec!(100.00))
        .unwrap();
    let tx_id = engine
        .execute_transfer(alice.id, bob.id, dec!(25.50))
        .unwrap();

    let tx = engine.transaction(tx_id).unwrap();
    assert_eq!(tx.entries.len(), 2);
    assert_eq!(engine.account_balance(alice.id).unwrap(), dec!(74.50));
    assert_eq!(engine.account_balance(bob.id).unwrap(), dec!(25.50));
    assert_eq!(
        engine.total_balance_for_asset(usd.id).unwrap(),
        Decimal::ZERO
    );
    engine.verify_ledger().unwrap();
}

#[tokio::test]
async fn double_spend_allows_only_one_debit() {
    let engine = engine();
    let usd = engine.register_asset("USD", AssetKind::Fiat, 2).unwrap();
    let system = engine
        .create_account_with_type(None, usd.id, AccountType::System)
        .unwrap();
    let alice = engine.create_account(user(), usd.id).unwrap();
    let bob = engine.create_account(user(), usd.id).unwrap();
    let charlie = engine.create_account(user(), usd.id).unwrap();

    engine
        .execute_transfer(system.id, alice.id, dec!(100.00))
        .unwrap();

    let first = tokio::spawn({
        let engine = engine.clone();
        async move { engine.execute_transfer(alice.id, bob.id, dec!(80.00)) }
    });
    let second = tokio::spawn({
        let engine = engine.clone();
        async move { engine.execute_transfer(alice.id, charlie.id, dec!(80.00)) }
    });

    let results = [first.await.unwrap(), second.await.unwrap()];
    let ok_count = results.iter().filter(|result| result.is_ok()).count();
    let insufficient_count = results
        .iter()
        .filter(|result| matches!(result, Err(FinancialError::InsufficientFunds { .. })))
        .count();

    let recipient_total = engine
        .account_balance(bob.id)
        .unwrap()
        .checked_add(engine.account_balance(charlie.id).unwrap())
        .unwrap();

    assert_eq!(ok_count, 1);
    assert_eq!(insufficient_count, 1);
    assert_eq!(engine.account_balance(alice.id).unwrap(), dec!(20.00));
    assert_eq!(recipient_total, dec!(80.00));
    engine.verify_ledger().unwrap();
}

#[test]
fn transfer_with_insufficient_balance_is_rejected() {
    let engine = engine();
    let usd = engine.register_asset("USD", AssetKind::Fiat, 2).unwrap();
    let system = engine
        .create_account_with_type(None, usd.id, AccountType::System)
        .unwrap();
    let alice = engine.create_account(user(), usd.id).unwrap();
    let bob = engine.create_account(user(), usd.id).unwrap();

    engine
        .execute_transfer(system.id, alice.id, dec!(10.00))
        .unwrap();
    let result = engine.execute_transfer(alice.id, bob.id, dec!(10.01));

    assert!(matches!(
        result,
        Err(FinancialError::InsufficientFunds { .. })
    ));
    assert_eq!(engine.account_balance(alice.id).unwrap(), dec!(10.00));
    assert_eq!(engine.account_balance(bob.id).unwrap(), Decimal::ZERO);
    engine.verify_ledger().unwrap();
}

#[tokio::test]
async fn parallel_transfers_preserve_asset_zero_sum() {
    let engine = engine();
    let usd = engine.register_asset("USD", AssetKind::Fiat, 2).unwrap();
    let system = engine
        .create_account_with_type(None, usd.id, AccountType::System)
        .unwrap();
    let alice = engine.create_account(user(), usd.id).unwrap();
    let bob = engine.create_account(user(), usd.id).unwrap();

    engine
        .execute_transfer(system.id, alice.id, dec!(1000.00))
        .unwrap();

    let mut handles = Vec::new();
    for _ in 0..100 {
        handles.push(tokio::spawn({
            let engine = engine.clone();
            async move { engine.execute_transfer(alice.id, bob.id, dec!(1.00)) }
        }));
    }

    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    assert_eq!(engine.account_balance(alice.id).unwrap(), dec!(900.00));
    assert_eq!(engine.account_balance(bob.id).unwrap(), dec!(100.00));
    assert_eq!(
        engine.total_balance_for_asset(usd.id).unwrap(),
        Decimal::ZERO
    );
    engine.verify_ledger().unwrap();
}

#[test]
fn lock_funds_moves_user_balance_to_escrow() {
    let engine = engine();
    let usd = engine.register_asset("USD", AssetKind::Fiat, 2).unwrap();
    let alice_id = user();
    let system = engine
        .create_account_with_type(None, usd.id, AccountType::System)
        .unwrap();
    let alice = engine.create_account(alice_id, usd.id).unwrap();
    let escrow = engine
        .create_account_with_type(Some(alice_id), usd.id, AccountType::Escrow)
        .unwrap();

    engine
        .execute_transfer(system.id, alice.id, dec!(100.00))
        .unwrap();
    engine.lock_funds(alice.id, escrow.id, dec!(40.00)).unwrap();

    assert_eq!(engine.account_balance(alice.id).unwrap(), dec!(60.00));
    assert_eq!(engine.account_balance(escrow.id).unwrap(), dec!(40.00));
    engine.verify_ledger().unwrap();
}

#[test]
fn rwa_mint_settlement_moves_escrow_and_issues_tokens() {
    let engine = engine();
    let usd = engine.register_asset("USD", AssetKind::Fiat, 2).unwrap();
    let rwa = engine
        .register_asset("RWA-1", AssetKind::RwaToken, 18)
        .unwrap();
    let alice_id = user();
    let system = engine
        .create_account_with_type(None, usd.id, AccountType::System)
        .unwrap();
    let alice_usd = engine.create_account(alice_id, usd.id).unwrap();
    let escrow = engine
        .create_account_with_type(Some(alice_id), usd.id, AccountType::Escrow)
        .unwrap();
    let alice_rwa = engine.create_account(alice_id, rwa.id).unwrap();

    engine
        .execute_transfer(system.id, alice_usd.id, dec!(1000.00))
        .unwrap();
    engine
        .lock_funds(alice_usd.id, escrow.id, dec!(250.00))
        .unwrap();
    let tx_id = engine
        .settle_rwa_mint(alice_id, rwa.id, dec!(12.34567890123456789), dec!(250.00))
        .unwrap();

    let tx = engine.transaction(tx_id).unwrap();
    assert_eq!(tx.kind, TransactionKind::RwaMintSettlement);
    assert_eq!(tx.entries.len(), 4);
    assert_eq!(engine.account_balance(escrow.id).unwrap(), Decimal::ZERO);
    assert_eq!(
        engine.account_balance(alice_rwa.id).unwrap(),
        dec!(12.34567890123456789)
    );
    assert_eq!(
        engine.total_balance_for_asset(usd.id).unwrap(),
        Decimal::ZERO
    );
    assert_eq!(
        engine.total_balance_for_asset(rwa.id).unwrap(),
        Decimal::ZERO
    );
    engine.verify_ledger().unwrap();
}
