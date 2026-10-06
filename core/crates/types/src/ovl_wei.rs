//! Canonical OVL wei (18 decimals).
//!
//! Experimental Trident state stored OVL as `u64` base units with 8 decimals.
//! Schema 22 stores the same whole-token amounts as wei by multiplying by
//! `10^10`. The 8-decimal integer is not a second balance: it is only the
//! exact quotient `wei / 10^10` when a legacy API still speaks that scale.
//!
//! Borsh encoding is 32-byte big-endian. JSON is a decimal string so clients
//! do not pass through JavaScript `number`.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Live EVM and schema-22 OVL scale.
pub const OVL_WEI_DECIMALS: u32 = 18;
/// Historical experimental account scale. Not a second live balance.
pub const OVL_LEGACY_DECIMALS: u32 = 8;
/// `10^(18-8)`.
pub const OVL_LEGACY_TO_WEI: u64 = 10_000_000_000;

/// Pinned execution profile name. Not "latest" and not Ethereum mainnet.
pub const OVL_EVM_PROFILE: &str = "OVL-EVM-v1";
/// revm `SpecId::SHANGHAI` for `revm` 42.0.1. Do not float this.
pub const OVL_EVM_SPEC_ID: &str = "shanghai";
pub const OVL_EVM_REVM_VERSION: &str = "42.0.1";

/// Dev chain id. Distinct from Ethereum (1) and BSC (56).
pub const OVL_EVM_DEV_CHAIN_ID: u64 = 74_000;
/// Testnet chain id. Production ids require a ceremony freeze and stay unset.
pub const OVL_EVM_TESTNET_CHAIN_ID: u64 = 74_001;

/// 100% of the EIP-1559 base fee is burned. Validators cannot change this.
pub const OVL_BASE_FEE_BURN_BPS: u16 = 10_000;

/// 32-byte big-endian OVL wei amount.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct OvlWei(pub [u8; 32]);

impl OvlWei {
    pub const ZERO: Self = Self([0u8; 32]);
    pub const MAX: Self = Self([0xff; 32]);

    pub const fn from_be_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn to_be_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn from_u64(value: u64) -> Self {
        let mut out = [0u8; 32];
        out[24..].copy_from_slice(&value.to_be_bytes());
        Self(out)
    }

    pub fn from_u128(value: u128) -> Self {
        let mut out = [0u8; 32];
        out[16..].copy_from_slice(&value.to_be_bytes());
        Self(out)
    }

    /// Scale a historical 8-decimal base amount into wei.
    pub fn from_legacy_base(legacy: u64) -> Option<Self> {
        Self::from_u64(legacy).checked_mul_u64(OVL_LEGACY_TO_WEI)
    }

    pub fn is_zero(self) -> bool {
        self.0 == [0u8; 32]
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let mut out = [0u8; 32];
        let mut carry = 0u16;
        for i in (0..32).rev() {
            let sum = u16::from(self.0[i]) + u16::from(rhs.0[i]) + carry;
            out[i] = sum as u8;
            carry = sum >> 8;
        }
        if carry == 0 {
            Some(Self(out))
        } else {
            None
        }
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        if self < rhs {
            return None;
        }
        let mut out = [0u8; 32];
        let mut borrow = 0i16;
        for i in (0..32).rev() {
            let mut diff = i16::from(self.0[i]) - i16::from(rhs.0[i]) - borrow;
            if diff < 0 {
                diff += 256;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out[i] = diff as u8;
        }
        Some(Self(out))
    }

    pub fn checked_mul_u64(self, rhs: u64) -> Option<Self> {
        if rhs == 0 || self.is_zero() {
            return Some(Self::ZERO);
        }
        let mut out = [0u8; 32];
        let mut carry = 0u128;
        for i in (0..32).rev() {
            let prod = u128::from(self.0[i]) * u128::from(rhs) + carry;
            out[i] = prod as u8;
            carry = prod >> 8;
        }
        if carry == 0 {
            Some(Self(out))
        } else {
            None
        }
    }

    pub fn div_rem_u64(self, rhs: u64) -> Option<(Self, u64)> {
        if rhs == 0 {
            return None;
        }
        let mut quot = [0u8; 32];
        let mut rem = 0u128;
        for (i, slot) in quot.iter_mut().enumerate() {
            let cur = (rem << 8) | u128::from(self.0[i]);
            *slot = (cur / u128::from(rhs)) as u8;
            rem = cur % u128::from(rhs);
        }
        Some((Self(quot), rem as u64))
    }

    /// Exact `wei / 10^10` when the amount has no sub-legacy dust.
    pub fn exact_legacy_quotient(self) -> Option<u64> {
        let (quot, rem) = self.div_rem_u64(OVL_LEGACY_TO_WEI)?;
        if rem != 0 {
            return None;
        }
        let mut bytes = [0u8; 8];
        if quot.0[..24].iter().any(|b| *b != 0) {
            return None;
        }
        bytes.copy_from_slice(&quot.0[24..]);
        Some(u64::from_be_bytes(bytes))
    }

    pub fn to_u64(self) -> Option<u64> {
        if self.0[..24].iter().any(|b| *b != 0) {
            return None;
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.0[24..]);
        Some(u64::from_be_bytes(bytes))
    }

    pub fn to_decimal(self) -> String {
        if self.is_zero() {
            return "0".to_string();
        }
        let mut digits = Vec::new();
        let mut n = self;
        while !n.is_zero() {
            let (quot, rem) = n.div_rem_u64(10).expect("10 != 0");
            digits.push(b'0' + rem as u8);
            n = quot;
        }
        digits.reverse();
        String::from_utf8(digits).expect("decimal digits")
    }

    pub fn from_decimal(input: &str) -> Option<Self> {
        if input.is_empty() || input.starts_with('+') {
            return None;
        }
        let input = input.strip_prefix("0x").unwrap_or(input);
        if input.starts_with("0x") {
            return None;
        }
        if input.chars().all(|c| c.is_ascii_hexdigit())
            && input.chars().any(|c| c.is_ascii_alphabetic())
        {
            return None;
        }
        if !input.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let mut acc = Self::ZERO;
        for c in input.bytes() {
            acc = acc.checked_mul_u64(10)?;
            acc = acc.checked_add(Self::from_u64(u64::from(c - b'0')))?;
        }
        Some(acc)
    }
}

impl std::fmt::Debug for OvlWei {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OvlWei({})", self.to_decimal())
    }
}

impl std::fmt::Display for OvlWei {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_decimal())
    }
}

impl BorshSerialize for OvlWei {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        writer.write_all(&self.0)
    }
}

impl BorshDeserialize for OvlWei {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        Ok(Self(buf))
    }
}

impl Serialize for OvlWei {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_decimal())
    }
}

impl<'de> Deserialize<'de> for OvlWei {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::from_decimal(&s).ok_or_else(|| serde::de::Error::custom("invalid OVL wei decimal"))
    }
}

impl TS for OvlWei {
    type WithoutGenerics = Self;
    fn name() -> String {
        "OvlWei".to_string()
    }
    fn decl() -> String {
        "type OvlWei = string;".to_string()
    }
    fn decl_concrete() -> String {
        Self::decl()
    }
    fn inline() -> String {
        "OvlWei".to_string()
    }
    fn inline_flattened() -> String {
        Self::inline()
    }
    fn output_path() -> Option<&'static std::path::Path> {
        Some(std::path::Path::new("OvlWei.ts"))
    }
}

/// Genesis fee-market parameters. The formula is consensus code, not a
/// validator setting. `base_fee_burn_bps` must be 10_000.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OvlFeeMarketParams {
    pub version: u32,
    pub initial_base_fee_wei: OvlWei,
    pub gas_limit: u64,
    pub elasticity_multiplier: u64,
    pub base_fee_change_denominator: u64,
    pub base_fee_burn_bps: u16,
}

impl OvlFeeMarketParams {
    pub const VERSION: u32 = 1;

    pub fn dev_default() -> Self {
        Self {
            version: Self::VERSION,
            // 1 gwei. Fits in revm 42's u64 block base fee.
            initial_base_fee_wei: OvlWei::from_u64(1_000_000_000),
            gas_limit: 30_000_000,
            elasticity_multiplier: 2,
            base_fee_change_denominator: 8,
            base_fee_burn_bps: OVL_BASE_FEE_BURN_BPS,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != Self::VERSION {
            return Err("unsupported OVL fee market version");
        }
        if self.gas_limit == 0 {
            return Err("gas limit must be non-zero");
        }
        if self.elasticity_multiplier == 0 {
            return Err("elasticity multiplier must be non-zero");
        }
        if !self.gas_limit.is_multiple_of(self.elasticity_multiplier) {
            return Err("gas limit must divide evenly by elasticity");
        }
        if self.base_fee_change_denominator == 0 {
            return Err("base fee change denominator must be non-zero");
        }
        if self.base_fee_burn_bps != OVL_BASE_FEE_BURN_BPS {
            return Err("OVL base fee burn is fixed at 100%");
        }
        if self.initial_base_fee_wei.to_u64().is_none() {
            return Err("initial base fee must fit the pinned engine's u64 base fee");
        }
        Ok(())
    }

    pub fn gas_target(&self) -> u64 {
        self.gas_limit / self.elasticity_multiplier
    }
}

impl BorshSerialize for OvlFeeMarketParams {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> Result<(), borsh::io::Error> {
        BorshSerialize::serialize(&self.version, writer)?;
        BorshSerialize::serialize(&self.initial_base_fee_wei, writer)?;
        BorshSerialize::serialize(&self.gas_limit, writer)?;
        BorshSerialize::serialize(&self.elasticity_multiplier, writer)?;
        BorshSerialize::serialize(&self.base_fee_change_denominator, writer)?;
        BorshSerialize::serialize(&self.base_fee_burn_bps, writer)
    }
}

impl BorshDeserialize for OvlFeeMarketParams {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            version: u32::deserialize_reader(reader)?,
            initial_base_fee_wei: OvlWei::deserialize_reader(reader)?,
            gas_limit: u64::deserialize_reader(reader)?,
            elasticity_multiplier: u64::deserialize_reader(reader)?,
            base_fee_change_denominator: u64::deserialize_reader(reader)?,
            base_fee_burn_bps: u16::deserialize_reader(reader)?,
        })
    }
}

/// Chain ids that must never be selected for an Agora OVL execution lane.
pub fn ovl_evm_chain_id_rejected(chain_id: u64) -> bool {
    chain_id == 0 || chain_id == 1 || chain_id == 56
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_whole_token_scales_by_ten_to_the_tenth() {
        let one_legacy = 100_000_000u64;
        let wei = OvlWei::from_legacy_base(one_legacy).unwrap();
        assert_eq!(wei.to_decimal(), "1000000000000000000");
        assert_eq!(wei.exact_legacy_quotient(), Some(one_legacy));
        let dust = wei.checked_add(OvlWei::from_u64(1)).unwrap();
        assert!(dust.exact_legacy_quotient().is_none());
    }

    #[test]
    fn historical_ovl_cap_fits_in_wei_and_has_no_type_level_maximum() {
        let historical_max = 2_100_000_000_000_000_000u64;
        let wei = OvlWei::from_legacy_base(historical_max).unwrap();
        assert!(wei.checked_add(OvlWei::from_u64(1)).is_some());
        assert!(OvlWei::from_legacy_base(u64::MAX).is_some());
    }

    #[test]
    fn fee_market_rejects_partial_burn_and_foreign_chain_ids() {
        let mut params = OvlFeeMarketParams::dev_default();
        params.validate().unwrap();
        params.base_fee_burn_bps = 9_999;
        assert!(params.validate().is_err());
        assert!(ovl_evm_chain_id_rejected(0));
        assert!(ovl_evm_chain_id_rejected(1));
        assert!(ovl_evm_chain_id_rejected(56));
        assert!(!ovl_evm_chain_id_rejected(OVL_EVM_DEV_CHAIN_ID));
        assert!(!ovl_evm_chain_id_rejected(OVL_EVM_TESTNET_CHAIN_ID));
    }
}
