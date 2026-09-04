use agora_types::{Hash, Transaction};

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

/// Sign with network binding (`chain_id` + genesis) so the signature cannot be
/// replayed on another Agora network.
pub fn sign_transaction_bound(
    tx: &mut Transaction,
    keypair: &KeyPair,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    let signing = tx.signing_bytes_bound(chain_id, genesis);
    let signature = keypair.sign(&signing)?;
    tx.public_key = keypair.public_key_bytes().to_vec();
    tx.signature = signature.to_vec();
    Ok(())
}

/// Verify secp256k1 auth on a transaction (domain-separated body).
pub fn verify_transaction(tx: &Transaction) -> Result<(), CryptoError> {
    verify_signing(tx, &tx.signing_bytes())
}

/// Verify a network-bound transaction signature.
pub fn verify_transaction_bound(
    tx: &Transaction,
    chain_id: &str,
    genesis: &Hash,
) -> Result<(), CryptoError> {
    verify_signing(tx, &tx.signing_bytes_bound(chain_id, genesis))
}

fn verify_signing(tx: &Transaction, signing: &[u8]) -> Result<(), CryptoError> {
    if tx.public_key.len() != 33 || tx.signature.len() != 64 {
        return Err(CryptoError::InvalidTransactionAuth);
    }
    let mut pubkey = [0u8; 33];
    pubkey.copy_from_slice(&tx.public_key);
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&tx.signature);
    KeyPair::verify(&pubkey, signing, &signature)
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

    #[test]
    fn network_bound_signature_rejects_cross_chain() {
        let seed = seed_from_mnemonic(PHRASE, "").expect("seed");
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).expect("derive");
        let genesis_a = Hash::hash_bytes(b"genesis-a");
        let genesis_b = Hash::hash_bytes(b"genesis-b");
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
                address: kp.address(),
            }],
            1,
        );
        sign_transaction_bound(&mut tx, &kp, "testnet", &genesis_a).unwrap();
        assert!(verify_transaction_bound(&tx, "testnet", &genesis_a).is_ok());
        assert!(verify_transaction_bound(&tx, "testnet", &genesis_b).is_err());
        assert!(verify_transaction_bound(&tx, "mainnet", &genesis_a).is_err());
        // Unbound verify must not accept a bound signature.
        assert!(verify_transaction(&tx).is_err());
    }

    /// Stable cross-language vector: Rust and TypeScript must produce the same
    /// preimage, pubkey, and signature for this fixture (devnet / testnet / mainnet ids).
    #[test]
    fn bound_signing_vector_abandon_external0() {
        let seed = seed_from_mnemonic(PHRASE, "").expect("seed");
        let kp = derive_bip44(&seed, &Bip44Path::external(0)).expect("derive");
        let genesis =
            Hash::from_hex("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
                .unwrap();
        let mut tx = Transaction::unsigned(
            1,
            vec![TxIn {
                previous_outpoint: OutPoint {
                    tx_id: Hash::ZERO,
                    index: 0,
                },
            }],
            vec![TxOut {
                value: Amount::from_base_units(1_000),
                address: kp.address(),
            }],
            42,
        );
        for (chain_id, preimage_prefix) in [
            ("agora-dev", "61676f72612d74782d7631"), // domain utf8 hex starts after len
            ("agora-testnet-1", "61676f72612d74782d7631"),
            ("agora-mainnet-1", "61676f72612d74782d7631"),
        ] {
            let preimage = tx.signing_bytes_bound(chain_id, &genesis);
            let pre_hex = hex::encode(&preimage);
            assert!(
                pre_hex.contains(preimage_prefix),
                "preimage must include domain for {chain_id}"
            );
            sign_transaction_bound(&mut tx, &kp, chain_id, &genesis).unwrap();
            verify_transaction_bound(&tx, chain_id, &genesis).unwrap();
            assert_eq!(
                hex::encode(&tx.public_key),
                hex::encode(kp.public_key_bytes())
            );
            assert_eq!(tx.signature.len(), 64);
            // Re-sign must be deterministic for the same key/preimage (RFC6979).
            let sig1 = tx.signature.clone();
            sign_transaction_bound(&mut tx, &kp, chain_id, &genesis).unwrap();
            assert_eq!(tx.signature, sig1);
        }
        // Fixture values consumed by apps/shared/light-client signing vector smoke.
        let chain_id = "agora-testnet-1";
        let preimage = tx.signing_bytes_bound(chain_id, &genesis);
        assert_eq!(
            hex::encode(&preimage),
            "0b00000061676f72612d74782d76310f00000061676f72612d746573746e65742d310123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef010000000100000000000000000000000000000000000000000000000000000000000000000000000000000001000000e803000000000000ff9ec96f09eb154d038a552ecae59c50204ea9a92a00000000000000"
        );
        sign_transaction_bound(&mut tx, &kp, chain_id, &genesis).unwrap();
        assert_eq!(
            hex::encode(&tx.public_key),
            "03ae62ade894b15c2b7aa2c61ac1103ee2de672f93668ab05a2760060d7f59b397"
        );
        assert_eq!(
            hex::encode(&tx.signature),
            "7c66bbaf11a82cf00f8edafd90e6c99627f4b4d25e1e285d7eeeb8f8dac01053354eb9b3b206246391bd61fa545af1d4b61631d5a7d2e025ea239b63a511a4a2"
        );
    }
}
