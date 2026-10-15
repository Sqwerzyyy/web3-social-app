use super::*;

fn addr(byte: u8) -> Address {
    Address::new([byte; 20])
}

fn tx_hash(byte: u8) -> TxHash {
    TxHash::new([byte; 32])
}

fn pending(to: Option<Address>, calldata: Vec<u8>) -> PendingTransaction {
    PendingTransaction {
        hash: tx_hash(9),
        from: addr(1),
        to,
        value: Word::zero(),
        calldata,
        gas_price: Word::from_u64(1),
        nonce: Some(7),
    }
}

fn trace(flags: TraceFlags) -> ExecutionTrace {
    ExecutionTrace {
        tx_hash: tx_hash(9),
        entrypoint: Some(addr(2)),
        call_stack: vec![CallFrame {
            depth: 0,
            caller: addr(1),
            callee: addr(2),
            value: Word::zero(),
            selector: Some([0xde, 0xad, 0xbe, 0xef]),
            kind: CallKind::Call,
        }],
        state_diffs: vec![StateSlotDiff {
            contract: addr(2),
            slot: Word::from_u64(11),
            before: Word::zero(),
            after: Word::from_u64(1),
        }],
        erc20_transfers: vec![TokenTransfer {
            token: addr(3),
            from: addr(4),
            to: addr(5),
            amount: Word::from_u64(100),
        }],
        balance_deltas: Vec::new(),
        flags,
        gas_used: 42_000,
    }
}

struct FixedSimulator {
    trace: ExecutionTrace,
}

#[async_trait]
impl TraceSimulator for FixedSimulator {
    async fn simulate(
        &self,
        _tx: &PendingTransaction,
    ) -> Result<ExecutionTrace, PreExecutionError> {
        Ok(self.trace.clone())
    }
}

#[test]
fn address_book_filters_direct_and_calldata_hits() {
    let rwa = addr(2);
    let pool = addr(8);
    let book = AddressBook::new([
        (rwa, TrackedSurface::RwaContract),
        (pool, TrackedSurface::LiquidityPool),
    ]);

    let direct = book.prefilter(&pending(Some(rwa), Vec::new()));
    assert_eq!(direct.hits.len(), 1);
    assert_eq!(direct.hits[0].location, PrefilterLocation::To);

    let mut calldata = vec![0xaa; 12];
    calldata.extend_from_slice(pool.as_bytes());
    calldata.extend_from_slice(&[0xbb; 16]);
    let embedded = book.prefilter(&pending(Some(addr(7)), calldata));

    assert_eq!(embedded.hits.len(), 1);
    assert_eq!(embedded.hits[0].address, pool);
    assert_eq!(embedded.hits[0].location, PrefilterLocation::Calldata);

    let noise = book.prefilter(&pending(Some(addr(99)), vec![0xcc; 128]));
    assert!(!noise.should_simulate());
}

#[test]
fn bincode_codec_roundtrips_execution_trace() {
    let mut flags = TraceFlags::empty();
    flags.insert(TraceFlags::FLASH_LOAN_PATTERN);
    flags.insert(TraceFlags::ORACLE_TOUCH);
    let trace = trace(flags);

    let codec = BincodeTraceCodec;
    let encoded = codec.encode(&trace).unwrap();
    let decoded = codec.decode(&encoded).unwrap();

    assert_eq!(decoded, trace);
}

#[tokio::test]
async fn pipeline_blocks_malicious_trace_and_dispatches_alert() {
    let mut flags = TraceFlags::empty();
    flags.insert(TraceFlags::FLASH_LOAN_PATTERN);
    flags.insert(TraceFlags::ORACLE_TOUCH);

    let book = AddressBook::new([(addr(2), TrackedSurface::RwaContract)]);
    let simulator = Arc::new(FixedSimulator {
        trace: trace(flags),
    });
    let analyzer = Arc::new(HeuristicTraceAnalyzer::new());
    let (tx_alert, mut rx_alert) = tokio::sync::mpsc::channel(1);
    let responder = Arc::new(ChannelThreatResponder::new(tx_alert));
    let pipeline = PreExecutionPipeline::new(book, simulator, analyzer, responder);

    let outcome = pipeline
        .process_transaction(pending(Some(addr(2)), Vec::new()))
        .await
        .unwrap();

    let ProcessOutcome::Blocked { risk, .. } = outcome else {
        panic!("expected malicious transaction to be blocked");
    };
    assert!(risk.malicious);
    assert!(risk.score > risk.critical_score);

    let alert = rx_alert.recv().await.unwrap();
    assert_eq!(alert.counter_measures, vec![CounterMeasure::Killswitch]);
    assert_eq!(alert.prefilter_hits.len(), 1);
}

#[tokio::test]
async fn pipeline_runs_pending_stream_concurrently() {
    let mut flags = TraceFlags::empty();
    flags.insert(TraceFlags::ORACLE_TOUCH);

    let book = AddressBook::new([(addr(2), TrackedSurface::Oracle)]);
    let simulator = Arc::new(FixedSimulator {
        trace: trace(flags),
    });
    let analyzer = Arc::new(HeuristicTraceAnalyzer::new());
    let responder = Arc::new(NoopThreatResponder);
    let config = PreExecutionConfig {
        critical_score: 90,
        max_in_flight: 2,
        ..PreExecutionConfig::default()
    };
    let pipeline = PreExecutionPipeline::with_config(
        book,
        simulator,
        analyzer,
        Arc::new(BincodeTraceCodec),
        responder,
        config,
    );
    let stream = tokio_stream::iter(vec![
        pending(Some(addr(2)), Vec::new()),
        pending(Some(addr(2)), Vec::new()),
        pending(Some(addr(99)), Vec::new()),
    ]);

    pipeline.run_stream(stream).await.unwrap();
}
