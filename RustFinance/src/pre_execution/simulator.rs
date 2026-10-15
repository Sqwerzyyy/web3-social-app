use super::{
    analyzer::{ObservedCall, ObservedCallKind, ObservedStorageWrite, ObservedTrace},
    Address, PendingTransaction, PreExecutionError, TraceSimulator,
};
use alloy::{
    consensus::BlockHeader,
    eips::{BlockId, BlockNumberOrTag},
    network::{primitives::HeaderResponse, BlockResponse},
    primitives::{Address as AlloyAddress, B256 as AlloyB256, U256 as AlloyU256},
    providers::Provider,
};
use async_trait::async_trait;
use revm::{
    context::{result::ExecutionResult, BlockEnv, CfgEnv, Context, TxEnv},
    database_interface::{DatabaseAsync, WrapDatabaseAsync},
    primitives::{hardfork::SpecId, Address as RevmAddress, Bytes, TxKind, B256, U256},
    state::{AccountInfo, Bytecode, EvmState},
    ExecuteEvm, MainBuilder, MainContext,
};
use std::{fmt, sync::Arc};
use thiserror::Error;
use tracing::{debug, instrument};

#[derive(Debug, Error)]
pub enum ForkDbError {
    #[error("rpc provider error: {0}")]
    Provider(String),

    #[error("missing latest block")]
    MissingLatestBlock,

    #[error("missing block hash for block {0}")]
    MissingBlockHash(u64),
}

impl revm::database_interface::DBErrorMarker for ForkDbError {}

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub chain_id: u64,
    pub spec_id: SpecId,
    pub disable_nonce_check: bool,
    pub disable_balance_check: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            chain_id: 1,
            spec_id: SpecId::PRAGUE,
            disable_nonce_check: true,
            disable_balance_check: false,
        }
    }
}

#[derive(Debug)]
pub struct SimulationOutcome {
    pub result: ExecutionResult,
    pub state: EvmState,
    pub observed: ObservedTrace,
}

#[derive(Clone)]
pub struct RevmSimulator<P> {
    provider: Arc<P>,
    config: SimulationConfig,
}

impl<P> RevmSimulator<P> {
    pub fn new(provider: Arc<P>, config: SimulationConfig) -> Self {
        Self { provider, config }
    }
}

impl<P> fmt::Debug for RevmSimulator<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RevmSimulator")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<P> RevmSimulator<P>
where
    P: Provider + Send + Sync + 'static,
{
    #[instrument(skip_all, fields(hash = %tx.hash))]
    pub async fn execute_tx(
        &self,
        tx: &PendingTransaction,
    ) -> Result<SimulationOutcome, PreExecutionError> {
        let block = self
            .provider
            .get_block_by_number(BlockNumberOrTag::Latest)
            .await
            .map_err(|error| PreExecutionError::Simulation(error.to_string()))?
            .ok_or_else(|| {
                PreExecutionError::Simulation(ForkDbError::MissingLatestBlock.to_string())
            })?;

        let block_env = block_env_from_alloy(&block)?;
        let fork = AlloyForkDb::new(
            Arc::clone(&self.provider),
            BlockId::Number(BlockNumberOrTag::Latest.into()),
        );
        let db = WrapDatabaseAsync::new(fork).ok_or_else(|| {
            PreExecutionError::Simulation(
                "revm async DB requires a multithreaded tokio runtime".into(),
            )
        })?;
        let mut evm = Context::mainnet()
            .with_db(db)
            .with_block(block_env)
            .modify_cfg_chained(|cfg: &mut CfgEnv| {
                cfg.chain_id = self.config.chain_id;
                cfg.disable_nonce_check = self.config.disable_nonce_check;
                cfg.disable_balance_check = self.config.disable_balance_check;
                cfg.set_spec_and_mainnet_gas_params(self.config.spec_id);
            })
            .build_mainnet();

        let tx_env = tx_env_from_pending(tx, self.config.chain_id)?;
        let out = evm
            .transact(tx_env)
            .map_err(|error| PreExecutionError::Simulation(error.to_string()))?;

        debug!(
            gas_used = out.result.tx_gas_used(),
            "pending transaction simulated"
        );

        Ok(SimulationOutcome {
            result: out.result,
            state: out.state,
            observed: ObservedTrace::default(),
        })
    }
}

#[async_trait]
impl<P> TraceSimulator for RevmSimulator<P>
where
    P: Provider + Send + Sync + 'static,
{
    async fn simulate(
        &self,
        tx: &PendingTransaction,
    ) -> Result<super::ExecutionTrace, PreExecutionError> {
        let outcome = self.execute_tx(tx).await?;
        Ok(super::ExecutionTrace {
            tx_hash: tx.hash,
            entrypoint: tx.to,
            call_stack: Vec::new(),
            state_diffs: state_diffs_from_revm(&outcome.state),
            erc20_transfers: Vec::new(),
            balance_deltas: Vec::new(),
            flags: super::TraceFlags::empty(),
            gas_used: outcome.result.tx_gas_used(),
        })
    }
}

#[derive(Clone)]
struct AlloyForkDb<P> {
    provider: Arc<P>,
    block_id: BlockId,
}

impl<P> AlloyForkDb<P> {
    fn new(provider: Arc<P>, block_id: BlockId) -> Self {
        Self { provider, block_id }
    }
}

impl<P> DatabaseAsync for AlloyForkDb<P>
where
    P: Provider + Send + Sync,
{
    type Error = ForkDbError;

    async fn basic_async(
        &mut self,
        address: RevmAddress,
    ) -> Result<Option<AccountInfo>, Self::Error> {
        let address = alloy_address(address);
        let balance = self
            .provider
            .get_balance(address)
            .block_id(self.block_id)
            .await
            .map_err(|error| ForkDbError::Provider(error.to_string()))?;
        let nonce = self
            .provider
            .get_transaction_count(address)
            .block_id(self.block_id)
            .await
            .map_err(|error| ForkDbError::Provider(error.to_string()))?;
        let code = self
            .provider
            .get_code_at(address)
            .block_id(self.block_id)
            .await
            .map_err(|error| ForkDbError::Provider(error.to_string()))?;

        let bytecode = Bytecode::new_raw(Bytes::copy_from_slice(code.as_ref()));
        Ok(Some(AccountInfo::new(
            revm_u256(balance),
            nonce,
            bytecode.hash_slow(),
            bytecode,
        )))
    }

    async fn code_by_hash_async(&mut self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        Ok(Bytecode::default())
    }

    async fn storage_async(
        &mut self,
        address: RevmAddress,
        index: U256,
    ) -> Result<U256, Self::Error> {
        let value = self
            .provider
            .get_storage_at(alloy_address(address), alloy_u256(index))
            .block_id(self.block_id)
            .await
            .map_err(|error| ForkDbError::Provider(error.to_string()))?;
        Ok(revm_u256(value))
    }

    async fn block_hash_async(&mut self, number: u64) -> Result<B256, Self::Error> {
        let block = self
            .provider
            .get_block_by_number(BlockNumberOrTag::Number(number))
            .await
            .map_err(|error| ForkDbError::Provider(error.to_string()))?
            .ok_or(ForkDbError::MissingBlockHash(number))?;
        Ok(revm_b256(block.header().hash))
    }
}

fn block_env_from_alloy<B>(block: &B) -> Result<BlockEnv, PreExecutionError>
where
    B: BlockResponse,
    B::Header: HeaderResponse,
{
    let header = block.header();
    let mut env = BlockEnv::default();
    env.number = U256::from(BlockHeader::number(header));
    env.beneficiary = revm_address(BlockHeader::beneficiary(header));
    env.timestamp = U256::from(BlockHeader::timestamp(header));
    env.gas_limit = BlockHeader::gas_limit(header);
    env.basefee = BlockHeader::base_fee_per_gas(header).unwrap_or_default();
    env.difficulty = revm_u256(BlockHeader::difficulty(header));
    env.prevrandao = BlockHeader::mix_hash(header).map(revm_b256);
    if let Some(excess_blob_gas) = BlockHeader::excess_blob_gas(header) {
        env.set_blob_excess_gas_and_price(
            excess_blob_gas,
            self::CANCUN_BLOB_BASE_FEE_UPDATE_FRACTION,
        );
    }
    Ok(env)
}

const CANCUN_BLOB_BASE_FEE_UPDATE_FRACTION: u64 = 3_338_477;

fn tx_env_from_pending(tx: &PendingTransaction, chain_id: u64) -> Result<TxEnv, PreExecutionError> {
    let kind = match tx.to {
        Some(to) => TxKind::Call(revm_address_from_local(to)),
        None => TxKind::Create,
    };
    Ok(TxEnv::builder()
        .caller(revm_address_from_local(tx.from))
        .kind(kind)
        .data(Bytes::copy_from_slice(&tx.calldata))
        .gas_limit(30_000_000)
        .gas_price(word_to_u128(tx.gas_price)?)
        .value(word_to_revm_u256(tx.value))
        .nonce(tx.nonce.unwrap_or_default())
        .chain_id(Some(chain_id))
        .build()
        .map_err(|error| PreExecutionError::Simulation(error.to_string()))?)
}

fn state_diffs_from_revm(state: &EvmState) -> Vec<super::StateSlotDiff> {
    let mut out = Vec::new();
    for (contract, account) in state {
        let contract = local_address(*contract);
        for (slot, value) in &account.storage {
            if !value.is_changed() {
                continue;
            }
            out.push(super::StateSlotDiff {
                contract,
                slot: super::Word::new(slot.to_be_bytes()),
                before: super::Word::new(value.original_value().to_be_bytes()),
                after: super::Word::new(value.present_value().to_be_bytes()),
            });
        }
    }
    out
}

fn revm_address_from_local(address: Address) -> RevmAddress {
    RevmAddress::from_slice(address.as_bytes())
}

fn local_address(address: RevmAddress) -> Address {
    let mut bytes = [0_u8; 20];
    bytes.copy_from_slice(address.as_slice());
    Address::new(bytes)
}

fn revm_address(address: AlloyAddress) -> RevmAddress {
    RevmAddress::from_slice(address.as_slice())
}

fn alloy_address(address: RevmAddress) -> AlloyAddress {
    AlloyAddress::from_slice(address.as_slice())
}

fn revm_b256(value: AlloyB256) -> B256 {
    B256::from_slice(value.as_slice())
}

fn alloy_u256(value: U256) -> AlloyU256 {
    AlloyU256::from_be_bytes(value.to_be_bytes::<32>())
}

fn revm_u256(value: AlloyU256) -> U256 {
    U256::from_be_bytes(value.to_be_bytes::<32>())
}

fn word_to_revm_u256(value: super::Word) -> U256 {
    U256::from_be_bytes(*value.as_bytes())
}

fn word_to_u128(value: super::Word) -> Result<u128, PreExecutionError> {
    let value = word_to_revm_u256(value);
    if value > U256::from(u128::MAX) {
        return Err(PreExecutionError::Simulation(
            "gas price exceeds u128".into(),
        ));
    }
    Ok(value.to::<u128>())
}

pub fn observed_call(
    depth: u16,
    caller: RevmAddress,
    callee: RevmAddress,
    kind: ObservedCallKind,
) -> ObservedCall {
    ObservedCall {
        depth,
        caller,
        callee,
        kind,
    }
}

pub fn observed_storage_write(
    contract: RevmAddress,
    slot: U256,
    before: U256,
    after: U256,
) -> ObservedStorageWrite {
    ObservedStorageWrite {
        contract,
        slot,
        before,
        after,
    }
}
