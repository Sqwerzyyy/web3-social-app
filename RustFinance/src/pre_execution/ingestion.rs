use super::{Address, AddressBook, PendingTransaction, PreExecutionError, TxHash, Word};
use alloy::{
    consensus::Transaction as AlloyTransaction,
    network::TransactionResponse,
    primitives::{Address as AlloyAddress, B256, U256},
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::Transaction,
};
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct IngestionConfig {
    pub wss_url: Arc<str>,
    pub channel_capacity: usize,
}

impl IngestionConfig {
    pub fn new(wss_url: impl Into<Arc<str>>, channel_capacity: usize) -> Self {
        Self {
            wss_url: wss_url.into(),
            channel_capacity: channel_capacity.max(1),
        }
    }
}

#[derive(Clone)]
pub struct MempoolIngestionWorker {
    address_book: Arc<AddressBook>,
    tx_out: mpsc::Sender<PendingTransaction>,
}

impl MempoolIngestionWorker {
    pub fn new(address_book: Arc<AddressBook>, tx_out: mpsc::Sender<PendingTransaction>) -> Self {
        Self {
            address_book,
            tx_out,
        }
    }

    pub fn channel(
        address_book: Arc<AddressBook>,
        capacity: usize,
    ) -> (Self, mpsc::Receiver<PendingTransaction>) {
        let (tx_out, rx) = mpsc::channel(capacity.max(1));
        (Self::new(address_book, tx_out), rx)
    }

    pub async fn run(self, wss_url: &str) -> Result<(), PreExecutionError> {
        let provider = ProviderBuilder::new()
            .connect_ws(WsConnect::new(wss_url))
            .await
            .map_err(|error| PreExecutionError::Ingestion(error.to_string()))?;

        let sub = provider
            .subscribe_full_pending_transactions()
            .await
            .map_err(|error| PreExecutionError::Ingestion(error.to_string()))?;
        let mut stream = sub.into_stream();

        info!(
            tracked_addresses = self.address_book.len(),
            "mempool stream attached"
        );

        while let Some(tx) = stream.next().await {
            let Some(to) = tx.to() else {
                debug!(hash = %tx.tx_hash(), "pending contract creation ignored");
                continue;
            };
            let to = address_from_alloy(to);
            if self.address_book.get(&to).is_none() {
                continue;
            }

            let pending = pending_from_alloy(tx)?;
            let hash = pending.hash;
            if self.tx_out.send(pending).await.is_err() {
                warn!(%hash, "simulation channel closed");
                return Ok(());
            }
        }

        warn!("mempool subscription stream ended");
        Ok(())
    }
}

pub async fn spawn_ingestion(
    config: IngestionConfig,
    address_book: Arc<AddressBook>,
) -> Result<mpsc::Receiver<PendingTransaction>, PreExecutionError> {
    let (worker, rx) = MempoolIngestionWorker::channel(address_book, config.channel_capacity);
    tokio::spawn(async move {
        if let Err(error) = worker.run(&config.wss_url).await {
            error!(%error, "mempool ingestion worker stopped");
        }
    });
    Ok(rx)
}

pub fn pending_from_alloy(tx: Transaction) -> Result<PendingTransaction, PreExecutionError> {
    Ok(PendingTransaction {
        hash: tx_hash_from_b256(tx.tx_hash()),
        from: address_from_alloy(tx.from()),
        to: tx.to().map(address_from_alloy),
        value: word_from_u256(tx.value()),
        calldata: tx.input().as_ref().to_vec(),
        gas_price: Word::from_u128(
            AlloyTransaction::gas_price(&tx)
                .unwrap_or_else(|| AlloyTransaction::max_fee_per_gas(&tx)),
        ),
        nonce: Some(tx.nonce()),
    })
}

fn tx_hash_from_b256(hash: B256) -> TxHash {
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(hash.as_slice());
    TxHash::new(bytes)
}

fn address_from_alloy(address: AlloyAddress) -> Address {
    let mut bytes = [0_u8; 20];
    bytes.copy_from_slice(address.as_slice());
    Address::new(bytes)
}

fn word_from_u256(value: U256) -> Word {
    Word::new(value.to_be_bytes())
}
