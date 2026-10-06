//! secp256k1 adapter for the TLT covenant script interpreter.
//!
//! Signatures use [`crate::KeyPair`], which hashes the covenant sighash preimage
//! with SHA-256 before ECDSA. This is the same audited path as v1 transfers.

use agora_types::{
    eval_covenant_input, SigChecker, TltCovenantTx, TltOutputOrigin, TltScriptError,
};

use crate::{CryptoError, KeyPair};

struct SecpChecker;

impl SigChecker for SecpChecker {
    fn check_sig(&self, pubkey: &[u8], signature: &[u8], message: &[u8]) -> bool {
        if pubkey.len() != 33 || signature.len() != 64 {
            return false;
        }
        let mut pubkey_bytes = [0u8; 33];
        let mut signature_bytes = [0u8; 64];
        pubkey_bytes.copy_from_slice(pubkey);
        signature_bytes.copy_from_slice(signature);
        KeyPair::verify(&pubkey_bytes, message, &signature_bytes).is_ok()
    }
}

/// Verify one covenant input with secp256k1 CHECKSIG / CHECKMULTISIG.
pub fn verify_tlt_covenant_input(
    tx: &TltCovenantTx,
    input_index: usize,
    script_pubkey: &[u8],
    spend_blue_score: u64,
    median_time_past_secs: u64,
    origin: TltOutputOrigin,
) -> Result<(), TltScriptError> {
    eval_covenant_input(
        tx,
        input_index,
        script_pubkey,
        spend_blue_score,
        median_time_past_secs,
        origin,
        &SecpChecker,
    )
}

/// Sign `message` with `keypair`. Callers pass [`TltCovenantTx::sighash_preimage`].
pub fn sign_tlt_covenant_preimage(
    keypair: &KeyPair,
    preimage: &[u8],
) -> Result<[u8; 64], CryptoError> {
    keypair.sign(preimage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bip44::{derive_bip44, Bip44Path};
    use crate::mnemonic::seed_from_mnemonic;
    use agora_types::{
        push_data, script_p2pkh, Amount, Hash, OutPoint, TltCovenantInput, TltCovenantOutput,
        TltCovenantTx, TLT_COVENANT_TX_VERSION, TLT_SEQUENCE_FINAL,
    };

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn secp256k1_p2pkh_style_covenant_spend() {
        let seed = seed_from_mnemonic(PHRASE, "").unwrap();
        let key = derive_bip44(&seed, &Bip44Path::external(0)).unwrap();
        let script_pubkey = script_p2pkh(&key.address());
        let mut tx = TltCovenantTx {
            version: TLT_COVENANT_TX_VERSION,
            inputs: vec![TltCovenantInput {
                previous_outpoint: OutPoint {
                    tx_id: Hash([4u8; 32]),
                    index: 0,
                },
                sequence: TLT_SEQUENCE_FINAL,
                script_sig: Vec::new(),
            }],
            outputs: vec![TltCovenantOutput {
                value: Amount::from_base_units(1),
                script_pubkey: script_pubkey.clone(),
            }],
            lock_time: 0,
            nonce: 9,
        };
        let preimage = tx.sighash_preimage();
        let signature = sign_tlt_covenant_preimage(&key, &preimage).unwrap();
        let mut script_sig = Vec::new();
        push_data(&mut script_sig, &signature).unwrap();
        push_data(&mut script_sig, &key.public_key_bytes()).unwrap();
        tx.inputs[0].script_sig = script_sig;
        verify_tlt_covenant_input(
            &tx,
            0,
            &script_pubkey,
            1,
            0,
            TltOutputOrigin {
                blue_score: 0,
                median_time_secs: 0,
            },
        )
        .unwrap();
        tx.outputs[0].value = Amount::from_base_units(2);
        assert!(verify_tlt_covenant_input(
            &tx,
            0,
            &script_pubkey,
            1,
            0,
            TltOutputOrigin {
                blue_score: 0,
                median_time_secs: 0,
            },
        )
        .is_err());
    }
}
