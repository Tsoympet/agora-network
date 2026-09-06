//! Audited cryptography for Agora Network.
//!
//! All signing uses secp256k1. Do not add custom elliptic-curve constructions here.
//! BIP-44 paths are realized via the `bip32` crate; secrets are imported into `secp256k1`.

mod account;
mod address;
mod attestation;
mod bip44;
mod data_availability;
mod error;
mod execution;
mod keys;
mod mnemonic;
mod passport;
mod payment;
mod stake;
mod transaction;

pub use account::{
    account_signer_address, sign_account_transfer_bound, verify_account_transfer_bound,
};
pub use address::address_from_pubkey;
pub use attestation::{sign_checkpoint_attestation, verify_checkpoint_attestation};
pub use bip44::{
    derive_bip44, Bip44Path, AGORA_COIN_TYPE, AGORA_COIN_TYPE_PROVISIONAL, AGORA_COIN_TYPE_TESTNET,
};
pub use data_availability::{sign_data_commitment_bound, verify_data_commitment_bound};
pub use error::CryptoError;
pub use execution::{sign_ovl_execution_bound, verify_ovl_execution_bound};
pub use keys::{parse_compressed_public_key, KeyPair, PublicKeyBytes, SignatureBytes};
pub use mnemonic::{generate_mnemonic, seed_fingerprint, seed_from_mnemonic};
pub use passport::{sign_passport_attestation_bound, verify_passport_attestation_bound};
pub use payment::{sign_drc_payment_bound, verify_drc_payment_bound};
pub use stake::{sign_stake_tx_bound, verify_stake_tx_bound};
pub use transaction::{
    sign_transaction, sign_transaction_bound, signature_from_slice, signer_address,
    verify_transaction, verify_transaction_bound,
};
