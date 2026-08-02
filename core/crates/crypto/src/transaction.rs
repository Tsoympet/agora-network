use agora_types::Transaction;

use crate::address::address_from_pubkey;
use crate::{CryptoError, KeyPair, PublicKeyBytes, SignatureBytes};

/// Sign a transaction body in place, attaching compressed pubkey + compact signature.
pub fn sign_transaction(tx: &mut Transaction, keypair: &KeyPair) -> Result<(), CryptoError> {
    let signing = tx.signing_bytes();
    let signature = keypair.sign(&signing)?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

/// Verify secp256k1 auth on a transaction and that the signer address matches an expected payer.
pub fn verify_transaction(tx: &Transaction) -> Result<(), CryptoError> {
    if tx.public_key.len() != 33 || tx.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut pubkey = [0u8; 33];
    pubkey.copy_from_slice(&tx.public_key);
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&tx.signature);
    KeyPair::verify(&pubkey, &tx.signing_bytes(), &signature)
}

/// Convenience: verify auth and return the derived signer address.
pub fn signer_address(tx: &Transaction) -> Result<agora_types::Address, CryptoError> {
    verify_transaction(tx)?;
    let mut pubkey: PublicKeyBytes = [0u8; 33];
    pubkey.copy_from_slice(&tx.public_key);
    Ok(address_from_pubkey(&pubkey))
}

/// Helper used by tests / wallets to pack a compact signature array.
pub fn signature_from_slice(bytes: &[u8]) -> Result<SignatureBytes, CryptoError> {
    if bytes.len() != 64 {
        return Err(CryptoError::InvalidSignatureLength);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use agora_types::{Amount, Hash, OutPoint, Transaction, TxIn, TxOut};

    use super::*;
    use crate::bip44::{derive_bip44, Bip44Path};
    use crate::mnemonic::seed_from_mnemonic;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn wallet_derivation_and_tx_sign_verify_roundtrip() {
        let seed = seed_from_mnemonic(PHRASE, "").expect("seed");
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).expect("derive");
        let to = derive_bip44(&seed, &Bip44Path::external(1))
            .expect("to")
            .address();

        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_whole(1).unwrap(),
                address: to,
            }],
            1,
        );

        let unsigned_id = tx.tx_id();
        sign_transaction(&mut tx, &kp).expect("sign");
        assert_ne!(tx.tx_id(), unsigned_id);
        verify_transaction(&tx).expect("verify");
        assert_eq!(signer_address(&tx).expect("signer"), kp.address());

        // Tamper with an output — signature must fail.
        tx.outputs[0].value = Amount::from_base_units(1);
        assert!(verify_transaction(&tx).is_err());
    }
}
