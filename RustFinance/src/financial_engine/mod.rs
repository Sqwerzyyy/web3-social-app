#[cfg(test)]
mod tests;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::debug;
use uuid::Uuid;

pub const MAX_ASSET_DECIMALS: u32 = 18;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AssetId(pub u64);

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AccountId(pub u64);

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TxId(pub u64);

impl fmt::Display for TxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    Fiat,
    Crypto,
    RwaToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetStatus {
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id:       AssetId,
    pub ticker:   String,
    pub decimals: u32,
    pub kind:     AssetKind,
    pub status:   AssetStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountType {
    User,
    Escrow,
    Revenue,
    System,
}

impl AccountType {
    fn can_overdraw(self) -> bool { matches!(self, Self::System) }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id:           AccountId,
    pub owner_id:     Option<Uuid>,
    pub asset_id:     AssetId,
    pub account_type: AccountType,
    pub balance:      Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryDirection {
    Debit,
    Credit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending,
    Posted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionKind {
    Transfer,
    LockFunds,
    RwaMintSettlement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub tx_id:      TxId,
    pub account_id: AccountId,
    pub amount:     Decimal,
    pub direction:  EntryDirection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub id:        TxId,
    pub timestamp: DateTime<Utc>,
    pub status:    TransactionStatus,
    pub kind:      TransactionKind,
    pub entries:   Vec<Entry>,
}

#[derive(Debug, Error)]
pub enum FinancialError {
    #[error("asset not found: {0}")]
    AssetNotFound(AssetId),

    #[error("account not found: {0}")]
    AccountNotFound(AccountId),

    #[error("asset is not active: {0}")]
    AssetInactive(AssetId),

    #[error("invalid ticker")]
    InvalidTicker,

    #[error("asset decimals must be between 0 and 18")]
    InvalidDecimals,

    #[error("amount must be positive")]
    InvalidAmount,

    #[error("amount precision {scale} exceeds asset decimals {decimals} for asset {asset_id}")]
    AmountPrecisionTooHigh { asset_id: AssetId, scale: u32, decimals: u32 },

    #[error("accounts use different assets: {from_asset_id} and {to_asset_id}")]
    AccountAssetMismatch { from_asset_id: AssetId, to_asset_id: AssetId },

    #[error("invalid account type for account {account_id}")]
    InvalidAccountType { account_id: AccountId },

    #[error("invalid account owner")]
    InvalidAccountOwner,

    #[error("account owner mismatch")]
    AccountOwnerMismatch,

    #[error("user account not found for user {user_id} and asset {asset_id}")]
    UserAccountNotFound { user_id: Uuid, asset_id: AssetId },

    #[error("insufficient funds in account {account_id}: available {available}, required {required}")]
    InsufficientFunds { account_id: AccountId, available: Decimal, required: Decimal },

    #[error("transaction must have at least two entries")]
    TooFewEntries,

    #[error("unbalanced transaction for asset {asset_id}: debits {debits}, credits {credits}")]
    UnbalancedTransaction { asset_id: AssetId, debits: Decimal, credits: Decimal },

    #[error("no eligible escrow account found")]
    NoEligibleEscrow,

    #[error("multiple eligible escrow accounts found")]
    AmbiguousEscrow,

    #[error("asset must be an RWA token: {0}")]
    RwaAssetRequired(AssetId),

    #[error("asset must be fiat or crypto: {0}")]
    SettlementAssetRequired(AssetId),

    #[error("arithmetic overflow")]
    Overflow,

    #[error("state lock poisoned")]
    LockPoisoned,
}

pub trait LedgerClock: Clone + Send + Sync + 'static {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl LedgerClock for SystemClock {
    fn now(&self) -> DateTime<Utc> { Utc::now() }
}

#[derive(Debug)]
pub struct FinancialEngine<C = SystemClock> {
    state: Arc<RwLock<FinancialState>>,
    clock: C,
}

impl FinancialEngine<SystemClock> {
    pub fn new() -> Self { Self::with_clock(SystemClock) }
}

impl Default for FinancialEngine<SystemClock> {
    fn default() -> Self { Self::new() }
}

impl<C: Clone> Clone for FinancialEngine<C> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            clock: self.clock.clone(),
        }
    }
}

impl<C: LedgerClock> FinancialEngine<C> {
    pub fn with_clock(clock: C) -> Self {
        Self {
            state: Arc::new(RwLock::new(FinancialState::default())),
            clock,
        }
    }

    pub fn register_asset(
        &self,
        ticker: impl AsRef<str>,
        kind: AssetKind,
        decimals: u32,
    ) -> Result<Asset, FinancialError> {
        if decimals > MAX_ASSET_DECIMALS {
            return Err(FinancialError::InvalidDecimals);
        }

        let ticker = normalize_ticker(ticker.as_ref())?;
        let mut state = self.state.write().map_err(|_| FinancialError::LockPoisoned)?;
        let id = state.next_asset_id()?;
        let asset = Asset {
            id,
            ticker,
            decimals,
            kind,
            status: AssetStatus::Active,
        };

        state.assets.insert(id, asset.clone());
        Ok(asset)
    }

    pub fn set_asset_status(
        &self,
        asset_id: AssetId,
        status: AssetStatus,
    ) -> Result<Asset, FinancialError> {
        let mut state = self.state.write().map_err(|_| FinancialError::LockPoisoned)?;
        let asset = state
            .assets
            .get_mut(&asset_id)
            .ok_or(FinancialError::AssetNotFound(asset_id))?;
        asset.status = status;
        Ok(asset.clone())
    }

    pub fn create_account(
        &self,
        user_id: Uuid,
        asset_id: AssetId,
    ) -> Result<Account, FinancialError> {
        self.create_account_with_type(Some(user_id), asset_id, AccountType::User)
    }

    pub fn create_account_with_type(
        &self,
        owner_id: Option<Uuid>,
        asset_id: AssetId,
        account_type: AccountType,
    ) -> Result<Account, FinancialError> {
        validate_owner(owner_id, account_type)?;

        let mut state = self.state.write().map_err(|_| FinancialError::LockPoisoned)?;
        state.asset(asset_id)?;
        state.create_account(owner_id, asset_id, account_type)
    }

    pub fn execute_transfer(
        &self,
        from: AccountId,
        to: AccountId,
        amount: Decimal,
    ) -> Result<TxId, FinancialError> {
        let timestamp = self.clock.now();
        let mut state = self.state.write().map_err(|_| FinancialError::LockPoisoned)?;
        let tx = state.execute_transfer_at(from, to, amount, timestamp)?;
        debug!(tx_id = %tx.id, from = %from, to = %to, "transfer posted");
        Ok(tx.id)
    }

    pub fn lock_funds(
        &self,
        from: AccountId,
        escrow: AccountId,
        amount: Decimal,
    ) -> Result<TxId, FinancialError> {
        let timestamp = self.clock.now();
        let mut state = self.state.write().map_err(|_| FinancialError::LockPoisoned)?;
        let tx = state.lock_funds_at(from, escrow, amount, timestamp)?;
        debug!(tx_id = %tx.id, from = %from, escrow = %escrow, "funds locked");
        Ok(tx.id)
    }

    pub fn settle_rwa_mint(
        &self,
        user_id: Uuid,
        rwa_asset_id: AssetId,
        asset_amount: Decimal,
        fiat_cost: Decimal,
    ) -> Result<TxId, FinancialError> {
        if fiat_cost <= Decimal::ZERO {
            return Err(FinancialError::InvalidAmount);
        }

        let timestamp = self.clock.now();
        let mut state = self.state.write().map_err(|_| FinancialError::LockPoisoned)?;
        let escrow = state.find_single_eligible_escrow(user_id, fiat_cost)?;
        let tx = state.settle_rwa_mint_at(
            user_id,
            escrow,
            rwa_asset_id,
            asset_amount,
            fiat_cost,
            timestamp,
        )?;
        debug!(tx_id = %tx.id, user_id = %user_id, rwa_asset_id = %rwa_asset_id, "RWA mint settled");
        Ok(tx.id)
    }

    pub fn settle_rwa_mint_from_escrow(
        &self,
        user_id: Uuid,
        escrow: AccountId,
        rwa_asset_id: AssetId,
        asset_amount: Decimal,
        fiat_cost: Decimal,
    ) -> Result<TxId, FinancialError> {
        let timestamp = self.clock.now();
        let mut state = self.state.write().map_err(|_| FinancialError::LockPoisoned)?;
        let tx = state.settle_rwa_mint_at(
            user_id,
            escrow,
            rwa_asset_id,
            asset_amount,
            fiat_cost,
            timestamp,
        )?;
        debug!(tx_id = %tx.id, user_id = %user_id, escrow = %escrow, "RWA mint settled");
        Ok(tx.id)
    }

    pub fn account(&self, account_id: AccountId) -> Result<Account, FinancialError> {
        let state = self.state.read().map_err(|_| FinancialError::LockPoisoned)?;
        state
            .accounts
            .get(&account_id)
            .cloned()
            .ok_or(FinancialError::AccountNotFound(account_id))
    }

    pub fn asset(&self, asset_id: AssetId) -> Result<Asset, FinancialError> {
        let state = self.state.read().map_err(|_| FinancialError::LockPoisoned)?;
        state
            .assets
            .get(&asset_id)
            .cloned()
            .ok_or(FinancialError::AssetNotFound(asset_id))
    }

    pub fn transaction(&self, tx_id: TxId) -> Option<Transaction> {
        self.state
            .read()
            .ok()
            .and_then(|state| state.transactions.get(&tx_id).cloned())
    }

    pub fn account_balance(&self, account_id: AccountId) -> Result<Decimal, FinancialError> {
        Ok(self.account(account_id)?.balance)
    }

    pub fn total_balance_for_asset(&self, asset_id: AssetId) -> Result<Decimal, FinancialError> {
        let state = self.state.read().map_err(|_| FinancialError::LockPoisoned)?;
        state.total_balance_for_asset(asset_id)
    }

    pub fn verify_ledger(&self) -> Result<(), FinancialError> {
        let state = self.state.read().map_err(|_| FinancialError::LockPoisoned)?;
        state.verify_ledger()
    }
}

#[derive(Debug)]
struct FinancialState {
    assets:       BTreeMap<AssetId, Asset>,
    accounts:     BTreeMap<AccountId, Account>,
    transactions: BTreeMap<TxId, Transaction>,
    next_asset:   u64,
    next_account: u64,
    next_tx:      u64,
}

impl Default for FinancialState {
    fn default() -> Self {
        Self {
            assets:       BTreeMap::new(),
            accounts:     BTreeMap::new(),
            transactions: BTreeMap::new(),
            next_asset:   1,
            next_account: 1,
            next_tx:      1,
        }
    }
}

impl FinancialState {
    fn next_asset_id(&mut self) -> Result<AssetId, FinancialError> {
        let id = AssetId(self.next_asset);
        self.next_asset = self.next_asset.checked_add(1).ok_or(FinancialError::Overflow)?;
        Ok(id)
    }

    fn next_account_id(&mut self) -> Result<AccountId, FinancialError> {
        let id = AccountId(self.next_account);
        self.next_account = self.next_account.checked_add(1).ok_or(FinancialError::Overflow)?;
        Ok(id)
    }

    fn next_tx_id(&mut self) -> Result<TxId, FinancialError> {
        let id = TxId(self.next_tx);
        self.next_tx = self.next_tx.checked_add(1).ok_or(FinancialError::Overflow)?;
        Ok(id)
    }

    fn asset(&self, asset_id: AssetId) -> Result<&Asset, FinancialError> {
        self.assets
            .get(&asset_id)
            .ok_or(FinancialError::AssetNotFound(asset_id))
    }

    fn account(&self, account_id: AccountId) -> Result<&Account, FinancialError> {
        self.accounts
            .get(&account_id)
            .ok_or(FinancialError::AccountNotFound(account_id))
    }

    fn create_account(
        &mut self,
        owner_id: Option<Uuid>,
        asset_id: AssetId,
        account_type: AccountType,
    ) -> Result<Account, FinancialError> {
        let id = self.next_account_id()?;
        let account = Account {
            id,
            owner_id,
            asset_id,
            account_type,
            balance: Decimal::ZERO,
        };

        self.accounts.insert(id, account.clone());
        Ok(account)
    }

    fn execute_transfer_at(
        &mut self,
        from: AccountId,
        to: AccountId,
        amount: Decimal,
        timestamp: DateTime<Utc>,
    ) -> Result<Transaction, FinancialError> {
        self.ensure_same_asset(from, to)?;
        self.post_transaction(
            TransactionKind::Transfer,
            timestamp,
            vec![
                NewEntry { account_id: from, amount, direction: EntryDirection::Credit },
                NewEntry { account_id: to, amount, direction: EntryDirection::Debit },
            ],
        )
    }

    fn lock_funds_at(
        &mut self,
        from: AccountId,
        escrow: AccountId,
        amount: Decimal,
        timestamp: DateTime<Utc>,
    ) -> Result<Transaction, FinancialError> {
        self.ensure_same_asset(from, escrow)?;

        let source = self.account(from)?;
        let target = self.account(escrow)?;
        if source.account_type != AccountType::User {
            return Err(FinancialError::InvalidAccountType { account_id: from });
        }
        if target.account_type != AccountType::Escrow {
            return Err(FinancialError::InvalidAccountType { account_id: escrow });
        }
        if source.owner_id != target.owner_id {
            return Err(FinancialError::AccountOwnerMismatch);
        }

        self.post_transaction(
            TransactionKind::LockFunds,
            timestamp,
            vec![
                NewEntry { account_id: from, amount, direction: EntryDirection::Credit },
                NewEntry { account_id: escrow, amount, direction: EntryDirection::Debit },
            ],
        )
    }

    fn settle_rwa_mint_at(
        &mut self,
        user_id: Uuid,
        escrow: AccountId,
        rwa_asset_id: AssetId,
        asset_amount: Decimal,
        fiat_cost: Decimal,
        timestamp: DateTime<Utc>,
    ) -> Result<Transaction, FinancialError> {
        let rwa_asset = self.asset(rwa_asset_id)?;
        if rwa_asset.kind != AssetKind::RwaToken {
            return Err(FinancialError::RwaAssetRequired(rwa_asset_id));
        }

        validate_amount(rwa_asset, asset_amount)?;

        let escrow_account = self.account(escrow)?;
        if escrow_account.account_type != AccountType::Escrow {
            return Err(FinancialError::InvalidAccountType { account_id: escrow });
        }
        if escrow_account.owner_id != Some(user_id) {
            return Err(FinancialError::AccountOwnerMismatch);
        }

        let settlement_asset = self.asset(escrow_account.asset_id)?;
        if !matches!(settlement_asset.kind, AssetKind::Fiat | AssetKind::Crypto) {
            return Err(FinancialError::SettlementAssetRequired(settlement_asset.id));
        }
        validate_amount(settlement_asset, fiat_cost)?;

        let user_rwa = self
            .find_account(Some(user_id), rwa_asset_id, AccountType::User)
            .ok_or(FinancialError::UserAccountNotFound { user_id, asset_id: rwa_asset_id })?;
        let platform = self.ensure_account(None, escrow_account.asset_id, AccountType::Revenue)?;
        let system_rwa = self.ensure_account(None, rwa_asset_id, AccountType::System)?;

        self.post_transaction(
            TransactionKind::RwaMintSettlement,
            timestamp,
            vec![
                NewEntry { account_id: escrow, amount: fiat_cost, direction: EntryDirection::Credit },
                NewEntry { account_id: platform, amount: fiat_cost, direction: EntryDirection::Debit },
                NewEntry { account_id: user_rwa, amount: asset_amount, direction: EntryDirection::Debit },
                NewEntry { account_id: system_rwa, amount: asset_amount, direction: EntryDirection::Credit },
            ],
        )
    }

    fn ensure_same_asset(
        &self,
        from: AccountId,
        to: AccountId,
    ) -> Result<AssetId, FinancialError> {
        let from_asset_id = self.account(from)?.asset_id;
        let to_asset_id = self.account(to)?.asset_id;
        if from_asset_id != to_asset_id {
            return Err(FinancialError::AccountAssetMismatch { from_asset_id, to_asset_id });
        }

        Ok(from_asset_id)
    }

    fn ensure_account(
        &mut self,
        owner_id: Option<Uuid>,
        asset_id: AssetId,
        account_type: AccountType,
    ) -> Result<AccountId, FinancialError> {
        if let Some(account_id) = self.find_account(owner_id, asset_id, account_type) {
            return Ok(account_id);
        }

        Ok(self.create_account(owner_id, asset_id, account_type)?.id)
    }

    fn find_account(
        &self,
        owner_id: Option<Uuid>,
        asset_id: AssetId,
        account_type: AccountType,
    ) -> Option<AccountId> {
        self.accounts
            .iter()
            .find(|(_, account)| {
                account.owner_id == owner_id
                    && account.asset_id == asset_id
                    && account.account_type == account_type
            })
            .map(|(id, _)| *id)
    }

    fn find_single_eligible_escrow(
        &self,
        user_id: Uuid,
        fiat_cost: Decimal,
    ) -> Result<AccountId, FinancialError> {
        let mut matches = self
            .accounts
            .iter()
            .filter_map(|(id, account)| {
                if account.owner_id != Some(user_id) || account.account_type != AccountType::Escrow {
                    return None;
                }

                let asset = self.assets.get(&account.asset_id)?;
                let scale_ok = validate_amount(asset, fiat_cost).is_ok();
                let kind_ok = matches!(asset.kind, AssetKind::Fiat | AssetKind::Crypto);

                (scale_ok && kind_ok && account.balance >= fiat_cost).then_some(*id)
            });

        let first = matches.next().ok_or(FinancialError::NoEligibleEscrow)?;
        if matches.next().is_some() {
            return Err(FinancialError::AmbiguousEscrow);
        }

        Ok(first)
    }

    fn post_transaction(
        &mut self,
        kind: TransactionKind,
        timestamp: DateTime<Utc>,
        entries: Vec<NewEntry>,
    ) -> Result<Transaction, FinancialError> {
        if entries.len() < 2 {
            return Err(FinancialError::TooFewEntries);
        }

        let mut by_asset: BTreeMap<AssetId, (Decimal, Decimal)> = BTreeMap::new();
        let mut deltas: BTreeMap<AccountId, Decimal> = BTreeMap::new();

        for entry in &entries {
            let account = self.account(entry.account_id)?;
            let asset = self.asset(account.asset_id)?;
            if asset.status != AssetStatus::Active {
                return Err(FinancialError::AssetInactive(asset.id));
            }

            validate_amount(asset, entry.amount)?;

            let totals = by_asset
                .entry(account.asset_id)
                .or_insert((Decimal::ZERO, Decimal::ZERO));
            match entry.direction {
                EntryDirection::Debit => {
                    totals.0 = totals.0.checked_add(entry.amount).ok_or(FinancialError::Overflow)?;
                }
                EntryDirection::Credit => {
                    totals.1 = totals.1.checked_add(entry.amount).ok_or(FinancialError::Overflow)?;
                }
            }

            let delta = deltas.entry(entry.account_id).or_insert(Decimal::ZERO);
            *delta = match entry.direction {
                EntryDirection::Debit => delta.checked_add(entry.amount).ok_or(FinancialError::Overflow)?,
                EntryDirection::Credit => delta.checked_sub(entry.amount).ok_or(FinancialError::Overflow)?,
            };
        }

        for (asset_id, (debits, credits)) in &by_asset {
            if debits != credits {
                return Err(FinancialError::UnbalancedTransaction {
                    asset_id: *asset_id,
                    debits: *debits,
                    credits: *credits,
                });
            }
        }

        for (account_id, delta) in &deltas {
            let account = self.account(*account_id)?;
            let next = account.balance.checked_add(*delta).ok_or(FinancialError::Overflow)?;
            if !account.account_type.can_overdraw() && next < Decimal::ZERO {
                let required = Decimal::ZERO.checked_sub(*delta).ok_or(FinancialError::Overflow)?;
                return Err(FinancialError::InsufficientFunds {
                    account_id: *account_id,
                    available: account.balance,
                    required,
                });
            }
        }

        let tx_id = self.next_tx_id()?;
        for (account_id, delta) in &deltas {
            let account = self
                .accounts
                .get_mut(account_id)
                .ok_or(FinancialError::AccountNotFound(*account_id))?;
            account.balance = account.balance.checked_add(*delta).ok_or(FinancialError::Overflow)?;
        }

        let entries = entries
            .into_iter()
            .map(|entry| Entry {
                tx_id,
                account_id: entry.account_id,
                amount: entry.amount,
                direction: entry.direction,
            })
            .collect();

        let tx = Transaction {
            id: tx_id,
            timestamp,
            status: TransactionStatus::Posted,
            kind,
            entries,
        };

        self.transactions.insert(tx_id, tx.clone());
        Ok(tx)
    }

    fn total_balance_for_asset(&self, asset_id: AssetId) -> Result<Decimal, FinancialError> {
        self.asset(asset_id)?;
        self.accounts
            .values()
            .filter(|account| account.asset_id == asset_id)
            .try_fold(Decimal::ZERO, |sum, account| {
                sum.checked_add(account.balance).ok_or(FinancialError::Overflow)
            })
    }

    fn verify_ledger(&self) -> Result<(), FinancialError> {
        for tx in self.transactions.values() {
            let mut by_asset: BTreeMap<AssetId, (Decimal, Decimal)> = BTreeMap::new();
            for entry in &tx.entries {
                let account = self.account(entry.account_id)?;
                let totals = by_asset
                    .entry(account.asset_id)
                    .or_insert((Decimal::ZERO, Decimal::ZERO));
                match entry.direction {
                    EntryDirection::Debit => {
                        totals.0 = totals.0.checked_add(entry.amount).ok_or(FinancialError::Overflow)?;
                    }
                    EntryDirection::Credit => {
                        totals.1 = totals.1.checked_add(entry.amount).ok_or(FinancialError::Overflow)?;
                    }
                }
            }

            for (asset_id, (debits, credits)) in by_asset {
                if debits != credits {
                    return Err(FinancialError::UnbalancedTransaction { asset_id, debits, credits });
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct NewEntry {
    account_id: AccountId,
    amount:     Decimal,
    direction:  EntryDirection,
}

fn normalize_ticker(ticker: &str) -> Result<String, FinancialError> {
    let ticker = ticker.trim().to_uppercase();
    let valid = !ticker.is_empty()
        && ticker.len() <= 32
        && ticker
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));

    valid.then_some(ticker).ok_or(FinancialError::InvalidTicker)
}

fn validate_owner(owner_id: Option<Uuid>, account_type: AccountType) -> Result<(), FinancialError> {
    match account_type {
        AccountType::User | AccountType::Escrow if owner_id.is_none() => {
            Err(FinancialError::InvalidAccountOwner)
        }
        AccountType::Revenue | AccountType::System if owner_id.is_some() => {
            Err(FinancialError::InvalidAccountOwner)
        }
        _ => Ok(()),
    }
}

fn validate_amount(asset: &Asset, amount: Decimal) -> Result<(), FinancialError> {
    if amount <= Decimal::ZERO {
        return Err(FinancialError::InvalidAmount);
    }

    let scale = amount.normalize().scale();
    if scale > asset.decimals {
        return Err(FinancialError::AmountPrecisionTooHigh {
            asset_id: asset.id,
            scale,
            decimals: asset.decimals,
        });
    }

    Ok(())
}
