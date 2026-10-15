use super::{AddressBook, PreExecutionError};
use revm::{
    context::result::ExecutionResult,
    primitives::{Address, Log, U256},
    state::EvmState,
};
use std::collections::{HashMap, HashSet};

const TRANSFER_TOPIC: [u8; 32] = [
    0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b, 0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d, 0xaa,
    0x95, 0x2b, 0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16, 0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef,
];
const CRITICAL_SCORE: u8 = 90;
const SUSPICIOUS_SCORE: u8 = 45;
const FLASH_LOAN_SCORE: u8 = 55;
const REENTRANCY_SCORE: u8 = 70;
const HALT_SCORE: u8 = 25;
const TVL_BPS_THRESHOLD: u128 = 1_000;
const BPS_DENOMINATOR: u128 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskStatus {
    Safe,
    Suspicious,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiskSignal {
    Reentrancy,
    FlashLoanImpact,
    ExecutionHalt,
    StateMutation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskScore {
    pub status: RiskStatus,
    pub score: u8,
    pub signals: Vec<RiskSignal>,
    pub max_call_depth: u16,
    pub max_tvl_impact_bps: u128,
}

impl RiskScore {
    pub fn safe() -> Self {
        Self {
            status: RiskStatus::Safe,
            score: 0,
            signals: Vec::new(),
            max_call_depth: 0,
            max_tvl_impact_bps: 0,
        }
    }

    fn push_signal(&mut self, signal: RiskSignal, score: u8) {
        if !self.signals.contains(&signal) {
            self.signals.push(signal);
        }
        self.score = self.score.saturating_add(score);
        self.status = if self.score >= CRITICAL_SCORE {
            RiskStatus::Critical
        } else if self.score >= SUSPICIOUS_SCORE {
            RiskStatus::Suspicious
        } else {
            RiskStatus::Safe
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedCallKind {
    Call,
    DelegateCall,
    StaticCall,
    CallCode,
    Create,
    Create2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedCall {
    pub depth: u16,
    pub caller: Address,
    pub callee: Address,
    pub kind: ObservedCallKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedStorageWrite {
    pub contract: Address,
    pub slot: U256,
    pub before: U256,
    pub after: U256,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ObservedTrace {
    pub calls: Vec<ObservedCall>,
    pub storage_writes: Vec<ObservedStorageWrite>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TvlRegistry {
    token_tvl: HashMap<Address, U256>,
}

impl TvlRegistry {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            token_tvl: HashMap::with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, token: Address, tvl: U256) {
        self.token_tvl.insert(token, tvl);
    }

    pub fn get(&self, token: &Address) -> Option<&U256> {
        self.token_tvl.get(token)
    }
}

pub fn analyze_trace(result: &ExecutionResult) -> RiskScore {
    let mut risk = RiskScore::safe();
    if result.is_halt() {
        risk.push_signal(RiskSignal::ExecutionHalt, HALT_SCORE);
    }
    risk
}

pub fn analyze_observed_trace(
    result: &ExecutionResult,
    state: &EvmState,
    observed: &ObservedTrace,
    address_book: &AddressBook,
    tvl: &TvlRegistry,
) -> Result<RiskScore, PreExecutionError> {
    let mut risk = analyze_trace(result);

    if has_reentrant_path(observed, address_book) {
        risk.push_signal(RiskSignal::Reentrancy, REENTRANCY_SCORE);
    }

    risk.max_call_depth = observed
        .calls
        .iter()
        .map(|call| call.depth)
        .max()
        .unwrap_or_default();

    if state.values().any(|account| {
        account.status.is_touched() || account.storage.values().any(|slot| slot.is_changed())
    }) {
        risk.push_signal(RiskSignal::StateMutation, 10);
    }

    let impact_bps = max_erc20_impact_bps(result.logs(), tvl)?;
    risk.max_tvl_impact_bps = impact_bps;
    if impact_bps > TVL_BPS_THRESHOLD {
        risk.push_signal(RiskSignal::FlashLoanImpact, FLASH_LOAN_SCORE);
    }

    Ok(risk)
}

fn has_reentrant_path(observed: &ObservedTrace, address_book: &AddressBook) -> bool {
    let mut active = HashSet::with_capacity(observed.calls.len().min(32));
    let mut last_depth = 0_u16;

    for call in &observed.calls {
        if call.depth <= last_depth {
            active.clear();
        }
        last_depth = call.depth;

        let tracked = super_address_from_revm(&call.callee)
            .map(|address| address_book.get(&address).is_some())
            .unwrap_or(false);
        if !tracked {
            continue;
        }
        if !active.insert(call.callee) {
            return true;
        }
    }

    false
}

fn max_erc20_impact_bps(logs: &[Log], tvl: &TvlRegistry) -> Result<u128, PreExecutionError> {
    let mut moved_by_token = HashMap::<Address, U256>::with_capacity(logs.len().min(64));
    for log in logs {
        let topics = log.topics();
        let Some(topic0) = topics.first() else {
            continue;
        };
        if topic0.as_slice() != TRANSFER_TOPIC {
            continue;
        }
        let amount = decode_transfer_amount(log.data.data.as_ref())?;
        let entry = moved_by_token.entry(log.address).or_insert(U256::ZERO);
        *entry = entry.saturating_add(amount);
    }

    let mut max_bps = 0_u128;
    for (token, moved) in moved_by_token {
        let Some(token_tvl) = tvl.get(&token) else {
            continue;
        };
        let Some(bps) = impact_bps(moved, *token_tvl) else {
            continue;
        };
        max_bps = max_bps.max(bps);
    }

    Ok(max_bps)
}

fn impact_bps(amount: U256, tvl: U256) -> Option<u128> {
    if tvl.is_zero() {
        return None;
    }
    let amount = amount.saturating_mul(U256::from(BPS_DENOMINATOR));
    u256_to_u128(amount / tvl)
}

fn decode_transfer_amount(data: &[u8]) -> Result<U256, PreExecutionError> {
    if data.len() < 32 {
        return Err(PreExecutionError::Analyzer(
            "short ERC-20 Transfer data".into(),
        ));
    }
    Ok(U256::from_be_slice(&data[..32]))
}

fn super_address_from_revm(address: &Address) -> Option<super::Address> {
    let mut out = [0_u8; 20];
    out.copy_from_slice(address.as_slice());
    Some(super::Address::new(out))
}

fn u256_to_u128(value: U256) -> Option<u128> {
    if value > U256::from(u128::MAX) {
        return None;
    }
    Some(value.to::<u128>())
}
