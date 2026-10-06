//! Audited cryptography for Agora Network.
//!
//! All signing uses secp256k1. Do not add custom elliptic-curve constructions here.
//! BIP-44 paths are realized via the `bip32` crate; secrets are imported into `secp256k1`.

mod account;
mod address;
mod attestation;
mod bip44;
mod data_availability;
mod drc_check;
mod drc_deposit_preauth;
mod drc_escrow;
mod drc_issued_controls;
mod drc_multisign;
mod drc_offer;
mod drc_operation;
mod drc_payment_channel;
mod drc_policy;
mod drc_regular_key;
mod drc_signer_list;
mod drc_ticket;
mod drc_trust_line;
mod error;
mod execution;
mod keys;
mod mnemonic;
mod passport;
mod payment;
mod stake;
mod tlt_script;
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
pub use drc_check::{
    sign_drc_check_cancel_bound, sign_drc_check_cash_bound, sign_drc_check_create_bound,
    verify_drc_check_cancel_bound, verify_drc_check_cash_bound, verify_drc_check_create_bound,
};
pub use drc_deposit_preauth::{sign_drc_deposit_preauth_bound, verify_drc_deposit_preauth_bound};
pub use drc_escrow::{
    sign_drc_escrow_cancel_bound, sign_drc_escrow_create_bound, sign_drc_escrow_finish_bound,
    verify_drc_escrow_cancel_bound, verify_drc_escrow_create_bound, verify_drc_escrow_finish_bound,
};
pub use drc_issued_controls::{
    sign_drc_issued_asset_policy_set_bound, sign_drc_issued_clawback_bound,
    sign_drc_trust_line_issuer_control_bound, verify_drc_issued_asset_policy_set_bound,
    verify_drc_issued_clawback_bound, verify_drc_trust_line_issuer_control_bound,
};
pub use drc_multisign::{
    sign_drc_multisign_participant_bound, validate_drc_operation_authorization_fields,
    verify_drc_multisign_against_list,
};
pub use drc_offer::{
    sign_drc_offer_cancel_bound, sign_drc_offer_create_bound, verify_drc_offer_cancel_bound,
    verify_drc_offer_create_bound,
};
pub use drc_operation::verify_bound_secp256k1;
pub use drc_payment_channel::{
    sign_drc_payment_channel_claim_bound, sign_drc_payment_channel_close_bound,
    sign_drc_payment_channel_create_bound, sign_drc_payment_channel_fund_bound,
    sign_payment_channel_offledger_claim, verify_drc_payment_channel_claim_bound,
    verify_drc_payment_channel_close_bound, verify_drc_payment_channel_create_bound,
    verify_drc_payment_channel_fund_bound, verify_payment_channel_offledger_claim,
};
pub use drc_policy::{sign_drc_account_policy_bound, verify_drc_account_policy_bound};
pub use drc_regular_key::{sign_drc_regular_key_bound, verify_drc_regular_key_bound};
pub use drc_signer_list::{
    sign_drc_signer_list_bound, verify_drc_signer_list_single_signature_bound,
};
pub use drc_ticket::{sign_drc_ticket_create_bound, verify_drc_ticket_create_bound};
pub use drc_trust_line::{
    sign_drc_issued_transfer_bound, sign_drc_trust_line_set_bound,
    verify_drc_issued_transfer_bound, verify_drc_trust_line_set_bound,
};
pub use error::CryptoError;
pub use execution::{sign_ovl_execution_bound, verify_ovl_execution_bound};
pub use keys::{parse_compressed_public_key, KeyPair, PublicKeyBytes, SignatureBytes};
pub use mnemonic::{generate_mnemonic, seed_fingerprint, seed_from_mnemonic};
pub use passport::{sign_passport_attestation_bound, verify_passport_attestation_bound};
pub use payment::{sign_drc_payment_bound, verify_drc_payment_bound};
pub use stake::{sign_stake_tx_bound, verify_stake_tx_bound};
pub use tlt_script::{sign_tlt_covenant_preimage, verify_tlt_covenant_input};
pub use transaction::{
    sign_transaction, sign_transaction_bound, signature_from_slice, signer_address,
    verify_transaction, verify_transaction_bound,
};
