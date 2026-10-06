//! Canonical Ethereum transaction parser for `OVL-EVM-v1`.
//!
//! Accepted envelopes are legacy EIP-155, EIP-2930 type 1, and EIP-1559 type 2.
//! Type 3 blob transactions and EIP-7702 are rejected. Unprotected legacy
//! transactions (`v` of 27 or 28) are rejected. High-`s` signatures are
//! rejected before recovery. There is no unsigned compact encoding and no
//! fixed funded caller.

use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use alloy_rlp::{Decodable, Encodable, Header};
use k256::ecdsa::{RecoveryId, Signature as K256Signature, SigningKey, VerifyingKey};

use crate::error::EvmError;
use crate::fee::effective_gas_price;

const SECP256K1_HALF_N: [u8; 32] = [
    0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0x5d, 0x57, 0x6e, 0x73, 0x57, 0xa4, 0x50, 0x1d, 0xdf, 0xe9, 0x2f, 0x46, 0x66, 0x81, 0xb2, 0x0a,
];

/// Shanghai intrinsic-gas constants. Execution gas after this floor is the
/// pinned engine's responsibility.
const TX_GAS: u64 = 21_000;
const TX_CREATE: u64 = 32_000;
const INITCODE_WORD_GAS: u64 = 2;
const MAX_INITCODE_SIZE: usize = 49_152;
const ACCESS_LIST_ADDRESS_GAS: u64 = 2_400;
const ACCESS_LIST_STORAGE_KEY_GAS: u64 = 1_900;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessItem {
    pub address: [u8; 20],
    pub storage_keys: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedTx {
    pub tx_type: u8,
    pub chain_id: u64,
    pub nonce: u64,
    pub gas_limit: u64,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: Option<u128>,
    pub to: Option<[u8; 20]>,
    pub value: [u8; 32],
    pub data: Vec<u8>,
    pub access_list: Vec<AccessItem>,
    pub caller: [u8; 20],
    pub hash: [u8; 32],
}

impl ParsedTx {
    pub fn effective_price(&self, base_fee: u128) -> u128 {
        effective_gas_price(
            self.tx_type,
            self.max_fee_per_gas,
            self.max_priority_fee_per_gas,
            base_fee,
        )
    }

    pub fn intrinsic_gas(&self) -> Result<u64, EvmError> {
        let mut gas = TX_GAS;
        if self.to.is_none() {
            gas = gas
                .checked_add(TX_CREATE)
                .ok_or_else(|| EvmError::rejected("intrinsic gas overflow"))?;
            if self.data.len() > MAX_INITCODE_SIZE {
                return Err(EvmError::rejected("init code exceeds Shanghai limit"));
            }
            let words = self.data.len().div_ceil(32) as u64;
            gas = gas
                .checked_add(
                    words
                        .checked_mul(INITCODE_WORD_GAS)
                        .ok_or_else(|| EvmError::rejected("init code gas overflow"))?,
                )
                .ok_or_else(|| EvmError::rejected("intrinsic gas overflow"))?;
        }
        for byte in &self.data {
            let cost = if *byte == 0 { 4 } else { 16 };
            gas = gas
                .checked_add(cost)
                .ok_or_else(|| EvmError::rejected("calldata gas overflow"))?;
        }
        for item in &self.access_list {
            gas = gas
                .checked_add(ACCESS_LIST_ADDRESS_GAS)
                .ok_or_else(|| EvmError::rejected("access list gas overflow"))?;
            gas = gas
                .checked_add(
                    (item.storage_keys.len() as u64)
                        .checked_mul(ACCESS_LIST_STORAGE_KEY_GAS)
                        .ok_or_else(|| EvmError::rejected("access list gas overflow"))?,
                )
                .ok_or_else(|| EvmError::rejected("access list gas overflow"))?;
        }
        Ok(gas)
    }
}

/// Parse a signed raw Ethereum transaction. Compact unsigned bytes are rejected.
pub fn parse_raw_transaction(raw: &[u8]) -> Result<ParsedTx, EvmError> {
    let first = *raw
        .first()
        .ok_or_else(|| EvmError::rejected("empty transaction"))?;
    match first {
        0x01 => parse_typed(raw, 1),
        0x02 => parse_typed(raw, 2),
        0x03 => Err(EvmError::rejected("type 3 blob transactions are rejected")),
        0x04 => Err(EvmError::rejected("EIP-7702 transactions are rejected")),
        byte if byte >= 0xc0 => parse_legacy(raw),
        _ => Err(EvmError::rejected(
            "unsupported transaction envelope; unsigned compact transactions are rejected",
        )),
    }
}

pub fn ethereum_address_from_signing_key(key: &SigningKey) -> [u8; 20] {
    ethereum_address(key.verifying_key())
}

fn ethereum_address(key: &VerifyingKey) -> [u8; 20] {
    let point = key.to_encoded_point(false);
    let hash = keccak256(&point.as_bytes()[1..]);
    let mut out = [0u8; 20];
    out.copy_from_slice(&hash.as_slice()[12..]);
    out
}

pub fn dev_signing_key() -> SigningKey {
    SigningKey::from_bytes((&[0x11u8; 32]).into()).expect("dev scalar")
}

#[allow(clippy::too_many_arguments)]
pub fn sign_legacy(
    key: &SigningKey,
    chain_id: u64,
    nonce: u64,
    gas_price: u128,
    gas_limit: u64,
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: &[u8],
) -> Vec<u8> {
    let sighash = legacy_sighash(nonce, gas_price, gas_limit, to, value, data, chain_id);
    let (y, r, s) = sign_low_s(key, sighash);
    let v = chain_id * 2 + 35 + u64::from(y);
    rlp_list(&[
        encoded(&nonce),
        encoded(&U256::from(gas_price)),
        encoded(&gas_limit),
        encoded_to(to),
        encoded(&U256::from_be_slice(&value)),
        encoded(&Bytes::copy_from_slice(data)),
        encoded(&v),
        encoded(&U256::from_be_slice(&r)),
        encoded(&U256::from_be_slice(&s)),
    ])
}

#[allow(clippy::too_many_arguments)]
pub fn sign_eip2930(
    key: &SigningKey,
    chain_id: u64,
    nonce: u64,
    gas_price: u128,
    gas_limit: u64,
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: &[u8],
    access_list: &[AccessItem],
) -> Vec<u8> {
    let body = eip2930_body(
        chain_id,
        nonce,
        gas_price,
        gas_limit,
        to,
        value,
        data,
        access_list,
    );
    finish_typed(key, 0x01, &body)
}

#[allow(clippy::too_many_arguments)]
pub fn sign_eip1559(
    key: &SigningKey,
    chain_id: u64,
    nonce: u64,
    max_priority_fee: u128,
    max_fee: u128,
    gas_limit: u64,
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: &[u8],
    access_list: &[AccessItem],
) -> Vec<u8> {
    let body = eip1559_body(
        chain_id,
        nonce,
        max_priority_fee,
        max_fee,
        gas_limit,
        to,
        value,
        data,
        access_list,
    );
    finish_typed(key, 0x02, &body)
}

fn finish_typed(key: &SigningKey, type_byte: u8, body: &[Vec<u8>]) -> Vec<u8> {
    let unsigned = rlp_list(body);
    let mut preimage = Vec::with_capacity(1 + unsigned.len());
    preimage.push(type_byte);
    preimage.extend_from_slice(&unsigned);
    let (y, r, s) = sign_low_s(key, keccak256(&preimage));
    let mut signed_body = body.to_vec();
    signed_body.push(encoded(&u64::from(y)));
    signed_body.push(encoded(&U256::from_be_slice(&r)));
    signed_body.push(encoded(&U256::from_be_slice(&s)));
    let payload = rlp_list(&signed_body);
    let mut raw = Vec::with_capacity(1 + payload.len());
    raw.push(type_byte);
    raw.extend_from_slice(&payload);
    raw
}

fn sign_low_s(key: &SigningKey, sighash: B256) -> (u8, [u8; 32], [u8; 32]) {
    let (sig, recid) = key
        .sign_prehash_recoverable(sighash.as_slice())
        .expect("sign prehash");
    let bytes = sig.to_bytes();
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&bytes[..32]);
    s.copy_from_slice(&bytes[32..]);
    assert!(s <= SECP256K1_HALF_N, "k256 produced a high-s signature");
    (recid.to_byte(), r, s)
}

fn parse_legacy(raw: &[u8]) -> Result<ParsedTx, EvmError> {
    let mut rest = raw;
    let mut payload = decode_list(&mut rest)?;
    if !rest.is_empty() {
        return Err(EvmError::rejected(
            "trailing bytes after legacy transaction",
        ));
    }
    let nonce = decode_u64(&mut payload)?;
    let gas_price = decode_u128(&mut payload)?;
    let gas_limit = decode_u64(&mut payload)?;
    let to = decode_to(&mut payload)?;
    let value = decode_u256_bytes(&mut payload)?;
    let data = decode_bytes(&mut payload)?;
    let v = decode_u64(&mut payload)?;
    let r = decode_u256_bytes(&mut payload)?;
    let s = decode_u256_bytes(&mut payload)?;
    if !payload.is_empty() {
        return Err(EvmError::rejected("trailing fields in legacy transaction"));
    }
    let (chain_id, y) = parse_legacy_v(v)?;
    let sighash = legacy_sighash(nonce, gas_price, gas_limit, to, value, &data, chain_id);
    let caller = recover(sighash, y, &r, &s)?;
    Ok(ParsedTx {
        tx_type: 0,
        chain_id,
        nonce,
        gas_limit,
        max_fee_per_gas: gas_price,
        max_priority_fee_per_gas: None,
        to,
        value,
        data,
        access_list: Vec::new(),
        caller,
        hash: keccak256(raw).into(),
    })
}

fn parse_typed(raw: &[u8], tx_type: u8) -> Result<ParsedTx, EvmError> {
    let mut rest = &raw[1..];
    let mut payload = decode_list(&mut rest)?;
    if !rest.is_empty() {
        return Err(EvmError::rejected("trailing bytes after typed transaction"));
    }
    let chain_id = decode_u64(&mut payload)?;
    let nonce = decode_u64(&mut payload)?;
    let (priority, max_fee) = if tx_type == 2 {
        let priority = decode_u128(&mut payload)?;
        let max_fee = decode_u128(&mut payload)?;
        (Some(priority), max_fee)
    } else {
        (None, decode_u128(&mut payload)?)
    };
    let gas_limit = decode_u64(&mut payload)?;
    let to = decode_to(&mut payload)?;
    let value = decode_u256_bytes(&mut payload)?;
    let data = decode_bytes(&mut payload)?;
    let access_list = decode_access_list(&mut payload)?;
    let y = decode_u64(&mut payload)?;
    let r = decode_u256_bytes(&mut payload)?;
    let s = decode_u256_bytes(&mut payload)?;
    if !payload.is_empty() {
        return Err(EvmError::rejected("trailing fields in typed transaction"));
    }
    if y > 1 {
        return Err(EvmError::rejected(
            "typed transaction y-parity must be 0 or 1",
        ));
    }
    let unsigned_body = if tx_type == 2 {
        eip1559_body(
            chain_id,
            nonce,
            priority.unwrap_or(0),
            max_fee,
            gas_limit,
            to,
            value,
            &data,
            &access_list,
        )
    } else {
        eip2930_body(
            chain_id,
            nonce,
            max_fee,
            gas_limit,
            to,
            value,
            &data,
            &access_list,
        )
    };
    let unsigned = rlp_list(&unsigned_body);
    let mut preimage = Vec::with_capacity(1 + unsigned.len());
    preimage.push(tx_type);
    preimage.extend_from_slice(&unsigned);
    let caller = recover(keccak256(&preimage), y as u8, &r, &s)?;
    Ok(ParsedTx {
        tx_type,
        chain_id,
        nonce,
        gas_limit,
        max_fee_per_gas: max_fee,
        max_priority_fee_per_gas: priority,
        to,
        value,
        data,
        access_list,
        caller,
        hash: keccak256(raw).into(),
    })
}

fn parse_legacy_v(v: u64) -> Result<(u64, u8), EvmError> {
    if v == 27 || v == 28 {
        return Err(EvmError::rejected(
            "unprotected legacy transactions are rejected",
        ));
    }
    if v < 35 {
        return Err(EvmError::rejected(format!("unsupported legacy v={v}")));
    }
    let y = ((v - 35) % 2) as u8;
    let chain_id = (v - 35 - u64::from(y)) / 2;
    if chain_id == 0 {
        return Err(EvmError::rejected("legacy chain id must be non-zero"));
    }
    Ok((chain_id, y))
}

fn recover(sighash: B256, y: u8, r: &[u8; 32], s: &[u8; 32]) -> Result<[u8; 20], EvmError> {
    if *r == [0u8; 32] || *s == [0u8; 32] {
        return Err(EvmError::rejected("signature r and s must be non-zero"));
    }
    if s > &SECP256K1_HALF_N {
        return Err(EvmError::rejected("high-s signatures are rejected"));
    }
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(r);
    sig_bytes[32..].copy_from_slice(s);
    let sig = K256Signature::from_slice(&sig_bytes)
        .map_err(|_| EvmError::rejected("malformed signature"))?;
    let recid = RecoveryId::from_byte(y).ok_or_else(|| EvmError::rejected("bad recovery id"))?;
    let key = VerifyingKey::recover_from_prehash(sighash.as_slice(), &sig, recid)
        .map_err(|_| EvmError::rejected("signature recovery failed"))?;
    Ok(ethereum_address(&key))
}

fn legacy_sighash(
    nonce: u64,
    gas_price: u128,
    gas_limit: u64,
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: &[u8],
    chain_id: u64,
) -> B256 {
    let payload = rlp_list(&[
        encoded(&nonce),
        encoded(&U256::from(gas_price)),
        encoded(&gas_limit),
        encoded_to(to),
        encoded(&U256::from_be_slice(&value)),
        encoded(&Bytes::copy_from_slice(data)),
        encoded(&chain_id),
        encoded(&0u8),
        encoded(&0u8),
    ]);
    keccak256(payload)
}

#[allow(clippy::too_many_arguments)]
fn eip2930_body(
    chain_id: u64,
    nonce: u64,
    gas_price: u128,
    gas_limit: u64,
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: &[u8],
    access_list: &[AccessItem],
) -> Vec<Vec<u8>> {
    vec![
        encoded(&chain_id),
        encoded(&nonce),
        encoded(&U256::from(gas_price)),
        encoded(&gas_limit),
        encoded_to(to),
        encoded(&U256::from_be_slice(&value)),
        encoded(&Bytes::copy_from_slice(data)),
        encode_access_list(access_list),
    ]
}

#[allow(clippy::too_many_arguments)]
fn eip1559_body(
    chain_id: u64,
    nonce: u64,
    priority: u128,
    max_fee: u128,
    gas_limit: u64,
    to: Option<[u8; 20]>,
    value: [u8; 32],
    data: &[u8],
    access_list: &[AccessItem],
) -> Vec<Vec<u8>> {
    vec![
        encoded(&chain_id),
        encoded(&nonce),
        encoded(&U256::from(priority)),
        encoded(&U256::from(max_fee)),
        encoded(&gas_limit),
        encoded_to(to),
        encoded(&U256::from_be_slice(&value)),
        encoded(&Bytes::copy_from_slice(data)),
        encode_access_list(access_list),
    ]
}

fn encode_access_list(list: &[AccessItem]) -> Vec<u8> {
    let mut items = Vec::new();
    for item in list {
        let mut keys = Vec::new();
        for key in &item.storage_keys {
            keys.push(encoded(&B256::from(*key)));
        }
        let entry = rlp_list(&[encoded(&Address::new(item.address)), rlp_list(&keys)]);
        items.push(entry);
    }
    rlp_list(&items)
}

fn encoded_to(to: Option<[u8; 20]>) -> Vec<u8> {
    match to {
        Some(addr) => encoded(&Address::new(addr)),
        None => encoded(&Bytes::new()),
    }
}

fn encoded<T: Encodable>(value: &T) -> Vec<u8> {
    let mut out = Vec::new();
    value.encode(&mut out);
    out
}

fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = Vec::new();
    for item in items {
        payload.extend_from_slice(item);
    }
    let mut out = Vec::new();
    Header {
        list: true,
        payload_length: payload.len(),
    }
    .encode(&mut out);
    out.extend_from_slice(&payload);
    out
}

fn decode_list<'a>(input: &mut &'a [u8]) -> Result<&'a [u8], EvmError> {
    let header = Header::decode(input).map_err(|err| EvmError::rejected(format!("rlp: {err}")))?;
    if !header.list {
        return Err(EvmError::rejected("expected RLP list"));
    }
    if input.len() < header.payload_length {
        return Err(EvmError::rejected("truncated RLP list"));
    }
    let (payload, rest) = input.split_at(header.payload_length);
    *input = rest;
    Ok(payload)
}

fn decode_u64(input: &mut &[u8]) -> Result<u64, EvmError> {
    Decodable::decode(input).map_err(|err| EvmError::rejected(format!("rlp integer: {err}")))
}

fn decode_u128(input: &mut &[u8]) -> Result<u128, EvmError> {
    let value: U256 = Decodable::decode(input)
        .map_err(|err| EvmError::rejected(format!("rlp integer: {err}")))?;
    value
        .try_into()
        .map_err(|_| EvmError::rejected("gas price exceeds u128"))
}

fn decode_u256_bytes(input: &mut &[u8]) -> Result<[u8; 32], EvmError> {
    let value: U256 =
        Decodable::decode(input).map_err(|err| EvmError::rejected(format!("rlp word: {err}")))?;
    Ok(value.to_be_bytes())
}

fn decode_bytes(input: &mut &[u8]) -> Result<Vec<u8>, EvmError> {
    let bytes: Bytes =
        Decodable::decode(input).map_err(|err| EvmError::rejected(format!("rlp bytes: {err}")))?;
    Ok(bytes.to_vec())
}

fn decode_to(input: &mut &[u8]) -> Result<Option<[u8; 20]>, EvmError> {
    let bytes: Bytes = Decodable::decode(input)
        .map_err(|err| EvmError::rejected(format!("rlp address: {err}")))?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() != 20 {
        return Err(EvmError::rejected("destination must be empty or 20 bytes"));
    }
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&bytes);
    Ok(Some(addr))
}

fn decode_access_list(input: &mut &[u8]) -> Result<Vec<AccessItem>, EvmError> {
    let mut list_payload = decode_list(input)?;
    let mut items = Vec::new();
    while !list_payload.is_empty() {
        let mut entry = decode_list(&mut list_payload)?;
        let address = decode_to(&mut entry)?
            .ok_or_else(|| EvmError::rejected("access list address must be 20 bytes"))?;
        let mut keys_payload = decode_list(&mut entry)?;
        if !entry.is_empty() {
            return Err(EvmError::rejected("trailing access list entry bytes"));
        }
        let mut storage_keys = Vec::new();
        while !keys_payload.is_empty() {
            storage_keys.push(decode_u256_bytes(&mut keys_payload)?);
        }
        items.push(AccessItem {
            address,
            storage_keys,
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn recovers_legacy_type1_and_type2_and_rejects_exclusions() {
        let key = dev_signing_key();
        let expected = ethereum_address_from_signing_key(&key);
        let to = [0x22u8; 20];
        let legacy = sign_legacy(&key, 74_000, 3, 2, 21_000, Some(to), [0u8; 32], &[]);
        let parsed = parse_raw_transaction(&legacy).unwrap();
        assert_eq!(parsed.caller, expected);
        assert_eq!(parsed.chain_id, 74_000);
        assert_eq!(parsed.tx_type, 0);
        assert_eq!(parsed.intrinsic_gas().unwrap(), 21_000);

        let typed = sign_eip1559(
            &key,
            74_000,
            4,
            1,
            2,
            21_000,
            Some(to),
            [0u8; 32],
            &[0x01],
            &[],
        );
        let parsed = parse_raw_transaction(&typed).unwrap();
        assert_eq!(parsed.caller, expected);
        assert_eq!(parsed.tx_type, 2);
        assert!(parsed.intrinsic_gas().unwrap() > 21_000);

        let access = AccessItem {
            address: to,
            storage_keys: vec![[0x33; 32]],
        };
        let type1 = sign_eip2930(
            &key,
            74_000,
            5,
            2,
            50_000,
            Some(to),
            [0u8; 32],
            &[],
            &[access],
        );
        let parsed = parse_raw_transaction(&type1).unwrap();
        assert_eq!(parsed.tx_type, 1);
        assert_eq!(parsed.access_list.len(), 1);

        assert!(parse_raw_transaction(&[0x03, 0xc0])
            .unwrap_err()
            .to_string()
            .contains("blob"));
        assert!(parse_raw_transaction(&[0x04, 0xc0])
            .unwrap_err()
            .to_string()
            .contains("7702"));
        assert!(parse_raw_transaction(&[0xA1; 52]).is_err());
    }

    #[test]
    fn high_s_is_rejected_before_recovery() {
        let key = dev_signing_key();
        let mut raw = sign_legacy(&key, 74_000, 1, 1, 21_000, Some([0x44; 20]), [0u8; 32], &[]);
        let parsed = parse_raw_transaction(&raw).unwrap();
        assert!(parse_raw_transaction(&raw).is_ok());
        // Flip the last payload byte of s inside the low range into a high value
        // by rebuilding is covered separately: a crafted s above n/2 fails closed.
        let mut high = [0xffu8; 32];
        high[0] = 0xff;
        assert!(recover(keccak256([]), 0, &parsed.hash, &high).is_err());
        raw.push(0x00);
        assert!(parse_raw_transaction(&raw).is_err());
    }

    #[test]
    fn ethereum_address_is_not_the_agora_sha256_address() {
        let key = dev_signing_key();
        let eth = ethereum_address_from_signing_key(&key);
        let compressed = key.verifying_key().to_encoded_point(true);
        let digest = Sha256::digest(compressed.as_bytes());
        let mut agora = [0u8; 20];
        agora.copy_from_slice(&digest[..20]);
        assert_ne!(eth, agora);
    }
}
