#[cfg(test)]
mod tests;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info_span, Instrument};

pub const ETHEREUM_CONFIRMATIONS_REQUIRED: u64 = 12;
pub const POLYGON_CONFIRMATIONS_REQUIRED: u64 = 64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId {
    pub block_number: u64,
    pub tx_hash: String,
    pub log_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractEventKind {
    Transfer,
    Mint,
    Burn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexedEventStatus {
    Unconfirmed,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedContractEvent {
    pub id: EventId,
    pub block_hash: String,
    pub contract_address: String,
    pub kind: ContractEventKind,
    pub from: Option<String>,
    pub to: Option<String>,
    pub amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredIndexedEvent {
    pub event: IndexedContractEvent,
    pub status: IndexedEventStatus,
}

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error("event amount must be positive")]
    InvalidAmount,

    #[error("block reorg at block {block_number}: {old_hash} -> {new_hash}")]
    Reorg {
        block_number: u64,
        old_hash: String,
        new_hash: String,
    },

    #[error("source error: {0}")]
    Source(String),

    #[error("sink error: {0}")]
    Sink(String),

    #[error("event is missing chain metadata: {0}")]
    MissingMetadata(&'static str),

    #[error("event decode error: {0}")]
    Decode(String),
}

#[derive(Debug, Clone)]
pub struct ReorgBuffer {
    confirmations_required: u64,
    events: BTreeMap<EventId, StoredIndexedEvent>,
    block_hashes: BTreeMap<u64, String>,
}

impl ReorgBuffer {
    pub fn new(confirmations_required: u64) -> Self {
        Self {
            confirmations_required,
            events: BTreeMap::new(),
            block_hashes: BTreeMap::new(),
        }
    }

    pub fn ingest_unconfirmed(
        &mut self,
        event: IndexedContractEvent,
    ) -> Result<Option<Vec<StoredIndexedEvent>>, IndexerError> {
        if event.amount <= Decimal::ZERO {
            return Err(IndexerError::InvalidAmount);
        }

        if let Some(old_hash) = self.block_hashes.get(&event.id.block_number) {
            if old_hash != &event.block_hash {
                let removed = self.rollback_unconfirmed_from(event.id.block_number);
                self.block_hashes
                    .insert(event.id.block_number, event.block_hash.clone());
                self.events.insert(
                    event.id.clone(),
                    StoredIndexedEvent {
                        event,
                        status: IndexedEventStatus::Unconfirmed,
                    },
                );
                return Ok(Some(removed));
            }
        }

        self.block_hashes
            .insert(event.id.block_number, event.block_hash.clone());
        self.events.insert(
            event.id.clone(),
            StoredIndexedEvent {
                event,
                status: IndexedEventStatus::Unconfirmed,
            },
        );
        Ok(None)
    }

    pub fn ingest_pending(
        &mut self,
        event: IndexedContractEvent,
    ) -> Result<Option<Vec<StoredIndexedEvent>>, IndexerError> {
        self.ingest_unconfirmed(event)
    }

    pub fn confirm_ready(&mut self, current_block: u64) -> Vec<StoredIndexedEvent> {
        let mut confirmed = Vec::new();
        for stored in self.events.values_mut() {
            if stored.status == IndexedEventStatus::Unconfirmed
                && current_block.saturating_sub(stored.event.id.block_number)
                    >= self.confirmations_required
            {
                stored.status = IndexedEventStatus::Confirmed;
                confirmed.push(stored.clone());
            }
        }
        confirmed
    }

    pub fn rollback_unconfirmed_from(&mut self, block_number: u64) -> Vec<StoredIndexedEvent> {
        let event_ids: Vec<EventId> = self
            .events
            .iter()
            .filter_map(|(id, stored)| {
                (id.block_number >= block_number
                    && stored.status == IndexedEventStatus::Unconfirmed)
                    .then_some(id.clone())
            })
            .collect();
        let mut removed = Vec::with_capacity(event_ids.len());
        for id in event_ids {
            if let Some(event) = self.events.remove(&id) {
                removed.push(event);
            }
        }

        self.block_hashes.retain(|number, _| *number < block_number);
        removed
    }

    pub fn rollback_from(&mut self, block_number: u64) -> Vec<StoredIndexedEvent> {
        self.rollback_unconfirmed_from(block_number)
    }

    pub fn pending_len(&self) -> usize {
        self.events
            .values()
            .filter(|event| event.status == IndexedEventStatus::Unconfirmed)
            .count()
    }
}

#[async_trait]
pub trait EvmEventSource: Send {
    async fn next_event(&mut self) -> Result<Option<IndexedContractEvent>, IndexerError>;
    async fn current_block(&self) -> Result<u64, IndexerError>;
}

#[async_trait]
pub trait ConfirmedEventSink: Send + Sync {
    async fn on_confirmed(&self, event: StoredIndexedEvent) -> Result<(), IndexerError>;

    async fn on_rollback(&self, _events: Vec<StoredIndexedEvent>) -> Result<(), IndexerError> {
        Ok(())
    }
}

pub fn spawn_event_worker<S, K>(
    mut source: S,
    sink: Arc<K>,
    buffer: Arc<Mutex<ReorgBuffer>>,
) -> JoinHandle<Result<(), IndexerError>>
where
    S: EvmEventSource + Send + 'static,
    K: ConfirmedEventSink + 'static,
{
    tokio::spawn(async move {
        loop {
            let Some(event) = source.next_event().await? else {
                break;
            };
            let current_block = source.current_block().await?;
            let block_number = event.id.block_number;
            let block_hash = event.block_hash.clone();
            let span = info_span!(
                "evm.block",
                block_number,
                block_hash = %block_hash,
                current_block
            );

            let (rolled_back, confirmed) = async {
                let mut buffer = buffer.lock().await;
                let rolled_back = buffer.ingest_unconfirmed(event)?.unwrap_or_default();
                let confirmed = buffer.confirm_ready(current_block);
                Ok::<_, IndexerError>((rolled_back, confirmed))
            }
            .instrument(span)
            .await?;

            if !rolled_back.is_empty() {
                sink.on_rollback(rolled_back).await?;
            }
            for confirmed in confirmed {
                sink.on_confirmed(confirmed).await?;
            }
        }
        Ok(())
    })
}

#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    current: Duration,
    initial: Duration,
    max: Duration,
    factor: u32,
}

impl ExponentialBackoff {
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            current: initial,
            initial,
            max,
            factor: 2,
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = match self.current.checked_mul(self.factor) {
            Some(next) => next.min(self.max),
            None => self.max,
        };
        delay
    }

    pub fn reset(&mut self) {
        self.current = self.initial;
    }
}

#[cfg(feature = "evm-alloy")]
pub mod alloy_indexer {
    use super::{
        ConfirmedEventSink, EventId, ExponentialBackoff, IndexedContractEvent, IndexerError,
        ReorgBuffer, StoredIndexedEvent,
    };
    use alloy::{
        primitives::{Address, U256},
        providers::{Provider, ProviderBuilder, WsConnect},
        rpc::types::{BlockNumberOrTag, Filter, Log},
        sol,
        sol_types::SolEvent,
    };
    use futures_util::StreamExt;
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::{sync::Mutex, time::sleep};
    use tracing::{info_span, warn, Instrument};

    sol!(
        #[allow(missing_docs)]
        #[sol(rpc)]
        RwaToken,
        "abi/RwaToken.json"
    );

    pub async fn run_rwa_indexer<K>(
        wss_url: &str,
        contract_address: Address,
        sink: Arc<K>,
        buffer: Arc<Mutex<ReorgBuffer>>,
    ) -> Result<(), IndexerError>
    where
        K: ConfirmedEventSink + 'static,
    {
        let mut backoff = ExponentialBackoff::new(Duration::from_secs(1), Duration::from_secs(60));

        loop {
            match stream_rwa_events(
                wss_url,
                contract_address,
                Arc::clone(&sink),
                Arc::clone(&buffer),
            )
            .await
            {
                Ok(()) => {
                    warn!("alloy websocket stream closed");
                }
                Err(error) => {
                    warn!(error = %error, "alloy websocket stream failed");
                }
            }
            let delay = backoff.next_delay();
            sleep(delay).await;
        }
    }

    pub async fn subscribe_rwa_events(
        wss_url: &str,
        contract_address: Address,
    ) -> Result<(), IndexerError> {
        let sink = Arc::new(NoopEventSink);
        let buffer = Arc::new(Mutex::new(ReorgBuffer::new(
            super::ETHEREUM_CONFIRMATIONS_REQUIRED,
        )));
        run_rwa_indexer(wss_url, contract_address, sink, buffer).await
    }

    async fn stream_rwa_events<K>(
        wss_url: &str,
        contract_address: Address,
        sink: Arc<K>,
        buffer: Arc<Mutex<ReorgBuffer>>,
    ) -> Result<(), IndexerError>
    where
        K: ConfirmedEventSink + 'static,
    {
        let ws = WsConnect::new(wss_url);
        let provider = ProviderBuilder::new()
            .connect_ws(ws)
            .await
            .map_err(|error| IndexerError::Source(error.to_string()))?;
        let filter = Filter::new()
            .address(contract_address)
            .from_block(BlockNumberOrTag::Latest);
        let sub = provider
            .subscribe_logs(&filter)
            .await
            .map_err(|error| IndexerError::Source(error.to_string()))?;
        let mut stream = sub.into_stream();

        while let Some(log) = stream.next().await {
            let current_block = provider
                .get_block_number()
                .await
                .map_err(|error| IndexerError::Source(error.to_string()))?;
            handle_log(log, current_block, &sink, &buffer).await?;
        }

        Ok(())
    }

    async fn handle_log<K>(
        log: Log,
        current_block: u64,
        sink: &Arc<K>,
        buffer: &Arc<Mutex<ReorgBuffer>>,
    ) -> Result<(), IndexerError>
    where
        K: ConfirmedEventSink + 'static,
    {
        if log.removed {
            let block_number = log
                .block_number
                .ok_or(IndexerError::MissingMetadata("block_number"))?;
            let rolled_back = {
                let mut buffer = buffer.lock().await;
                buffer.rollback_unconfirmed_from(block_number)
            };
            if !rolled_back.is_empty() {
                sink.on_rollback(rolled_back).await?;
            }
            return Ok(());
        }

        let Some(event) = decode_rwa_event(&log)? else {
            return Ok(());
        };
        let block_number = event.id.block_number;
        let block_hash = event.block_hash.clone();
        let span = info_span!(
            "evm.block",
            block_number,
            block_hash = %block_hash,
            current_block
        );

        let (rolled_back, confirmed) = async {
            let mut buffer = buffer.lock().await;
            let rolled_back = buffer.ingest_unconfirmed(event)?.unwrap_or_default();
            let confirmed = buffer.confirm_ready(current_block);
            Ok::<_, IndexerError>((rolled_back, confirmed))
        }
        .instrument(span)
        .await?;

        if !rolled_back.is_empty() {
            sink.on_rollback(rolled_back).await?;
        }
        for confirmed in confirmed {
            sink.on_confirmed(confirmed).await?;
        }
        Ok(())
    }

    fn decode_rwa_event(log: &Log) -> Result<Option<IndexedContractEvent>, IndexerError> {
        match log.topic0() {
            Some(&RwaToken::Minted::SIGNATURE_HASH) => {
                let decoded = log
                    .log_decode::<RwaToken::Minted>()
                    .map_err(|error| IndexerError::Decode(error.to_string()))?;
                Ok(Some(indexed_event(
                    log,
                    super::ContractEventKind::Mint,
                    None,
                    Some(decoded.inner.data.to.to_string()),
                    decoded.inner.data.amount,
                )?))
            }
            Some(&RwaToken::Redeemed::SIGNATURE_HASH) => {
                let decoded = log
                    .log_decode::<RwaToken::Redeemed>()
                    .map_err(|error| IndexerError::Decode(error.to_string()))?;
                Ok(Some(indexed_event(
                    log,
                    super::ContractEventKind::Burn,
                    Some(decoded.inner.data.from.to_string()),
                    None,
                    decoded.inner.data.amount,
                )?))
            }
            Some(&RwaToken::Transfer::SIGNATURE_HASH) => {
                let decoded = log
                    .log_decode::<RwaToken::Transfer>()
                    .map_err(|error| IndexerError::Decode(error.to_string()))?;
                Ok(Some(indexed_event(
                    log,
                    super::ContractEventKind::Transfer,
                    Some(decoded.inner.data.from.to_string()),
                    Some(decoded.inner.data.to.to_string()),
                    decoded.inner.data.value,
                )?))
            }
            _ => Ok(None),
        }
    }

    fn indexed_event(
        log: &Log,
        kind: super::ContractEventKind,
        from: Option<String>,
        to: Option<String>,
        amount: U256,
    ) -> Result<IndexedContractEvent, IndexerError> {
        let block_number = log
            .block_number
            .ok_or(IndexerError::MissingMetadata("block_number"))?;
        let block_hash = log
            .block_hash
            .ok_or(IndexerError::MissingMetadata("block_hash"))?;
        let tx_hash = log
            .transaction_hash
            .ok_or(IndexerError::MissingMetadata("transaction_hash"))?;
        let log_index = log
            .log_index
            .ok_or(IndexerError::MissingMetadata("log_index"))?;
        let amount = Decimal::from_str(&amount.to_string())
            .map_err(|error| IndexerError::Decode(error.to_string()))?;

        Ok(IndexedContractEvent {
            id: EventId {
                block_number,
                tx_hash: tx_hash.to_string(),
                log_index,
            },
            block_hash: block_hash.to_string(),
            contract_address: log.address().to_string(),
            kind,
            from,
            to,
            amount,
        })
    }

    struct NoopEventSink;

    #[async_trait::async_trait]
    impl ConfirmedEventSink for NoopEventSink {
        async fn on_confirmed(&self, _event: StoredIndexedEvent) -> Result<(), IndexerError> {
            Ok(())
        }
    }
}
