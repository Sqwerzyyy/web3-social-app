#[cfg(test)]
mod tests;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

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
    Pending,
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

    pub fn ingest_pending(
        &mut self,
        event: IndexedContractEvent,
    ) -> Result<Option<Vec<StoredIndexedEvent>>, IndexerError> {
        if event.amount <= Decimal::ZERO {
            return Err(IndexerError::InvalidAmount);
        }

        if let Some(old_hash) = self.block_hashes.get(&event.id.block_number) {
            if old_hash != &event.block_hash {
                let removed = self.rollback_from(event.id.block_number);
                self.block_hashes
                    .insert(event.id.block_number, event.block_hash.clone());
                self.events.insert(
                    event.id.clone(),
                    StoredIndexedEvent {
                        event,
                        status: IndexedEventStatus::Pending,
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
                status: IndexedEventStatus::Pending,
            },
        );
        Ok(None)
    }

    pub fn confirm_ready(&mut self, current_block: u64) -> Vec<StoredIndexedEvent> {
        let mut confirmed = Vec::new();
        for stored in self.events.values_mut() {
            if stored.status == IndexedEventStatus::Pending
                && current_block.saturating_sub(stored.event.id.block_number)
                    >= self.confirmations_required
            {
                stored.status = IndexedEventStatus::Confirmed;
                confirmed.push(stored.clone());
            }
        }
        confirmed
    }

    pub fn rollback_from(&mut self, block_number: u64) -> Vec<StoredIndexedEvent> {
        let event_ids: Vec<EventId> = self
            .events
            .keys()
            .filter(|id| id.block_number >= block_number)
            .cloned()
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

    pub fn pending_len(&self) -> usize {
        self.events
            .values()
            .filter(|event| event.status == IndexedEventStatus::Pending)
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

            {
                let mut buffer = buffer.lock().await;
                let _ = buffer.ingest_pending(event)?;
                let current_block = source.current_block().await?;
                for confirmed in buffer.confirm_ready(current_block) {
                    sink.on_confirmed(confirmed).await?;
                }
            }
        }
        Ok(())
    })
}

#[cfg(feature = "evm-alloy")]
pub mod alloy_indexer {
    use super::IndexerError;
    use alloy::{
        primitives::Address,
        providers::{Provider, ProviderBuilder, WsConnect},
        rpc::types::{BlockNumberOrTag, Filter},
        sol,
        sol_types::SolEvent,
    };
    use futures_util::StreamExt;

    sol!(
        #[allow(missing_docs)]
        #[sol(rpc)]
        RwaToken,
        "abi/RwaToken.json"
    );

    pub async fn subscribe_rwa_events(
        wss_url: &str,
        contract_address: Address,
    ) -> Result<(), IndexerError> {
        loop {
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
                match log.topic0() {
                    Some(&RwaToken::Transfer::SIGNATURE_HASH)
                    | Some(&RwaToken::Mint::SIGNATURE_HASH)
                    | Some(&RwaToken::Burn::SIGNATURE_HASH) => {}
                    _ => {}
                }
            }
        }
    }
}
