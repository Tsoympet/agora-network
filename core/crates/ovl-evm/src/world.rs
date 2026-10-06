//! Canonical OVL execution world.
//!
//! Accounts are Ethereum 20-byte addresses. Balances are OVL wei. This record
//! is the consensus input to the EVM. It is not an ERC-20 balance and it is
//! not an alias of an Agora SHA-256/Bech32 address.

use std::collections::BTreeMap;

use agora_types::{OvlFeeMarketParams, OvlWei, OVL_EVM_DEV_CHAIN_ID};
use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};

use crate::error::EvmError;

pub const EXECUTION_SUBROOT_DOMAIN: &[u8] = b"agora-ovl-evm-subroot-v1";

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvmAccount {
    pub balance: OvlWei,
    pub nonce: u64,
    pub code: Vec<u8>,
    pub storage: BTreeMap<[u8; 32], [u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvmLog {
    pub address: [u8; 20],
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvmReceipt {
    pub hash: [u8; 32],
    pub index: u64,
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub contract_address: Option<[u8; 20]>,
    pub gas_used: u64,
    pub cumulative_gas_used: u64,
    pub status: bool,
    pub effective_gas_price: u128,
    pub logs: Vec<EvmLog>,
    pub output: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvmBlockRecord {
    pub number: u64,
    pub hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub timestamp: u64,
    pub beneficiary: [u8; 20],
    pub base_fee: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub tx_hashes: Vec<[u8; 32]>,
}

/// Selected-order block fields copied into the EVM environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedBlock {
    pub number: u64,
    pub hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub timestamp: u64,
    pub beneficiary: [u8; 20],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OvlEvmWorld {
    pub active: bool,
    pub chain_id: u64,
    pub fee: OvlFeeMarketParams,
    pub base_fee: u64,
    pub block: EvmBlockRecord,
    pub accounts: BTreeMap<[u8; 20], EvmAccount>,
    pub receipts: Vec<EvmReceipt>,
    pub issued: OvlWei,
    pub burned: OvlWei,
    /// Contract-verification inputs. Not a certification and not consensus
    /// truth beyond the bytecode hash the submitter bound.
    pub verification_inputs: BTreeMap<[u8; 20], String>,
}

impl Default for OvlEvmWorld {
    fn default() -> Self {
        Self::inactive()
    }
}

impl OvlEvmWorld {
    pub fn inactive() -> Self {
        let fee = OvlFeeMarketParams::dev_default();
        Self {
            active: false,
            chain_id: 0,
            base_fee: 0,
            block: empty_block(&fee, [0u8; 20]),
            accounts: BTreeMap::new(),
            receipts: Vec::new(),
            issued: OvlWei::ZERO,
            burned: OvlWei::ZERO,
            verification_inputs: BTreeMap::new(),
            fee,
        }
    }

    /// Dev/test activation. Production chain ids are not allocated here.
    pub fn activate(
        chain_id: u64,
        fee: OvlFeeMarketParams,
        beneficiary: [u8; 20],
    ) -> Result<Self, EvmError> {
        if agora_types::ovl_evm_chain_id_rejected(chain_id) {
            return Err(EvmError::rejected(
                "OVL chain id must be a distinct non-zero id other than Ethereum or BSC",
            ));
        }
        fee.validate().map_err(EvmError::rejected)?;
        let base_fee = fee
            .initial_base_fee_wei
            .to_u64()
            .ok_or_else(|| EvmError::rejected("initial base fee exceeds u64"))?;
        let mut block = empty_block(&fee, beneficiary);
        block.base_fee = base_fee;
        Ok(Self {
            active: true,
            chain_id,
            fee,
            base_fee,
            block,
            accounts: BTreeMap::new(),
            receipts: Vec::new(),
            issued: OvlWei::ZERO,
            burned: OvlWei::ZERO,
            verification_inputs: BTreeMap::new(),
        })
    }

    pub fn dev() -> Self {
        Self::activate(
            OVL_EVM_DEV_CHAIN_ID,
            OvlFeeMarketParams::dev_default(),
            [0xB0; 20],
        )
        .expect("dev profile")
    }

    pub fn require_active(&self) -> Result<(), EvmError> {
        if self.active {
            Ok(())
        } else {
            Err(EvmError::rejected("OVL-EVM-v1 dev gate is inactive"))
        }
    }

    pub fn fund(&mut self, address: [u8; 20], amount: OvlWei) -> Result<(), EvmError> {
        self.require_active()?;
        if amount.is_zero() {
            return Err(EvmError::rejected("zero OVL fund"));
        }
        let account = self.accounts.entry(address).or_default();
        account.balance = account
            .balance
            .checked_add(amount)
            .ok_or_else(|| EvmError::rejected("OVL balance overflow"))?;
        self.issued = self
            .issued
            .checked_add(amount)
            .ok_or_else(|| EvmError::rejected("OVL issued overflow"))?;
        self.check_supply()
    }

    pub fn balance(&self, address: &[u8; 20]) -> OvlWei {
        self.accounts
            .get(address)
            .map(|account| account.balance)
            .unwrap_or(OvlWei::ZERO)
    }

    pub fn nonce(&self, address: &[u8; 20]) -> u64 {
        self.accounts
            .get(address)
            .map(|account| account.nonce)
            .unwrap_or(0)
    }

    pub fn code(&self, address: &[u8; 20]) -> &[u8] {
        self.accounts
            .get(address)
            .map(|account| account.code.as_slice())
            .unwrap_or(&[])
    }

    pub fn storage_at(&self, address: &[u8; 20], slot: &[u8; 32]) -> [u8; 32] {
        self.accounts
            .get(address)
            .and_then(|account| account.storage.get(slot).copied())
            .unwrap_or([0u8; 32])
    }

    pub fn open_block(&mut self, selected: SelectedBlock) -> Result<(), EvmError> {
        self.require_active()?;
        if selected.number != self.block.number.saturating_add(1)
            && self.block.gas_used == 0
            && self.block.number == 0
            && self.block.hash == [0u8; 32]
        {
            self.block.number = selected.number;
            self.block.hash = selected.hash;
            self.block.parent_hash = selected.parent_hash;
            self.block.timestamp = selected.timestamp;
            self.block.beneficiary = selected.beneficiary;
            return Ok(());
        }
        let parent_gas = self.block.gas_used;
        let parent_base = self.block.base_fee;
        let next = crate::fee::next_base_fee(parent_base, parent_gas, &self.fee)?;
        self.block = EvmBlockRecord {
            number: selected.number,
            hash: selected.hash,
            parent_hash: selected.parent_hash,
            timestamp: selected.timestamp,
            beneficiary: selected.beneficiary,
            base_fee: next,
            gas_limit: self.fee.gas_limit,
            gas_used: 0,
            tx_hashes: Vec::new(),
        };
        self.base_fee = next;
        Ok(())
    }

    pub fn check_supply(&self) -> Result<(), EvmError> {
        let mut sum = self.burned;
        for account in self.accounts.values() {
            sum = sum
                .checked_add(account.balance)
                .ok_or_else(|| EvmError::rejected("OVL supply sum overflow"))?;
        }
        if sum != self.issued {
            return Err(EvmError::rejected(format!(
                "OVL supply invariant failed: balances+burned={} issued={}",
                sum.to_decimal(),
                self.issued.to_decimal()
            )));
        }
        Ok(())
    }

    pub fn execution_subroot(&self) -> [u8; 32] {
        let bytes = borsh::to_vec(&(
            EXECUTION_SUBROOT_DOMAIN,
            self.active,
            self.chain_id,
            &self.fee,
            self.base_fee,
            &self.block,
            &self.accounts,
            &self.receipts,
            &self.issued,
            &self.burned,
        ))
        .expect("borsh execution subroot");
        let digest = Sha256::digest(bytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("borsh world")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EvmError> {
        Self::try_from_slice(bytes).map_err(|err| EvmError::rejected(format!("world bytes: {err}")))
    }
}

fn empty_block(fee: &OvlFeeMarketParams, beneficiary: [u8; 20]) -> EvmBlockRecord {
    EvmBlockRecord {
        number: 0,
        hash: [0u8; 32],
        parent_hash: [0u8; 32],
        timestamp: 0,
        beneficiary,
        base_fee: fee.initial_base_fee_wei.to_u64().unwrap_or(0),
        gas_limit: fee.gas_limit,
        gas_used: 0,
        tx_hashes: Vec::new(),
    }
}
