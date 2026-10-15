#[cfg(test)]
mod tests;

#[cfg(feature = "evm-alloy")]
pub mod analyzer;
#[cfg(feature = "evm-alloy")]
pub mod ingestion;
#[cfg(feature = "evm-alloy")]
pub mod simulator;

#[cfg(feature = "evm-alloy")]
pub use analyzer::{
    analyze_observed_trace, analyze_trace, ObservedCall, ObservedCallKind, ObservedStorageWrite,
    ObservedTrace, RiskScore, RiskSignal, RiskStatus, TvlRegistry,
};
#[cfg(feature = "evm-alloy")]
pub use ingestion::{pending_from_alloy, spawn_ingestion, IngestionConfig, MempoolIngestionWorker};
#[cfg(feature = "evm-alloy")]
pub use simulator::{RevmSimulator, SimulationConfig, SimulationOutcome};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::task::JoinSet;
use tokio_stream::{Stream, StreamExt};

const ADDRESS_BLOOM_BITS: usize = 2048;
const ADDRESS_BLOOM_WORDS: usize = ADDRESS_BLOOM_BITS / 64;
const DEFAULT_CRITICAL_RISK_SCORE: u8 = 80;
const DEFAULT_LATENCY_BUDGET: Duration = Duration::from_millis(50);
const TRACE_PAYLOAD_VERSION: u16 = 1;

#[derive(Debug, Error)]
pub enum PreExecutionError {
    #[error("invalid hex length for {kind}: expected {expected}, got {actual}")]
    InvalidHexLength {
        kind: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("invalid hex character in {kind}")]
    InvalidHexCharacter { kind: &'static str },

    #[error("mempool ingestion failed: {0}")]
    Ingestion(String),

    #[error("trace simulation failed: {0}")]
    Simulation(String),

    #[error("trace codec encode failed: {0}")]
    CodecEncode(String),

    #[error("trace codec decode failed: {0}")]
    CodecDecode(String),

    #[error("WASM analyzer failed: {0}")]
    Analyzer(String),

    #[error("counter-measure dispatch failed: {0}")]
    CounterMeasure(String),

    #[error("pending stream worker failed: {0}")]
    Worker(String),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Address([u8; 20]);

impl Address {
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; 20])
    }

    pub fn from_hex(value: &str) -> Result<Self, PreExecutionError> {
        let bytes = parse_fixed_hex::<20>(value, "address")?;
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_hex(f, &self.0)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_hex(f, &self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TxHash([u8; 32]);

impl TxHash {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; 32])
    }

    pub fn from_hex(value: &str) -> Result<Self, PreExecutionError> {
        let bytes = parse_fixed_hex::<32>(value, "transaction hash")?;
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_hex(f, &self.0)
    }
}

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_hex(f, &self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Word([u8; 32]);

impl Word {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; 32])
    }

    pub fn from_u64(value: u64) -> Self {
        let mut bytes = [0; 32];
        bytes[24..].copy_from_slice(&value.to_be_bytes());
        Self(bytes)
    }

    pub fn from_u128(value: u128) -> Self {
        let mut bytes = [0; 32];
        bytes[16..].copy_from_slice(&value.to_be_bytes());
        Self(bytes)
    }

    pub fn from_hex(value: &str) -> Result<Self, PreExecutionError> {
        let bytes = parse_fixed_hex::<32>(value, "word")?;
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_hex(f, &self.0)
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_hex(f, &self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTransaction {
    pub hash: TxHash,
    pub from: Address,
    pub to: Option<Address>,
    pub value: Word,
    pub calldata: Vec<u8>,
    pub gas_price: Word,
    pub nonce: Option<u64>,
}

impl PendingTransaction {
    pub fn selector(&self) -> Option<[u8; 4]> {
        let bytes = self.calldata.get(..4)?;
        Some([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackedSurface {
    RwaContract,
    Escrow,
    Oracle,
    LiquidityPool,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrefilterLocation {
    To,
    Calldata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefilterHit {
    pub address: Address,
    pub surface: TrackedSurface,
    pub location: PrefilterLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefilterDecision {
    pub hits: Vec<PrefilterHit>,
}

impl PrefilterDecision {
    pub fn should_simulate(&self) -> bool {
        !self.hits.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressBook {
    tracked: HashMap<Address, TrackedSurface>,
    bloom: AddressBloom,
}

impl AddressBook {
    pub fn new(entries: impl IntoIterator<Item = (Address, TrackedSurface)>) -> Self {
        let mut tracked = HashMap::new();
        let mut bloom = AddressBloom::default();
        for (address, surface) in entries {
            tracked.insert(address, surface);
            bloom.insert(address);
        }
        Self { tracked, bloom }
    }

    pub fn insert(&mut self, address: Address, surface: TrackedSurface) {
        self.tracked.insert(address, surface);
        self.bloom.insert(address);
    }

    pub fn get(&self, address: &Address) -> Option<TrackedSurface> {
        self.tracked.get(address).copied()
    }

    pub fn prefilter(&self, tx: &PendingTransaction) -> PrefilterDecision {
        let mut hits = Vec::with_capacity(2);
        let mut seen = HashSet::new();

        if let Some(to) = tx.to {
            if self.bloom.might_contain(to) {
                if let Some(surface) = self.get(&to) {
                    hits.push(PrefilterHit {
                        address: to,
                        surface,
                        location: PrefilterLocation::To,
                    });
                    seen.insert(to);
                }
            }
        }

        if tx.calldata.len() >= 20 {
            for window in tx.calldata.windows(20) {
                let address = slice_to_address(window);
                if seen.contains(&address) || !self.bloom.might_contain(address) {
                    continue;
                }
                if let Some(surface) = self.get(&address) {
                    hits.push(PrefilterHit {
                        address,
                        surface,
                        location: PrefilterLocation::Calldata,
                    });
                    seen.insert(address);
                }
            }
        }

        PrefilterDecision { hits }
    }

    pub fn len(&self) -> usize {
        self.tracked.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracked.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AddressBloom {
    words: [u64; ADDRESS_BLOOM_WORDS],
}

impl Default for AddressBloom {
    fn default() -> Self {
        Self {
            words: [0; ADDRESS_BLOOM_WORDS],
        }
    }
}

impl AddressBloom {
    fn insert(&mut self, address: Address) {
        for bit in bloom_bits(address) {
            self.words[bit / 64] |= 1_u64 << (bit % 64);
        }
    }

    fn might_contain(&self, address: Address) -> bool {
        bloom_bits(address)
            .into_iter()
            .all(|bit| self.words[bit / 64] & (1_u64 << (bit % 64)) != 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallKind {
    Call,
    DelegateCall,
    StaticCall,
    CallCode,
    Create,
    Create2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallFrame {
    pub depth: u16,
    pub caller: Address,
    pub callee: Address,
    pub value: Word,
    pub selector: Option<[u8; 4]>,
    pub kind: CallKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSlotDiff {
    pub contract: Address,
    pub slot: Word,
    pub before: Word,
    pub after: Word,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenTransfer {
    pub token: Address,
    pub from: Address,
    pub to: Address,
    pub amount: Word,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceDelta {
    pub address: Address,
    pub token: Option<Address>,
    pub sign: i8,
    pub amount: Word,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceFlags(u16);

impl TraceFlags {
    pub const RECURSIVE_CALL: u16 = 1 << 0;
    pub const FLASH_LOAN_PATTERN: u16 = 1 << 1;
    pub const ORACLE_TOUCH: u16 = 1 << 2;
    pub const SYBIL_DUST: u16 = 1 << 3;
    pub const REENTRANT_WRITE: u16 = 1 << 4;

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    pub fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }

    pub fn contains(&self, flag: u16) -> bool {
        self.0 & flag != 0
    }

    pub fn bits(&self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub tx_hash: TxHash,
    pub entrypoint: Option<Address>,
    pub call_stack: Vec<CallFrame>,
    pub state_diffs: Vec<StateSlotDiff>,
    pub erc20_transfers: Vec<TokenTransfer>,
    pub balance_deltas: Vec<BalanceDelta>,
    pub flags: TraceFlags,
    pub gas_used: u64,
}

impl ExecutionTrace {
    pub fn touched_addresses(&self) -> Vec<Address> {
        let mut seen = HashSet::new();
        let mut addresses = Vec::new();
        for frame in &self.call_stack {
            push_unique_address(&mut seen, &mut addresses, frame.caller);
            push_unique_address(&mut seen, &mut addresses, frame.callee);
        }
        for diff in &self.state_diffs {
            push_unique_address(&mut seen, &mut addresses, diff.contract);
        }
        for transfer in &self.erc20_transfers {
            push_unique_address(&mut seen, &mut addresses, transfer.token);
            push_unique_address(&mut seen, &mut addresses, transfer.from);
            push_unique_address(&mut seen, &mut addresses, transfer.to);
        }
        for delta in &self.balance_deltas {
            push_unique_address(&mut seen, &mut addresses, delta.address);
            if let Some(token) = delta.token {
                push_unique_address(&mut seen, &mut addresses, token);
            }
        }
        addresses
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TracePayload {
    pub version: u16,
    pub trace: ExecutionTrace,
}

pub trait TraceCodec: Send + Sync {
    fn encode(&self, trace: &ExecutionTrace) -> Result<Vec<u8>, PreExecutionError>;
    fn decode(&self, payload: &[u8]) -> Result<ExecutionTrace, PreExecutionError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BincodeTraceCodec;

impl TraceCodec for BincodeTraceCodec {
    fn encode(&self, trace: &ExecutionTrace) -> Result<Vec<u8>, PreExecutionError> {
        let payload = TracePayload {
            version: TRACE_PAYLOAD_VERSION,
            trace: trace.clone(),
        };
        bincode::serde::encode_to_vec(payload, bincode::config::standard())
            .map_err(|error| PreExecutionError::CodecEncode(error.to_string()))
    }

    fn decode(&self, payload: &[u8]) -> Result<ExecutionTrace, PreExecutionError> {
        let (payload, _) = bincode::serde::decode_from_slice::<TracePayload, _>(
            payload,
            bincode::config::standard(),
        )
        .map_err(|error| PreExecutionError::CodecDecode(error.to_string()))?;
        if payload.version != TRACE_PAYLOAD_VERSION {
            return Err(PreExecutionError::CodecDecode(format!(
                "unsupported payload version {}",
                payload.version
            )));
        }
        Ok(payload.trace)
    }
}

#[async_trait]
pub trait TraceSimulator: Send + Sync {
    async fn simulate(&self, tx: &PendingTransaction) -> Result<ExecutionTrace, PreExecutionError>;
}

#[async_trait]
pub trait TraceAnalyzer: Send + Sync {
    async fn analyze_trace(&self, payload: &[u8]) -> Result<AnalyzerVerdict, PreExecutionError>;
}

#[async_trait]
pub trait ThreatResponder: Send + Sync {
    async fn on_malicious(&self, alert: ThreatAlert) -> Result<(), PreExecutionError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatKind {
    FlashLoan,
    OracleManipulation,
    SybilDust,
    Reentrancy,
    ProtectedSurface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzerVerdict {
    pub score: u8,
    pub threats: Vec<ThreatKind>,
}

impl AnalyzerVerdict {
    pub fn clean() -> Self {
        Self {
            score: 0,
            threats: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskProfile {
    pub score: u8,
    pub critical_score: u8,
    pub malicious: bool,
    pub threats: Vec<ThreatKind>,
    pub touched_addresses: Vec<Address>,
    pub flags: TraceFlags,
    pub latency_micros: u128,
    pub latency_exceeded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterMeasure {
    Killswitch,
    PauseContracts { priority_fee: Word },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreatAlert {
    pub tx: PendingTransaction,
    pub prefilter_hits: Vec<PrefilterHit>,
    pub risk: RiskProfile,
    pub trace: ExecutionTrace,
    pub counter_measures: Vec<CounterMeasure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreExecutionConfig {
    pub critical_score: u8,
    pub latency_budget: Duration,
    pub max_in_flight: usize,
    pub enable_admin_pause: bool,
    pub pause_priority_fee: Word,
}

impl Default for PreExecutionConfig {
    fn default() -> Self {
        Self {
            critical_score: DEFAULT_CRITICAL_RISK_SCORE,
            latency_budget: DEFAULT_LATENCY_BUDGET,
            max_in_flight: 1024,
            enable_admin_pause: false,
            pause_priority_fee: Word::from_u64(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessOutcome {
    Filtered { tx_hash: TxHash },
    Simulated { tx_hash: TxHash, risk: RiskProfile },
    Blocked { tx_hash: TxHash, risk: RiskProfile },
}

#[derive(Clone)]
pub struct PreExecutionPipeline {
    address_book: Arc<AddressBook>,
    simulator: Arc<dyn TraceSimulator>,
    analyzer: Arc<dyn TraceAnalyzer>,
    codec: Arc<dyn TraceCodec>,
    responder: Arc<dyn ThreatResponder>,
    config: PreExecutionConfig,
}

impl PreExecutionPipeline {
    pub fn new(
        address_book: AddressBook,
        simulator: Arc<dyn TraceSimulator>,
        analyzer: Arc<dyn TraceAnalyzer>,
        responder: Arc<dyn ThreatResponder>,
    ) -> Self {
        Self::with_config(
            address_book,
            simulator,
            analyzer,
            Arc::new(BincodeTraceCodec),
            responder,
            PreExecutionConfig::default(),
        )
    }

    pub fn with_config(
        address_book: AddressBook,
        simulator: Arc<dyn TraceSimulator>,
        analyzer: Arc<dyn TraceAnalyzer>,
        codec: Arc<dyn TraceCodec>,
        responder: Arc<dyn ThreatResponder>,
        config: PreExecutionConfig,
    ) -> Self {
        Self {
            address_book: Arc::new(address_book),
            simulator,
            analyzer,
            codec,
            responder,
            config,
        }
    }

    pub fn address_book(&self) -> &AddressBook {
        &self.address_book
    }

    pub async fn process_transaction(
        &self,
        tx: PendingTransaction,
    ) -> Result<ProcessOutcome, PreExecutionError> {
        let started = Instant::now();
        let decision = self.address_book.prefilter(&tx);
        if !decision.should_simulate() {
            return Ok(ProcessOutcome::Filtered { tx_hash: tx.hash });
        }

        let trace = self.simulator.simulate(&tx).await?;
        let payload = self.codec.encode(&trace)?;
        let verdict = self.analyzer.analyze_trace(&payload).await?;
        let latency = started.elapsed();
        let risk = RiskProfile {
            score: verdict.score,
            critical_score: self.config.critical_score,
            malicious: verdict.score > self.config.critical_score,
            threats: verdict.threats,
            touched_addresses: trace.touched_addresses(),
            flags: trace.flags,
            latency_micros: latency.as_micros(),
            latency_exceeded: latency > self.config.latency_budget,
        };

        if risk.malicious {
            let alert = ThreatAlert {
                tx: tx.clone(),
                prefilter_hits: decision.hits,
                risk: risk.clone(),
                trace,
                counter_measures: self.counter_measures(),
            };
            self.responder.on_malicious(alert).await?;
            return Ok(ProcessOutcome::Blocked {
                tx_hash: tx.hash,
                risk,
            });
        }

        Ok(ProcessOutcome::Simulated {
            tx_hash: tx.hash,
            risk,
        })
    }

    pub async fn run_stream<S>(&self, stream: S) -> Result<(), PreExecutionError>
    where
        S: Stream<Item = PendingTransaction>,
    {
        tokio::pin!(stream);
        let mut workers = JoinSet::new();
        while let Some(tx) = stream.next().await {
            let pipeline = self.clone();
            workers.spawn(async move { pipeline.process_transaction(tx).await });

            while workers.len() >= self.config.max_in_flight.max(1) {
                join_next(&mut workers).await?;
            }
        }

        while !workers.is_empty() {
            join_next(&mut workers).await?;
        }

        Ok(())
    }

    fn counter_measures(&self) -> Vec<CounterMeasure> {
        let mut actions = vec![CounterMeasure::Killswitch];
        if self.config.enable_admin_pause {
            actions.push(CounterMeasure::PauseContracts {
                priority_fee: self.config.pause_priority_fee,
            });
        }
        actions
    }
}

#[derive(Debug, Default)]
pub struct NoopThreatResponder;

#[async_trait]
impl ThreatResponder for NoopThreatResponder {
    async fn on_malicious(&self, _alert: ThreatAlert) -> Result<(), PreExecutionError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct ChannelThreatResponder {
    tx: tokio::sync::mpsc::Sender<ThreatAlert>,
}

impl ChannelThreatResponder {
    pub fn new(tx: tokio::sync::mpsc::Sender<ThreatAlert>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl ThreatResponder for ChannelThreatResponder {
    async fn on_malicious(&self, alert: ThreatAlert) -> Result<(), PreExecutionError> {
        self.tx
            .send(alert)
            .await
            .map_err(|error| PreExecutionError::CounterMeasure(error.to_string()))
    }
}

pub struct HeuristicTraceAnalyzer<C = BincodeTraceCodec> {
    codec: C,
}

impl HeuristicTraceAnalyzer {
    pub fn new() -> Self {
        Self {
            codec: BincodeTraceCodec,
        }
    }
}

impl Default for HeuristicTraceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> HeuristicTraceAnalyzer<C> {
    pub fn with_codec(codec: C) -> Self {
        Self { codec }
    }
}

#[async_trait]
impl<C> TraceAnalyzer for HeuristicTraceAnalyzer<C>
where
    C: TraceCodec + Send + Sync,
{
    async fn analyze_trace(&self, payload: &[u8]) -> Result<AnalyzerVerdict, PreExecutionError> {
        let trace = self.codec.decode(payload)?;
        let mut score = 0_u8;
        let mut threats = Vec::new();

        if trace.flags.contains(TraceFlags::FLASH_LOAN_PATTERN) {
            score = score.saturating_add(50);
            threats.push(ThreatKind::FlashLoan);
        }
        if trace.flags.contains(TraceFlags::ORACLE_TOUCH) {
            score = score.saturating_add(40);
            threats.push(ThreatKind::OracleManipulation);
        }
        if trace.flags.contains(TraceFlags::SYBIL_DUST) {
            score = score.saturating_add(35);
            threats.push(ThreatKind::SybilDust);
        }
        if trace.flags.contains(TraceFlags::RECURSIVE_CALL)
            || trace.flags.contains(TraceFlags::REENTRANT_WRITE)
        {
            score = score.saturating_add(25);
            threats.push(ThreatKind::Reentrancy);
        }
        if !trace.state_diffs.is_empty() || !trace.balance_deltas.is_empty() {
            score = score.saturating_add(10);
            threats.push(ThreatKind::ProtectedSurface);
        }

        Ok(AnalyzerVerdict { score, threats })
    }
}

async fn join_next(
    workers: &mut JoinSet<Result<ProcessOutcome, PreExecutionError>>,
) -> Result<(), PreExecutionError> {
    match workers.join_next().await {
        Some(Ok(Ok(_))) => Ok(()),
        Some(Ok(Err(error))) => Err(error),
        Some(Err(error)) => Err(PreExecutionError::Worker(error.to_string())),
        None => Ok(()),
    }
}

fn push_unique_address(
    seen: &mut HashSet<Address>,
    addresses: &mut Vec<Address>,
    address: Address,
) {
    if seen.insert(address) {
        addresses.push(address);
    }
}

fn bloom_bits(address: Address) -> [usize; 3] {
    [
        (mix64(0x9e37_79b9_7f4a_7c15, address.as_bytes()) as usize) & 2047,
        (mix64(0xc2b2_ae3d_27d4_eb4f, address.as_bytes()) as usize) & 2047,
        (mix64(0x1656_67b1_9e37_79f9, address.as_bytes()) as usize) & 2047,
    ]
}

fn mix64(seed: u64, bytes: &[u8; 20]) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
        hash ^= hash >> 32;
    }
    hash
}

fn slice_to_address(slice: &[u8]) -> Address {
    let mut bytes = [0; 20];
    bytes.copy_from_slice(slice);
    Address::new(bytes)
}

fn parse_fixed_hex<const N: usize>(
    value: &str,
    kind: &'static str,
) -> Result<[u8; N], PreExecutionError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    let expected = N * 2;
    if value.len() != expected {
        return Err(PreExecutionError::InvalidHexLength {
            kind,
            expected,
            actual: value.len(),
        });
    }

    let mut out = [0_u8; N];
    let bytes = value.as_bytes();
    for index in 0..N {
        let high = hex_value(bytes[index * 2], kind)?;
        let low = hex_value(bytes[index * 2 + 1], kind)?;
        out[index] = (high << 4) | low;
    }
    Ok(out)
}

fn hex_value(byte: u8, kind: &'static str) -> Result<u8, PreExecutionError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(PreExecutionError::InvalidHexCharacter { kind }),
    }
}

fn fmt_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    f.write_str("0x")?;
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

#[cfg(feature = "evm-alloy")]
pub mod alloy_mempool {
    use super::{
        Address, PendingTransaction, PreExecutionError, PreExecutionPipeline, TxHash, Word,
    };
    use alloy::{
        consensus::Transaction as AlloyTransactionTrait,
        network::TransactionResponse,
        primitives::{Address as AlloyAddress, B256, U256},
        providers::{Provider, ProviderBuilder, WsConnect},
        rpc::types::Transaction as AlloyTransaction,
    };
    use futures_util::StreamExt;

    pub async fn run_full_pending_wss(
        wss_url: &str,
        pipeline: PreExecutionPipeline,
    ) -> Result<(), PreExecutionError> {
        let provider = ProviderBuilder::new()
            .connect_ws(WsConnect::new(wss_url))
            .await
            .map_err(|error| PreExecutionError::Ingestion(error.to_string()))?;
        let sub = provider
            .subscribe_full_pending_transactions()
            .await
            .map_err(|error| PreExecutionError::Ingestion(error.to_string()))?;
        let stream = sub
            .into_stream()
            .filter_map(|tx| async move { pending_from_alloy(tx).ok() });

        pipeline.run_stream(stream).await
    }

    pub fn pending_from_alloy(
        tx: AlloyTransaction,
    ) -> Result<PendingTransaction, PreExecutionError> {
        Ok(PendingTransaction {
            hash: tx_hash_from_b256(tx.tx_hash()),
            from: address_from_alloy(tx.from()),
            to: tx.to().map(address_from_alloy),
            value: word_from_u256(tx.value()),
            calldata: tx.input().as_ref().to_vec(),
            gas_price: Word::from_u128(
                AlloyTransactionTrait::gas_price(&tx)
                    .unwrap_or_else(|| AlloyTransactionTrait::max_fee_per_gas(&tx)),
            ),
            nonce: Some(tx.nonce()),
        })
    }

    fn tx_hash_from_b256(hash: B256) -> TxHash {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(hash.as_slice());
        TxHash::new(bytes)
    }

    fn address_from_alloy(address: AlloyAddress) -> Address {
        let mut bytes = [0; 20];
        bytes.copy_from_slice(address.as_slice());
        Address::new(bytes)
    }

    fn word_from_u256(value: U256) -> Word {
        Word::new(value.to_be_bytes())
    }
}
