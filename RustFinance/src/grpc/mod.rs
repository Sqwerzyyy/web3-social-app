#![allow(clippy::result_large_err)]

#[cfg(test)]
mod tests;

use crate::financial_engine::{
    AccountId, AssetId, FinancialEngine as LedgerEngine, FinancialError, LedgerClock, SystemClock,
};
use crate::oracle::{OracleError, RiskEngine};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tracing::{info_span, Instrument};
use uuid::{Uuid, Version};

pub mod proto {
    tonic::include_proto!("rwa.financial.v1");
}

use proto::financial_engine_server::{
    FinancialEngine as FinancialEngineRpc, FinancialEngineServer,
};
use proto::{
    DecimalString, ExecuteSettlementRequest, ExecuteSettlementResponse, GetBalanceRequest,
    GetBalanceResponse, LockFundsRequest, LockFundsResponse,
};

#[derive(Debug, Clone)]
pub enum IdempotentResponse {
    LockFunds(LockFundsResponse),
    ExecuteSettlement(ExecuteSettlementResponse),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyOperation {
    LockFunds,
    ExecuteSettlement,
}

#[cfg(feature = "postgres-idempotency")]
impl IdempotencyOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::LockFunds => "LockFunds",
            Self::ExecuteSettlement => "ExecuteSettlement",
        }
    }

    fn from_str(value: &str) -> Result<Self, RpcError> {
        match value {
            "LockFunds" => Ok(Self::LockFunds),
            "ExecuteSettlement" => Ok(Self::ExecuteSettlement),
            _ => Err(RpcError::InvalidCachedResponse),
        }
    }
}

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("{field} is required")]
    MissingDecimal { field: &'static str },

    #[error("{field} must be a decimal string")]
    InvalidDecimal { field: &'static str },

    #[error("{field} must be UUID")]
    InvalidUuid { field: &'static str },

    #[error("{field} must be UUID v4")]
    InvalidUuidVersion { field: &'static str },

    #[error("idempotency key reused for another operation")]
    IdempotencyKeyConflict,

    #[error("idempotency response type mismatch")]
    IdempotencyResponseMismatch,

    #[error("idempotency store lock poisoned")]
    IdempotencyStorePoisoned,

    #[error("invalid cached idempotency response")]
    InvalidCachedResponse,

    #[error("database error: {0}")]
    Database(String),
}

impl From<RpcError> for Status {
    fn from(error: RpcError) -> Self {
        match error {
            RpcError::MissingDecimal { .. }
            | RpcError::InvalidDecimal { .. }
            | RpcError::InvalidUuid { .. }
            | RpcError::InvalidUuidVersion { .. } => Status::invalid_argument(error.to_string()),
            RpcError::IdempotencyKeyConflict => Status::failed_precondition(error.to_string()),
            RpcError::IdempotencyResponseMismatch
            | RpcError::IdempotencyStorePoisoned
            | RpcError::InvalidCachedResponse
            | RpcError::Database(_) => Status::internal(error.to_string()),
        }
    }
}

type IdempotencyCallback<'a> = Box<dyn FnOnce() -> Result<IdempotentResponse, Status> + Send + 'a>;

#[async_trait::async_trait]
pub trait IdempotencyStore: Send + Sync {
    async fn execute_once(
        &self,
        key: Uuid,
        operation: IdempotencyOperation,
        execute: IdempotencyCallback<'_>,
    ) -> Result<(IdempotentResponse, bool), Status>;
}

#[derive(Debug, Default)]
pub struct InMemoryIdempotencyStore {
    records: Mutex<HashMap<Uuid, IdempotentResponse>>,
}

impl InMemoryIdempotencyStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl IdempotencyStore for InMemoryIdempotencyStore {
    async fn execute_once(
        &self,
        key: Uuid,
        operation: IdempotencyOperation,
        execute: IdempotencyCallback<'_>,
    ) -> Result<(IdempotentResponse, bool), Status> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| Status::from(RpcError::IdempotencyStorePoisoned))?;
        match records.get(&key) {
            Some(response) if response.operation() == operation => Ok((response.clone(), true)),
            Some(_) => Err(Status::from(RpcError::IdempotencyKeyConflict)),
            None => {
                let response = execute()?;
                records.insert(key, response.clone());
                Ok((response, false))
            }
        }
    }
}

impl IdempotentResponse {
    fn operation(&self) -> IdempotencyOperation {
        match self {
            Self::LockFunds(_) => IdempotencyOperation::LockFunds,
            Self::ExecuteSettlement(_) => IdempotencyOperation::ExecuteSettlement,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum CachedIdempotentResponse {
    LockFunds { tx_id: u64 },
    ExecuteSettlement { tx_id: u64 },
}

impl From<&IdempotentResponse> for CachedIdempotentResponse {
    fn from(response: &IdempotentResponse) -> Self {
        match response {
            IdempotentResponse::LockFunds(response) => Self::LockFunds {
                tx_id: response.tx_id,
            },
            IdempotentResponse::ExecuteSettlement(response) => Self::ExecuteSettlement {
                tx_id: response.tx_id,
            },
        }
    }
}

impl TryFrom<CachedIdempotentResponse> for IdempotentResponse {
    type Error = RpcError;

    fn try_from(value: CachedIdempotentResponse) -> Result<Self, Self::Error> {
        match value {
            CachedIdempotentResponse::LockFunds { tx_id } => {
                Ok(Self::LockFunds(LockFundsResponse {
                    tx_id,
                    idempotent_replay: false,
                }))
            }
            CachedIdempotentResponse::ExecuteSettlement { tx_id } => {
                Ok(Self::ExecuteSettlement(ExecuteSettlementResponse {
                    tx_id,
                    idempotent_replay: false,
                }))
            }
        }
    }
}

#[cfg(feature = "postgres-idempotency")]
#[derive(Clone)]
pub struct PostgresIdempotencyStore {
    pool: sqlx::PgPool,
}

#[cfg(feature = "postgres-idempotency")]
impl PostgresIdempotencyStore {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "postgres-idempotency")]
#[async_trait::async_trait]
impl IdempotencyStore for PostgresIdempotencyStore {
    async fn execute_once(
        &self,
        key: Uuid,
        operation: IdempotencyOperation,
        execute: IdempotencyCallback<'_>,
    ) -> Result<(IdempotentResponse, bool), Status> {
        use sqlx::Row;

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        sqlx::query(
            r#"
            INSERT INTO grpc_idempotency_keys (idempotency_key, operation, status)
            VALUES ($1, $2, 'Started')
            ON CONFLICT (idempotency_key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(operation.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        // SELECT FOR UPDATE serializes concurrent retries for the same key.
        let row = sqlx::query(
            r#"
            SELECT operation, status, response_json
            FROM grpc_idempotency_keys
            WHERE idempotency_key = $1
            FOR UPDATE
            "#,
        )
        .bind(key)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let stored_operation: String = row.try_get("operation").map_err(map_sqlx_error)?;
        let stored_status: String = row.try_get("status").map_err(map_sqlx_error)?;
        let response_json: Option<serde_json::Value> =
            row.try_get("response_json").map_err(map_sqlx_error)?;
        let stored_operation = IdempotencyOperation::from_str(&stored_operation)?;
        if stored_operation != operation {
            return Err(Status::from(RpcError::IdempotencyKeyConflict));
        }

        if stored_status == "Completed" {
            let value = response_json.ok_or(RpcError::InvalidCachedResponse)?;
            let cached: CachedIdempotentResponse =
                serde_json::from_value(value).map_err(|_| RpcError::InvalidCachedResponse)?;
            let response = IdempotentResponse::try_from(cached)?;
            if response.operation() != operation {
                return Err(Status::from(RpcError::InvalidCachedResponse));
            }
            tx.commit().await.map_err(map_sqlx_error)?;
            return Ok((response, true));
        }

        sqlx::query(
            r#"
            UPDATE grpc_idempotency_keys
            SET status = 'Started', updated_at = NOW()
            WHERE idempotency_key = $1
            "#,
        )
        .bind(key)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let response = match execute() {
            Ok(response) => response,
            Err(status) => {
                sqlx::query(
                    r#"
                    UPDATE grpc_idempotency_keys
                    SET status = 'Failed', updated_at = NOW()
                    WHERE idempotency_key = $1
                    "#,
                )
                .bind(key)
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx_error)?;
                tx.commit().await.map_err(map_sqlx_error)?;
                return Err(status);
            }
        };
        let response_json = serde_json::to_value(CachedIdempotentResponse::from(&response))
            .map_err(|_| Status::from(RpcError::InvalidCachedResponse))?;

        sqlx::query(
            r#"
            UPDATE grpc_idempotency_keys
            SET status = 'Completed', response_json = $2, updated_at = NOW()
            WHERE idempotency_key = $1
            "#,
        )
        .bind(key)
        .bind(response_json)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
        tx.commit().await.map_err(map_sqlx_error)?;
        Ok((response, false))
    }
}

#[cfg(feature = "postgres-idempotency")]
fn map_sqlx_error(error: sqlx::Error) -> Status {
    Status::from(RpcError::Database(error.to_string()))
}

#[derive(Clone)]
pub struct FinancialGrpcBridge<C = SystemClock> {
    ledger: LedgerEngine<C>,
    idempotency: Arc<dyn IdempotencyStore>,
    risk: Option<Arc<RiskEngine>>,
}

impl FinancialGrpcBridge<SystemClock> {
    pub fn new(ledger: LedgerEngine<SystemClock>) -> Self {
        Self::with_idempotency(ledger, Arc::new(InMemoryIdempotencyStore::new()))
    }
}

impl<C: LedgerClock> FinancialGrpcBridge<C> {
    pub fn with_idempotency(
        ledger: LedgerEngine<C>,
        idempotency: Arc<dyn IdempotencyStore>,
    ) -> Self {
        Self {
            ledger,
            idempotency,
            risk: None,
        }
    }

    pub fn with_risk_engine(mut self, risk: Arc<RiskEngine>) -> Self {
        self.risk = Some(risk);
        self
    }

    pub fn into_server(self) -> FinancialEngineServer<Self> {
        FinancialEngineServer::new(self)
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<(), tonic::transport::Error> {
        Server::builder()
            .add_service(self.into_server())
            .serve(addr)
            .await
    }

    fn ensure_trading_open(&self) -> Result<(), Status> {
        if let Some(risk) = &self.risk {
            risk.ensure_trading_open().map_err(map_oracle_error)?;
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl<C> FinancialEngineRpc for FinancialGrpcBridge<C>
where
    C: LedgerClock,
{
    async fn get_balance(
        &self,
        request: Request<GetBalanceRequest>,
    ) -> Result<Response<GetBalanceResponse>, Status> {
        let request = request.into_inner();
        let span = info_span!(
            "grpc.transaction",
            rpc = "GetBalance",
            span_id = %Uuid::new_v4(),
            account_id = request.account_id
        );
        async move {
            let account = self
                .ledger
                .account(AccountId(request.account_id))
                .map_err(map_financial_error)?;

            Ok(Response::new(GetBalanceResponse {
                account_id: account.id.0,
                asset_id: account.asset_id.0,
                balance: Some(decimal_string(account.balance)),
            }))
        }
        .instrument(span)
        .await
    }

    async fn lock_funds(
        &self,
        request: Request<LockFundsRequest>,
    ) -> Result<Response<LockFundsResponse>, Status> {
        let request = request.into_inner();
        let span = info_span!(
            "grpc.transaction",
            rpc = "LockFunds",
            span_id = %Uuid::new_v4(),
            idempotency_key = %request.idempotency_key
        );
        async move {
            self.ensure_trading_open()?;
            let key = parse_uuid_v4(&request.idempotency_key, "idempotency_key")?;
            let amount = parse_decimal_field(request.amount.as_ref(), "amount")?;

            let (response, replay) = self
                .idempotency
                .execute_once(
                    key,
                    IdempotencyOperation::LockFunds,
                    Box::new(move || {
                        let tx_id = self
                            .ledger
                            .lock_funds(
                                AccountId(request.from_account_id),
                                AccountId(request.escrow_account_id),
                                amount,
                            )
                            .map_err(map_financial_error)?;
                        Ok(IdempotentResponse::LockFunds(LockFundsResponse {
                            tx_id: tx_id.0,
                            idempotent_replay: false,
                        }))
                    }),
                )
                .await?;
            let IdempotentResponse::LockFunds(mut response) = response else {
                return Err(Status::from(RpcError::IdempotencyResponseMismatch));
            };
            response.idempotent_replay = replay;

            Ok(Response::new(response))
        }
        .instrument(span)
        .await
    }

    async fn execute_settlement(
        &self,
        request: Request<ExecuteSettlementRequest>,
    ) -> Result<Response<ExecuteSettlementResponse>, Status> {
        let request = request.into_inner();
        let span = info_span!(
            "grpc.transaction",
            rpc = "ExecuteSettlement",
            span_id = %Uuid::new_v4(),
            idempotency_key = %request.idempotency_key
        );
        async move {
            self.ensure_trading_open()?;
            let key = parse_uuid_v4(&request.idempotency_key, "idempotency_key")?;
            let user_id = parse_uuid(&request.user_id, "user_id")?;
            let asset_amount = parse_decimal_field(request.asset_amount.as_ref(), "asset_amount")?;
            let fiat_cost = parse_decimal_field(request.fiat_cost.as_ref(), "fiat_cost")?;

            let (response, replay) = self
                .idempotency
                .execute_once(
                    key,
                    IdempotencyOperation::ExecuteSettlement,
                    Box::new(move || {
                        let tx_id = self
                            .ledger
                            .settle_rwa_mint_from_escrow(
                                user_id,
                                AccountId(request.escrow_account_id),
                                AssetId(request.rwa_asset_id),
                                asset_amount,
                                fiat_cost,
                            )
                            .map_err(map_financial_error)?;
                        Ok(IdempotentResponse::ExecuteSettlement(
                            ExecuteSettlementResponse {
                                tx_id: tx_id.0,
                                idempotent_replay: false,
                            },
                        ))
                    }),
                )
                .await?;
            let IdempotentResponse::ExecuteSettlement(mut response) = response else {
                return Err(Status::from(RpcError::IdempotencyResponseMismatch));
            };
            response.idempotent_replay = replay;

            Ok(Response::new(response))
        }
        .instrument(span)
        .await
    }
}

fn decimal_string(value: Decimal) -> DecimalString {
    DecimalString {
        value: value.normalize().to_string(),
    }
}

fn parse_decimal_field(
    value: Option<&DecimalString>,
    field: &'static str,
) -> Result<Decimal, RpcError> {
    value
        .ok_or(RpcError::MissingDecimal { field })?
        .value
        .parse::<Decimal>()
        .map_err(|_| RpcError::InvalidDecimal { field })
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, RpcError> {
    Uuid::parse_str(value).map_err(|_| RpcError::InvalidUuid { field })
}

fn parse_uuid_v4(value: &str, field: &'static str) -> Result<Uuid, RpcError> {
    let uuid = parse_uuid(value, field)?;
    if uuid.get_version() != Some(Version::Random) {
        return Err(RpcError::InvalidUuidVersion { field });
    }
    Ok(uuid)
}

fn map_financial_error(error: FinancialError) -> Status {
    match error {
        FinancialError::AccountNotFound(_) | FinancialError::AssetNotFound(_) => {
            Status::not_found(error.to_string())
        }
        FinancialError::InsufficientFunds { .. } => Status::failed_precondition(error.to_string()),
        FinancialError::InvalidAmount
        | FinancialError::InvalidDecimals
        | FinancialError::InvalidTicker
        | FinancialError::AmountPrecisionTooHigh { .. } => {
            Status::invalid_argument(error.to_string())
        }
        FinancialError::Overflow | FinancialError::LockPoisoned => {
            Status::internal(error.to_string())
        }
        _ => Status::failed_precondition(error.to_string()),
    }
}

fn map_oracle_error(error: OracleError) -> Status {
    match error {
        OracleError::Halted => Status::failed_precondition(error.to_string()),
        OracleError::StaleData { .. }
        | OracleError::Outlier { .. }
        | OracleError::NoConsensus(_) => Status::failed_precondition(error.to_string()),
        OracleError::MathOverflow | OracleError::LockPoisoned => {
            Status::internal(error.to_string())
        }
        _ => Status::unavailable(error.to_string()),
    }
}
