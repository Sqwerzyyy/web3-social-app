use super::*;
use rust_decimal_macros::dec;

fn event(block_number: u64, block_hash: &str, tx_hash: &str) -> IndexedContractEvent {
    IndexedContractEvent {
        id: EventId {
            block_number,
            tx_hash: tx_hash.to_string(),
            log_index: 0,
        },
        block_hash: block_hash.to_string(),
        contract_address: "0xcontract".to_string(),
        kind: ContractEventKind::Mint,
        from: None,
        to: Some("0xuser".to_string()),
        amount: dec!(10),
    }
}

#[test]
fn pending_events_confirm_after_required_depth() {
    let mut buffer = ReorgBuffer::new(12);
    buffer.ingest_pending(event(100, "0xaaa", "0xtx")).unwrap();

    assert!(buffer.confirm_ready(111).is_empty());
    let confirmed = buffer.confirm_ready(112);

    assert_eq!(confirmed.len(), 1);
    assert_eq!(confirmed[0].status, IndexedEventStatus::Confirmed);
}

#[test]
fn reorg_removes_orphaned_pending_events() {
    let mut buffer = ReorgBuffer::new(12);
    buffer.ingest_pending(event(100, "0xaaa", "0xtx1")).unwrap();
    buffer.ingest_pending(event(101, "0xbbb", "0xtx2")).unwrap();

    let removed = buffer
        .ingest_pending(event(100, "0xccc", "0xtx3"))
        .unwrap()
        .unwrap();

    assert_eq!(removed.len(), 2);
    assert_eq!(buffer.pending_len(), 1);
}
