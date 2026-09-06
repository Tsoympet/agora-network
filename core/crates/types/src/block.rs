use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AccountTransfer, DataCommitmentAuthorization, DrcPaymentTx, Hash, OvlExecutionTx,
    SignedStakeTx, Transaction,
};

/// Explicit version/domain for bodies carrying authenticated DA commitments.
pub const TRIDENT_BLOCK_BODY_VERSION: u16 = 5;
pub const TRIDENT_BLOCK_BODY_DOMAIN: &[u8] = b"agora-block-body-v5";

/// Block header for Agora's BlockDAG tips.
///
/// Multiple parents enable parallel block production; GHOSTDAG later imposes order.
#[derive(
    Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[ts(export)]
pub struct BlockHeader {
    pub version: u16,
    pub parents: Vec<Hash>,
    pub timestamp_ms: u64,
    pub bits: u32,
    pub nonce: u64,
    pub tx_root: Hash,
}

impl BlockHeader {
    pub fn hash(&self) -> Hash {
        Hash::hash_borsh(self)
    }
}

/// Full Trident body: TLT UTXO plus native account, stake, execution, payment, and data lanes.
#[derive(Clone, PartialEq, Eq, Debug, BorshSerialize, Serialize, Deserialize, TS)]
pub struct Block {
    pub header: BlockHeader,
    /// TLT UTXO lane (coinbase + transfers).
    pub transactions: Vec<Transaction>,
    /// OVL/DRC liquid account transfers (Trident).
    #[serde(default)]
    pub account_transfers: Vec<AccountTransfer>,
    /// OVL/DRC staking ops (Trident).
    #[serde(default)]
    pub stake_ops: Vec<SignedStakeTx>,
    /// Signed, gas-metered OVL execution envelopes.
    #[serde(default)]
    pub ovl_executions: Vec<OvlExecutionTx>,
    /// Signed native DRC payments with routing metadata.
    #[serde(default)]
    pub drc_payments: Vec<DrcPaymentTx>,
    /// Provenance-bound, operator-authorized data commitments.
    #[serde(default)]
    pub data_commitments: Vec<DataCommitmentAuthorization>,
}

impl Block {
    /// Construct a UTXO-only block (empty account/stake lanes).
    pub fn utxo(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        Self {
            header,
            transactions,
            account_transfers: Vec::new(),
            stake_ops: Vec::new(),
            ovl_executions: Vec::new(),
            drc_payments: Vec::new(),
            data_commitments: Vec::new(),
        }
    }

    pub fn id(&self) -> Hash {
        self.header.hash()
    }

    /// Compute a simple pairwise tx merkle root (duplicate last leaf when odd).
    ///
    /// Kept intentionally minimal for Phase 1 ID stability; consensus may harden this later.
    pub fn compute_tx_root(transactions: &[Transaction]) -> Hash {
        if transactions.is_empty() {
            return Hash::ZERO;
        }
        let mut level: Vec<Hash> = transactions.iter().map(Transaction::tx_id).collect();
        while level.len() > 1 {
            if level.len() % 2 == 1 {
                level.push(*level.last().expect("non-empty"));
            }
            level = level
                .chunks(2)
                .map(|pair| {
                    let mut buf = [0u8; 64];
                    buf[..32].copy_from_slice(pair[0].as_bytes());
                    buf[32..].copy_from_slice(pair[1].as_bytes());
                    Hash::hash_bytes(&buf)
                })
                .collect();
        }
        level[0]
    }

    /// Body commitment for `header.tx_root`.
    ///
    /// UTXO-only blocks keep the legacy merkle root; account/stake-only bodies
    /// retain v2; OVL execution uses v3; DRC payments use v4; authenticated
    /// data commitments use v5.
    pub fn compute_body_root(&self) -> Hash {
        if !self.data_commitments.is_empty() {
            let authorization_ids: Vec<Hash> = self
                .data_commitments
                .iter()
                .map(DataCommitmentAuthorization::authorization_id)
                .collect();
            return Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_DOMAIN,
                TRIDENT_BLOCK_BODY_VERSION,
                self.compute_body_root_v4(),
                authorization_ids,
            ));
        }
        self.compute_body_root_v4()
    }

    fn compute_body_root_v4(&self) -> Hash {
        if !self.drc_payments.is_empty() {
            let payment_ids: Vec<Hash> = self
                .drc_payments
                .iter()
                .map(DrcPaymentTx::payment_id)
                .collect();
            return Hash::hash_borsh(&(
                b"agora-block-body-v4",
                self.compute_body_root_v3(),
                payment_ids,
            ));
        }
        self.compute_body_root_v3()
    }

    fn compute_body_root_v3(&self) -> Hash {
        if !self.ovl_executions.is_empty() {
            let execution_ids: Vec<Hash> = self
                .ovl_executions
                .iter()
                .map(OvlExecutionTx::tx_id)
                .collect();
            return Hash::hash_borsh(&(
                b"agora-block-body-v3",
                self.compute_body_root_v2(),
                execution_ids,
            ));
        }
        self.compute_body_root_v2()
    }

    fn compute_body_root_v2(&self) -> Hash {
        if self.account_transfers.is_empty() && self.stake_ops.is_empty() {
            return Self::compute_tx_root(&self.transactions);
        }
        let account_ids: Vec<Hash> = self
            .account_transfers
            .iter()
            .map(AccountTransfer::transfer_id)
            .collect();
        let stake_ids: Vec<Hash> = self
            .stake_ops
            .iter()
            .map(SignedStakeTx::stake_tx_id)
            .collect();
        Hash::hash_borsh(&(
            b"agora-block-body-v2",
            Self::compute_tx_root(&self.transactions),
            account_ids,
            stake_ids,
        ))
    }
}

impl BorshDeserialize for Block {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> Result<Self, borsh::io::Error> {
        Ok(Self {
            header: BlockHeader::deserialize_reader(reader)?,
            transactions: Vec::<Transaction>::deserialize_reader(reader)?,
            account_transfers: deserialize_trailing_vec(reader)?,
            stake_ops: deserialize_trailing_vec(reader)?,
            ovl_executions: deserialize_trailing_vec(reader)?,
            drc_payments: deserialize_trailing_vec(reader)?,
            data_commitments: deserialize_trailing_vec(reader)?,
        })
    }
}

/// Appended body lanes are optional only at an exact legacy end-of-input
/// boundary. A partial vector length or element remains a hard decode failure.
fn deserialize_trailing_vec<T, R>(reader: &mut R) -> Result<Vec<T>, borsh::io::Error>
where
    T: BorshDeserialize,
    R: borsh::io::Read,
{
    let Some(len) = deserialize_optional_len(reader)? else {
        return Ok(Vec::new());
    };
    let mut values = Vec::with_capacity((len as usize).min(1024));
    for _ in 0..len {
        values.push(T::deserialize_reader(reader)?);
    }
    Ok(values)
}

fn deserialize_optional_len<R: borsh::io::Read>(
    reader: &mut R,
) -> Result<Option<u32>, borsh::io::Error> {
    let mut bytes = [0u8; 4];
    let mut filled = 0usize;
    while filled < bytes.len() {
        match reader.read(&mut bytes[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => {
                return Err(borsh::io::Error::new(
                    borsh::io::ErrorKind::UnexpectedEof,
                    "partial trailing block-lane length",
                ));
            }
            Ok(read) => filled += read,
            Err(err) if err.kind() == borsh::io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(Some(u32::from_le_bytes(bytes)))
}

#[cfg(test)]
mod tests {
    use crate::{Address, Amount, DataAvailabilityCommitment, DrcPaymentTx};

    use super::*;

    #[test]
    fn drc_payment_activates_body_root_v4() {
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        let legacy = block.compute_body_root();
        block.drc_payments.push(DrcPaymentTx::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(1),
            Amount::from_base_units(1),
            9,
            Hash([3; 32]),
            0,
        ));
        assert_ne!(block.compute_body_root(), legacy);
        assert_eq!(
            block.compute_body_root(),
            Hash::hash_borsh(&(
                b"agora-block-body-v4",
                legacy,
                vec![block.drc_payments[0].payment_id()]
            ))
        );
    }

    #[test]
    fn data_commitment_activates_body_root_v5() {
        let mut block = Block::utxo(
            BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            vec![],
        );
        let legacy = block.compute_body_root();
        block
            .data_commitments
            .push(DataCommitmentAuthorization::unsigned(
                Address([7; 20]),
                0,
                DataAvailabilityCommitment::agora_layers_ovolos_batch(
                    "agora-ovolos-testnet-1".into(),
                    Hash([1; 32]),
                    Hash([2; 32]),
                    3,
                    Hash([4; 32]),
                    Hash([5; 32]),
                    Hash([6; 32]),
                    7,
                    8,
                ),
            ));
        let first = block.compute_body_root();
        assert_ne!(first, legacy);
        assert_eq!(
            first,
            Hash::hash_borsh(&(
                TRIDENT_BLOCK_BODY_DOMAIN,
                TRIDENT_BLOCK_BODY_VERSION,
                legacy,
                vec![block.data_commitments[0].authorization_id()]
            ))
        );

        block.data_commitments[0].replay_nonce += 1;
        assert_ne!(block.compute_body_root(), first);
        let bytes = borsh::to_vec(&block).unwrap();
        assert_eq!(Block::try_from_slice(&bytes).unwrap(), block);
    }

    #[derive(BorshSerialize)]
    struct LegacyUtxoBlock {
        header: BlockHeader,
        transactions: Vec<Transaction>,
    }

    #[derive(BorshSerialize)]
    struct LegacyV4Block {
        header: BlockHeader,
        transactions: Vec<Transaction>,
        account_transfers: Vec<AccountTransfer>,
        stake_ops: Vec<SignedStakeTx>,
        ovl_executions: Vec<OvlExecutionTx>,
        drc_payments: Vec<DrcPaymentTx>,
    }

    #[test]
    fn legacy_utxo_block_bytes_decode_with_empty_appended_lanes() {
        let legacy = LegacyUtxoBlock {
            header: BlockHeader {
                version: 1,
                parents: vec![Hash([9; 32])],
                timestamp_ms: 11,
                bits: 2,
                nonce: 3,
                tx_root: Hash::ZERO,
            },
            transactions: Vec::new(),
        };
        let bytes = borsh::to_vec(&legacy).unwrap();
        let decoded = Block::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.header, legacy.header);
        assert!(decoded.transactions.is_empty());
        assert!(decoded.account_transfers.is_empty());
        assert!(decoded.stake_ops.is_empty());
        assert!(decoded.ovl_executions.is_empty());
        assert!(decoded.drc_payments.is_empty());
        assert!(decoded.data_commitments.is_empty());
    }

    #[test]
    fn legacy_v4_block_bytes_decode_with_empty_data_lane() {
        let payment = DrcPaymentTx::unsigned(
            Address([1; 20]),
            Address([2; 20]),
            Amount::from_base_units(3),
            Amount::ZERO,
            4,
            Hash([5; 32]),
            6,
        );
        let legacy = LegacyV4Block {
            header: BlockHeader {
                version: 1,
                parents: vec![Hash([7; 32])],
                timestamp_ms: 8,
                bits: 9,
                nonce: 10,
                tx_root: Hash([11; 32]),
            },
            transactions: Vec::new(),
            account_transfers: Vec::new(),
            stake_ops: Vec::new(),
            ovl_executions: Vec::new(),
            drc_payments: vec![payment.clone()],
        };

        let decoded = Block::try_from_slice(&borsh::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(decoded.header, legacy.header);
        assert_eq!(decoded.drc_payments, vec![payment]);
        assert!(decoded.data_commitments.is_empty());
    }

    #[test]
    fn partial_appended_lane_length_is_not_treated_as_legacy_eof() {
        let legacy = LegacyUtxoBlock {
            header: BlockHeader {
                version: 1,
                parents: vec![],
                timestamp_ms: 0,
                bits: 0,
                nonce: 0,
                tx_root: Hash::ZERO,
            },
            transactions: Vec::new(),
        };
        let mut bytes = borsh::to_vec(&legacy).unwrap();
        bytes.push(1);
        assert!(Block::try_from_slice(&bytes).is_err());
    }
}
