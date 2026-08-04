//! Ethereum-class signed transaction decoding for OVL.
//!
//! Supports:
//! - Legacy RLP txs: `[nonce, gasPrice, gas, to, value, data, v, r, s]`
//! - Compact unsigned bootstrap: `to (20) || value (32 BE) || data`
//!
//! Sender recovery uses secp256k1 + keccak256 (Ethereum address rules).

use alloy_primitives::{keccak256, B256, U256};
use alloy_rlp::{Decodable, Encodable, Header};
use k256::ecdsa::{RecoveryId, Signature as K256Signature, VerifyingKey};
use revm::primitives::{Address, Bytes};

use crate::types::EvmTx;
use crate::RollupError;

/// Decoded EVM transaction ready for `TxEnv` construction.
#[derive(Debug, Clone)]
pub struct DecodedEvmTx {
    pub caller: Address,
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub nonce: Option<u64>,
    pub gas_limit: Option<u64>,
    /// True when recovered from an RLP-signed Ethereum transaction.
    pub signed: bool,
}

/// Decode either a signed legacy Ethereum tx or the compact unsigned encoding.
pub fn decode_evm_tx(raw: &EvmTx) -> Result<DecodedEvmTx, RollupError> {
    if looks_like_rlp_list(&raw.0) {
        return decode_legacy_signed(&raw.0);
    }
    decode_compact(&raw.0)
}

fn looks_like_rlp_list(raw: &[u8]) -> bool {
    matches!(raw.first(), Some(b) if *b >= 0xc0)
}

fn decode_compact(raw: &[u8]) -> Result<DecodedEvmTx, RollupError> {
    if raw.len() < 52 {
        return Err(RollupError::Execution(
            "evm tx too short; expected RLP signed tx or to||value[||data]".into(),
        ));
    }
    let mut to = [0u8; 20];
    to.copy_from_slice(&raw[..20]);
    let value = U256::from_be_slice(&raw[20..52]);
    let data = Bytes::copy_from_slice(&raw[52..]);
    Ok(DecodedEvmTx {
        // Compact path keeps the historical default caller (funded at genesis seed).
        caller: Address::new([0xA1; 20]),
        to: Address::new(to),
        value,
        data,
        nonce: None,
        gas_limit: None,
        signed: false,
    })
}

fn decode_legacy_signed(raw: &[u8]) -> Result<DecodedEvmTx, RollupError> {
    let mut buf = raw;
    let header =
        Header::decode(&mut buf).map_err(|e| RollupError::Execution(format!("rlp header: {e}")))?;
    if !header.list {
        return Err(RollupError::Execution("legacy tx must be RLP list".into()));
    }
    if buf.len() < header.payload_length {
        return Err(RollupError::Execution("rlp payload truncated".into()));
    }
    let mut payload = &buf[..header.payload_length];

    let nonce: u64 = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp nonce: {e}")))?;
    let gas_price: U256 = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp gasPrice: {e}")))?;
    let gas_limit: u64 = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp gas: {e}")))?;
    let to_bytes: alloy_primitives::Bytes = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp to: {e}")))?;
    let value: U256 = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp value: {e}")))?;
    let data_bytes: alloy_primitives::Bytes = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp data: {e}")))?;
    let v: u64 = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp v: {e}")))?;
    let r: U256 = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp r: {e}")))?;
    let s: U256 = Decodable::decode(&mut payload)
        .map_err(|e| RollupError::Execution(format!("rlp s: {e}")))?;

    let to = if to_bytes.is_empty() {
        Address::ZERO
    } else if to_bytes.len() == 20 {
        let mut a = [0u8; 20];
        a.copy_from_slice(to_bytes.as_ref());
        Address::new(a)
    } else {
        return Err(RollupError::Execution("invalid to address length".into()));
    };

    let (chain_id, recovery_id) = parse_v(v)?;
    let sighash = legacy_signing_hash(
        nonce,
        gas_price,
        gas_limit,
        &to_bytes,
        value,
        &data_bytes,
        chain_id,
    )?;
    let caller = recover_caller(sighash, recovery_id, r, s)?;

    Ok(DecodedEvmTx {
        caller,
        to,
        value,
        data: Bytes::copy_from_slice(data_bytes.as_ref()),
        nonce: Some(nonce),
        gas_limit: Some(gas_limit.max(21_000)),
        signed: true,
    })
}

fn parse_v(v: u64) -> Result<(Option<u64>, u8), RollupError> {
    // Homestead: v ∈ {27,28}; EIP-155: v = chainId*2 + 35 + {0,1}
    if v == 27 || v == 28 {
        return Ok((None, (v - 27) as u8));
    }
    if v >= 35 {
        let rec = ((v - 35) % 2) as u8;
        let chain_id = (v - 35 - rec as u64) / 2;
        return Ok((Some(chain_id), rec));
    }
    Err(RollupError::Execution(format!("unsupported tx v={v}")))
}

fn legacy_signing_hash(
    nonce: u64,
    gas_price: U256,
    gas_limit: u64,
    to: &alloy_primitives::Bytes,
    value: U256,
    data: &alloy_primitives::Bytes,
    chain_id: Option<u64>,
) -> Result<B256, RollupError> {
    let mut payload = Vec::new();
    nonce.encode(&mut payload);
    gas_price.encode(&mut payload);
    gas_limit.encode(&mut payload);
    to.encode(&mut payload);
    value.encode(&mut payload);
    data.encode(&mut payload);
    if let Some(cid) = chain_id {
        cid.encode(&mut payload);
        0u8.encode(&mut payload);
        0u8.encode(&mut payload);
    }
    let mut buf = Vec::new();
    Header {
        list: true,
        payload_length: payload.len(),
    }
    .encode(&mut buf);
    buf.extend_from_slice(&payload);
    Ok(keccak256(&buf))
}

fn recover_caller(
    sighash: B256,
    recovery_id: u8,
    r: U256,
    s: U256,
) -> Result<Address, RollupError> {
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&r.to_be_bytes::<32>());
    sig_bytes[32..].copy_from_slice(&s.to_be_bytes::<32>());
    let sig = K256Signature::from_slice(&sig_bytes)
        .map_err(|e| RollupError::Execution(format!("bad signature: {e}")))?;
    let recid = RecoveryId::from_byte(recovery_id)
        .ok_or_else(|| RollupError::Execution("bad recovery id".into()))?;
    let vk = VerifyingKey::recover_from_prehash(sighash.as_slice(), &sig, recid)
        .map_err(|e| RollupError::Execution(format!("ecrecover failed: {e}")))?;
    let uncompressed = vk.to_encoded_point(false);
    let bytes = uncompressed.as_bytes();
    // Ethereum address = keccak256(pubkey[1..])[12..]
    if bytes.len() != 65 || bytes[0] != 0x04 {
        return Err(RollupError::Execution("unexpected pubkey encoding".into()));
    }
    let hash = keccak256(&bytes[1..]);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&hash.as_slice()[12..]);
    Ok(Address::new(addr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey;
    use k256::elliptic_curve::rand_core::OsRng;

    #[test]
    fn decode_compact_unsigned() {
        let mut raw = vec![0u8; 52];
        raw[..20].fill(0xB2);
        raw[51] = 42;
        let d = decode_evm_tx(&EvmTx(raw)).unwrap();
        assert!(!d.signed);
        assert_eq!(d.value, U256::from(42));
        assert_eq!(d.to, Address::new([0xB2; 20]));
    }

    #[test]
    fn sign_and_recover_legacy_eip155() {
        let sk = SigningKey::random(&mut OsRng);
        let nonce = 1u64;
        let gas_price = U256::from(1u64);
        let gas_limit = 21_000u64;
        let to = alloy_primitives::Bytes::from(vec![0xBBu8; 20]);
        let value = U256::from(7u64);
        let data = alloy_primitives::Bytes::new();
        let chain_id = 888802u64;
        let sighash = legacy_signing_hash(
            nonce,
            gas_price,
            gas_limit,
            &to,
            value,
            &data,
            Some(chain_id),
        )
        .unwrap();
        let (sig, recid) = sk.sign_prehash_recoverable(sighash.as_slice()).unwrap();
        let sig_bytes = sig.to_bytes();
        let r = U256::from_be_slice(&sig_bytes[..32]);
        let s = U256::from_be_slice(&sig_bytes[32..]);
        let v = chain_id * 2 + 35 + u64::from(recid.to_byte());

        let mut payload = Vec::new();
        nonce.encode(&mut payload);
        gas_price.encode(&mut payload);
        gas_limit.encode(&mut payload);
        to.encode(&mut payload);
        value.encode(&mut payload);
        data.encode(&mut payload);
        v.encode(&mut payload);
        r.encode(&mut payload);
        s.encode(&mut payload);
        let mut raw = Vec::new();
        Header {
            list: true,
            payload_length: payload.len(),
        }
        .encode(&mut raw);
        raw.extend_from_slice(&payload);

        let d = decode_evm_tx(&EvmTx(raw)).unwrap();
        assert!(d.signed);
        assert_eq!(d.value, value);
        assert_eq!(d.nonce, Some(nonce));
        let expected = {
            let vk = sk.verifying_key();
            let pt = vk.to_encoded_point(false);
            let hash = keccak256(&pt.as_bytes()[1..]);
            Address::new(hash.as_slice()[12..].try_into().unwrap())
        };
        assert_eq!(d.caller, expected);
    }
}
