#![allow(clippy::result_large_err)]

#[cfg(test)]
mod tests;

use crate::financial_engine::{
    AccountId, AssetId, FinancialEngine as LedgerEngine, FinancialError, LedgerClock, SystemClock,
};
use crate::oracle::{OracleError, RiskEngine};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

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

pub trait IdempotencyStore: Send + Sync {
    fn execute_once(
        &self,
        key: Uuid,
        operation: IdempotencyOperation,
        execute: &mut dyn FnMut() -> Result<IdempotentResponse, Status>,
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

impl IdempotencyStore for InMemoryIdempotencyStore {
    fn execute_once(
        &self,
        key: Uuid,
        operation: IdempotencyOperation,
        execute: &mut dyn FnMut() -> Result<IdempotentResponse, Status>,
    ) -> Result<(IdempotentResponse, bool), Status> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| Status::internal("idempotency store lock poisoned"))?;
        match records.get(&key) {
            Some(response) if response.operation() == operation => Ok((response.clone(), true)),
            Some(_) => Err(Status::failed_precondition(
                "idempotency key reused for another operation",
            )),
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

    async fn lock_funds(
        &self,
        request: Request<LockFundsRequest>,
    ) -> Result<Response<LockFundsResponse>, Status> {
        self.ensure_trading_open()?;
        let request = request.into_inner();
        let key = parse_uuid(&request.idempotency_key, "idempotency_key")?;
        let amount = parse_decimal_field(request.amount.as_ref(), "amount")?;

        let (response, replay) =
            self.idempotency
                .execute_once(key, IdempotencyOperation::LockFunds, &mut || {
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
                })?;
        let IdempotentResponse::LockFunds(mut response) = response else {
            return Err(Status::internal("idempotency response type mismatch"));
        };
        response.idempotent_replay = replay;

        Ok(Response::new(response))
    }

    async fn execute_settlement(
        &self,
        request: Request<ExecuteSettlementRequest>,
    ) -> Result<Response<ExecuteSettlementResponse>, Status> {
        self.ensure_trading_open()?;
        let request = request.into_inner();
        let key = parse_uuid(&request.idempotency_key, "idempotency_key")?;
        let user_id = parse_uuid(&request.user_id, "user_id")?;
        let asset_amount = parse_decimal_field(request.asset_amount.as_ref(), "asset_amount")?;
        let fiat_cost = parse_decimal_field(request.fiat_cost.as_ref(), "fiat_cost")?;

        let (response, replay) = self.idempotency.execute_once(
            key,
            IdempotencyOperation::ExecuteSettlement,
            &mut || {
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
            },
        )?;
        let IdempotentResponse::ExecuteSettlement(mut response) = response else {
            return Err(Status::internal("idempotency response type mismatch"));
        };
        response.idempotent_replay = replay;

        Ok(Response::new(response))
    }
}

fn decimal_string(value: Decimal) -> DecimalString {
    DecimalString {
        value: value.normalize().to_string(),
    }
}

fn parse_decimal_field(value: Option<&DecimalString>, field: &str) -> Result<Decimal, Status> {
    value
        .ok_or_else(|| Status::invalid_argument(format!("{field} is required")))?
        .value
        .parse::<Decimal>()
        .map_err(|_| Status::invalid_argument(format!("{field} must be a decimal string")))
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(value).map_err(|_| Status::invalid_argument(format!("{field} must be UUID")))
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
