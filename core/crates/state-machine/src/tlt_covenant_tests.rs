//! Block-admission checks for the TLT covenant lane.
//!
//! v1 transfers stay on their frozen wire. These tests spend the genesis premine
//! through covenant scripts and roll the UTXO set back.

use agora_consensus::{EmissionSchedule, COINBASE_MATURITY};
use agora_crypto::{
    derive_bip44, seed_from_mnemonic, sign_tlt_covenant_preimage, Bip44Path, KeyPair,
};
use agora_types::{
    prove_tlt_tx_merkle, push_data, script_htlc, script_multisig, script_p2pkh, script_p2sh,
    tlt_tx_merkle_root, verify_tlt_tx_merkle, Address, Amount, Block, BlockHeader, Hash, OutPoint,
    TltCovenantInput, TltCovenantOutput, TltCovenantTx, Transaction, TxOut,
    TLT_COVENANT_TX_VERSION, TLT_SEQUENCE_FINAL,
};
use borsh::BorshDeserialize;

use crate::apply::{
    apply_block_batched_with_auth_at_blue_score, balance_of, load_utxo, revert_journal, UtxoJournal,
};
use crate::genesis::GenesisBuilder;
use crate::monetary::TLT_MAX_SUPPLY_BASE;
use crate::tlt_covenant::load_covenant_utxo;
use crate::tx_index::{index_block_transactions, lookup_covenant_tx_location};
use crate::{StateStore, TxAuthContext};

const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

fn key(index: u32) -> KeyPair {
    let seed = seed_from_mnemonic(PHRASE, "").unwrap();
    derive_bip44(&seed, &Bip44Path::external(index)).unwrap()
}

fn coinbase(value: u64, nonce: u64) -> Transaction {
    Transaction::unsigned(
        1,
        vec![],
        vec![TxOut {
            value: Amount::from_base_units(value),
            address: Address::ZERO,
        }],
        nonce,
    )
}

fn seal(
    parents: Vec<Hash>,
    transactions: Vec<Transaction>,
    covenants: Vec<TltCovenantTx>,
    timestamp_ms: u64,
) -> Block {
    let mut header = BlockHeader {
        version: 1,
        parents,
        timestamp_ms,
        bits: 0,
        nonce: 0,
        tx_root: Hash::ZERO,
    };
    let mut block = Block::utxo(header.clone(), transactions);
    block.tlt_covenants = covenants;
    header.tx_root = block.compute_body_root();
    block.header = header;
    block
}

fn commit(
    store: &StateStore,
    block: &Block,
    blue_score: u64,
    auth: Option<&TxAuthContext>,
) -> UtxoJournal {
    let result =
        apply_block_batched_with_auth_at_blue_score(store, block, 0, auth, blue_score).unwrap();
    let journal = result.journal.clone();
    store.write_batch(result.batch).unwrap();
    journal
}

struct Premine {
    store: StateStore,
    genesis: Hash,
    owner: KeyPair,
    premine_txid: Hash,
    value: u64,
}

fn ignite_premine() -> Premine {
    let store = StateStore::open_in_memory();
    let owner = key(0);
    let genesis = GenesisBuilder::default()
        .with_premine_address(owner.address())
        .ignite(&store)
        .unwrap();
    let block = {
        let bytes = store
            .get_cf(crate::ColumnFamily::Hot, genesis.as_bytes())
            .unwrap()
            .unwrap();
        Block::try_from_slice(&bytes).unwrap()
    };
    let value = block.transactions[0].outputs[0].value.as_base_units();
    Premine {
        store,
        genesis,
        owner,
        premine_txid: block.transactions[0].tx_id(),
        value,
    }
}

fn covenant_tx(
    outpoint: OutPoint,
    sequence: u32,
    outputs: Vec<TltCovenantOutput>,
    lock_time: u64,
    nonce: u64,
) -> TltCovenantTx {
    TltCovenantTx {
        version: TLT_COVENANT_TX_VERSION,
        inputs: vec![TltCovenantInput {
            previous_outpoint: outpoint,
            sequence,
            script_sig: Vec::new(),
        }],
        outputs,
        lock_time,
        nonce,
    }
}

fn sign_input(tx: &mut TltCovenantTx, index: usize, signer: &KeyPair, auth: Option<(&str, &Hash)>) {
    let preimage = match auth {
        Some((chain_id, genesis)) => tx.sighash_preimage_bound(chain_id, genesis),
        None => tx.sighash_preimage(),
    };
    let signature = sign_tlt_covenant_preimage(signer, &preimage).unwrap();
    let mut script_sig = Vec::new();
    push_data(&mut script_sig, &signature).unwrap();
    push_data(&mut script_sig, &signer.public_key_bytes()).unwrap();
    tx.inputs[index].script_sig = script_sig;
}

fn premine_out(funded: &Premine) -> OutPoint {
    OutPoint {
        tx_id: funded.premine_txid,
        index: 0,
    }
}

#[test]
fn monetary_constants_and_empty_covenant_lane_stay_on_the_v1_wire() {
    let schedule = EmissionSchedule::default();
    assert_eq!(schedule.initial_reward, 50_0000_0000);
    assert_eq!(schedule.halving_interval, 210_000);
    assert_eq!(schedule.reward_at_blue_score(0), 50_0000_0000);
    assert_eq!(schedule.reward_at_blue_score(210_000), 25_0000_0000);
    assert_eq!(TLT_MAX_SUPPLY_BASE, 10_000_000_000_000_000);
    assert_eq!(COINBASE_MATURITY, 100);

    let bare = seal(vec![], vec![coinbase(0, 1)], vec![], 1);
    assert_eq!(
        bare.compute_body_root(),
        Block::compute_tx_root(&bare.transactions)
    );
    let encoded = borsh::to_vec(&bare).unwrap();
    let decoded = Block::try_from_slice(&encoded).unwrap();
    assert!(decoded.tlt_covenants.is_empty());
    assert_eq!(borsh::to_vec(&decoded).unwrap(), encoded);

    let legacy = borsh::to_vec(&UtxoJournal::default()).unwrap();
    assert!(legacy.ends_with(&[0, 0, 0, 0, 0, 0, 0, 0]));
    let stripped = UtxoJournal::from_bytes(&legacy[..legacy.len() - 8]).unwrap();
    assert!(stripped.tlt_covenant_created.is_empty());
    assert!(stripped.tlt_covenant_spent.is_empty());
}

#[test]
fn p2pkh_covenant_spends_premine_pays_fee_reverts_and_replays() {
    let funded = ignite_premine();
    let recipient = key(1).address();
    let fee = 1u64;
    let mut tx = covenant_tx(
        premine_out(&funded),
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(funded.value - fee),
            script_pubkey: script_p2pkh(&recipient),
        }],
        0,
        7,
    );
    sign_input(&mut tx, 0, &funded.owner, None);
    let block = seal(
        vec![funded.genesis],
        vec![coinbase(fee, 11)],
        vec![tx.clone()],
        2_000,
    );
    assert_ne!(
        block.header.tx_root,
        Block::compute_tx_root(&block.transactions)
    );
    let leaves: Vec<Hash> = block.transactions.iter().map(|tx| tx.tx_id()).collect();
    let merkle = tlt_tx_merkle_root(&leaves);
    assert_eq!(merkle, Block::compute_tx_root(&block.transactions));
    let proof = prove_tlt_tx_merkle(&leaves, 0).unwrap();
    assert!(verify_tlt_tx_merkle(&merkle, &proof));
    assert!(!verify_tlt_tx_merkle(&block.header.tx_root, &proof));

    let round = Block::try_from_slice(&borsh::to_vec(&block).unwrap()).unwrap();
    assert_eq!(round.tlt_covenants[0].tx_id(), tx.tx_id());

    let journal = commit(&funded.store, &block, 1, None);
    assert_eq!(journal.fees, fee);
    assert_eq!(
        balance_of(&funded.store, &recipient)
            .unwrap()
            .as_base_units(),
        funded.value - fee
    );
    assert!(load_utxo(&funded.store, &premine_out(&funded)).is_err());
    index_block_transactions(&funded.store, &block).unwrap();
    assert_eq!(
        lookup_covenant_tx_location(&funded.store, &tx.tx_id()).unwrap(),
        Some((block.id(), 0))
    );

    let replay = apply_block_batched_with_auth_at_blue_score(&funded.store, &block, 0, None, 1);
    assert!(replay.is_err(), "replay of a spent premine must fail");

    revert_journal(&funded.store, &journal).unwrap();
    assert_eq!(
        balance_of(&funded.store, &funded.owner.address())
            .unwrap()
            .as_base_units(),
        funded.value
    );
    assert_eq!(balance_of(&funded.store, &recipient).unwrap(), Amount::ZERO);
    let again = commit(&funded.store, &block, 1, None);
    assert_eq!(again.fees, fee);
}

#[test]
fn bound_sighash_rejects_cross_chain_replay() {
    let funded = ignite_premine();
    let mut tx = covenant_tx(
        premine_out(&funded),
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(funded.value - 1),
            script_pubkey: script_p2pkh(&key(2).address()),
        }],
        0,
        3,
    );
    sign_input(
        &mut tx,
        0,
        &funded.owner,
        Some(("chain-a", &funded.genesis)),
    );
    let block = seal(vec![funded.genesis], vec![coinbase(1, 4)], vec![tx], 2_000);
    let wrong = TxAuthContext {
        chain_id: "chain-b".into(),
        genesis: funded.genesis,
        data_availability_network_fingerprint: None,
    };
    assert!(
        apply_block_batched_with_auth_at_blue_score(&funded.store, &block, 0, Some(&wrong), 1)
            .is_err()
    );
    let right = TxAuthContext {
        chain_id: "chain-a".into(),
        genesis: funded.genesis,
        data_availability_network_fingerprint: None,
    };
    commit(&funded.store, &block, 1, Some(&right));
    assert!(load_utxo(&funded.store, &premine_out(&funded)).is_err());
}

#[test]
fn p2sh_multisig_and_htlc_are_block_accepted() {
    let funded = ignite_premine();
    let redeem = script_p2pkh(&funded.owner.address());
    let (p2sh, _) = script_p2sh(&redeem);
    let fee = 1u64;
    let mut fund = covenant_tx(
        premine_out(&funded),
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(funded.value - fee),
            script_pubkey: p2sh,
        }],
        0,
        1,
    );
    sign_input(&mut fund, 0, &funded.owner, None);
    let created = seal(
        vec![funded.genesis],
        vec![coinbase(fee, 2)],
        vec![fund.clone()],
        3_000,
    );
    commit(&funded.store, &created, 1, None);
    let locked = OutPoint {
        tx_id: fund.tx_id(),
        index: 0,
    };
    assert!(load_covenant_utxo(&funded.store, &locked)
        .unwrap()
        .is_some());
    assert!(load_utxo(&funded.store, &locked).is_err());

    let mut spend = covenant_tx(
        locked,
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(funded.value - fee - fee),
            script_pubkey: script_p2pkh(&key(3).address()),
        }],
        0,
        5,
    );
    let preimage = spend.sighash_preimage();
    let signature = sign_tlt_covenant_preimage(&funded.owner, &preimage).unwrap();
    let mut script_sig = Vec::new();
    push_data(&mut script_sig, &signature).unwrap();
    push_data(&mut script_sig, &funded.owner.public_key_bytes()).unwrap();
    push_data(&mut script_sig, &redeem).unwrap();
    spend.inputs[0].script_sig = script_sig;
    let spent = seal(
        vec![created.id()],
        vec![coinbase(fee, 6)],
        vec![spend],
        4_000,
    );
    commit(&funded.store, &spent, 2, None);
    assert!(load_covenant_utxo(&funded.store, &locked)
        .unwrap()
        .is_none());
    assert_eq!(
        balance_of(&funded.store, &key(3).address())
            .unwrap()
            .as_base_units(),
        funded.value - 2
    );

    let second = ignite_premine();
    let other = key(1);
    let pubs = [second.owner.public_key_bytes(), other.public_key_bytes()];
    let multisig = script_multisig(2, &pubs).unwrap();
    let mut fund_ms = covenant_tx(
        premine_out(&second),
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(second.value - 1),
            script_pubkey: multisig,
        }],
        0,
        8,
    );
    sign_input(&mut fund_ms, 0, &second.owner, None);
    let ms_block = seal(
        vec![second.genesis],
        vec![coinbase(1, 9)],
        vec![fund_ms.clone()],
        5_000,
    );
    commit(&second.store, &ms_block, 1, None);
    let ms_out = OutPoint {
        tx_id: fund_ms.tx_id(),
        index: 0,
    };
    let mut ms_spend = covenant_tx(
        ms_out,
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(second.value - 2),
            script_pubkey: script_p2pkh(&key(4).address()),
        }],
        0,
        10,
    );
    let ms_preimage = ms_spend.sighash_preimage();
    let sig0 = sign_tlt_covenant_preimage(&second.owner, &ms_preimage).unwrap();
    let sig1 = sign_tlt_covenant_preimage(&other, &ms_preimage).unwrap();
    let mut ms_sig = Vec::new();
    push_data(&mut ms_sig, &sig0).unwrap();
    push_data(&mut ms_sig, &sig1).unwrap();
    ms_spend.inputs[0].script_sig = ms_sig;
    commit(
        &second.store,
        &seal(
            vec![ms_block.id()],
            vec![coinbase(1, 12)],
            vec![ms_spend],
            6_000,
        ),
        2,
        None,
    );
    assert_eq!(
        balance_of(&second.store, &key(4).address())
            .unwrap()
            .as_base_units(),
        second.value - 2
    );

    let third = ignite_premine();
    let preimage_bytes = b"agora-htlc-preimage";
    let hash = Hash::hash_bytes(preimage_bytes);
    let recipient = key(5);
    let htlc = script_htlc(
        &hash,
        &recipient.public_key_bytes(),
        &third.owner.public_key_bytes(),
        20,
    )
    .unwrap();
    let mut fund_htlc = covenant_tx(
        premine_out(&third),
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(third.value - 1),
            script_pubkey: htlc,
        }],
        0,
        13,
    );
    sign_input(&mut fund_htlc, 0, &third.owner, None);
    let htlc_block = seal(
        vec![third.genesis],
        vec![coinbase(1, 14)],
        vec![fund_htlc.clone()],
        7_000,
    );
    commit(&third.store, &htlc_block, 1, None);
    let htlc_out = OutPoint {
        tx_id: fund_htlc.tx_id(),
        index: 0,
    };
    let mut claim = covenant_tx(
        htlc_out,
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(third.value - 2),
            script_pubkey: script_p2pkh(&recipient.address()),
        }],
        0,
        15,
    );
    let claim_preimage = claim.sighash_preimage();
    let claim_sig = sign_tlt_covenant_preimage(&recipient, &claim_preimage).unwrap();
    let mut bad = Vec::new();
    push_data(&mut bad, &claim_sig).unwrap();
    push_data(&mut bad, b"wrong").unwrap();
    bad.push(0x51);
    claim.inputs[0].script_sig = bad;
    let bad_block = seal(
        vec![htlc_block.id()],
        vec![coinbase(1, 16)],
        vec![claim.clone()],
        8_000,
    );
    assert!(
        apply_block_batched_with_auth_at_blue_score(&third.store, &bad_block, 0, None, 2).is_err()
    );
    let mut good_sig = Vec::new();
    push_data(&mut good_sig, &claim_sig).unwrap();
    push_data(&mut good_sig, preimage_bytes).unwrap();
    good_sig.push(0x51);
    claim.inputs[0].script_sig = good_sig;
    commit(
        &third.store,
        &seal(
            vec![htlc_block.id()],
            vec![coinbase(1, 16)],
            vec![claim],
            8_000,
        ),
        2,
        None,
    );
    assert_eq!(
        balance_of(&third.store, &recipient.address())
            .unwrap()
            .as_base_units(),
        third.value - 2
    );
}

#[test]
fn cltv_csv_and_malformed_scripts_fail_closed() {
    let funded = ignite_premine();
    let mut cltv = Vec::new();
    push_data(&mut cltv, &10u64.to_le_bytes()).unwrap();
    cltv.push(0xb1);
    cltv.push(0x75);
    push_data(&mut cltv, &funded.owner.public_key_bytes()).unwrap();
    cltv.push(0xac);
    let mut fund = covenant_tx(
        premine_out(&funded),
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(funded.value - 1),
            script_pubkey: cltv,
        }],
        0,
        1,
    );
    sign_input(&mut fund, 0, &funded.owner, None);
    let created = seal(
        vec![funded.genesis],
        vec![coinbase(1, 2)],
        vec![fund.clone()],
        9_000,
    );
    commit(&funded.store, &created, 0, None);
    let locked = OutPoint {
        tx_id: fund.tx_id(),
        index: 0,
    };

    let mut early = covenant_tx(
        locked,
        TLT_SEQUENCE_FINAL - 1,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(funded.value - 2),
            script_pubkey: script_p2pkh(&key(6).address()),
        }],
        10,
        3,
    );
    let early_sig = sign_tlt_covenant_preimage(&funded.owner, &early.sighash_preimage()).unwrap();
    let mut early_script = Vec::new();
    push_data(&mut early_script, &early_sig).unwrap();
    early.inputs[0].script_sig = early_script;
    assert!(apply_block_batched_with_auth_at_blue_score(
        &funded.store,
        &seal(
            vec![created.id()],
            vec![coinbase(1, 4)],
            vec![early.clone()],
            10_000,
        ),
        0,
        None,
        9,
    )
    .is_err());
    commit(
        &funded.store,
        &seal(
            vec![created.id()],
            vec![coinbase(1, 4)],
            vec![early],
            10_000,
        ),
        10,
        None,
    );

    let csv_funded = ignite_premine();
    let mut csv = vec![0x52, 0xb2, 0x75];
    push_data(&mut csv, &csv_funded.owner.public_key_bytes()).unwrap();
    csv.push(0xac);
    let mut csv_fund = covenant_tx(
        premine_out(&csv_funded),
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(csv_funded.value - 1),
            script_pubkey: csv,
        }],
        0,
        20,
    );
    sign_input(&mut csv_fund, 0, &csv_funded.owner, None);
    let csv_created = seal(
        vec![csv_funded.genesis],
        vec![coinbase(1, 21)],
        vec![csv_fund.clone()],
        11_000,
    );
    commit(&csv_funded.store, &csv_created, 0, None);
    let csv_out = OutPoint {
        tx_id: csv_fund.tx_id(),
        index: 0,
    };
    let mut csv_spend = covenant_tx(
        csv_out,
        2,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(csv_funded.value - 2),
            script_pubkey: script_p2pkh(&key(7).address()),
        }],
        0,
        22,
    );
    let csv_sig =
        sign_tlt_covenant_preimage(&csv_funded.owner, &csv_spend.sighash_preimage()).unwrap();
    let mut csv_script = Vec::new();
    push_data(&mut csv_script, &csv_sig).unwrap();
    csv_spend.inputs[0].script_sig = csv_script;
    assert!(apply_block_batched_with_auth_at_blue_score(
        &csv_funded.store,
        &seal(
            vec![csv_created.id()],
            vec![coinbase(1, 23)],
            vec![csv_spend.clone()],
            12_000,
        ),
        0,
        None,
        1,
    )
    .is_err());
    commit(
        &csv_funded.store,
        &seal(
            vec![csv_created.id()],
            vec![coinbase(1, 23)],
            vec![csv_spend],
            12_000,
        ),
        2,
        None,
    );

    let broken = ignite_premine();
    let mut bad_fund = covenant_tx(
        premine_out(&broken),
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(broken.value - 1),
            script_pubkey: vec![0xff],
        }],
        0,
        30,
    );
    sign_input(&mut bad_fund, 0, &broken.owner, None);
    let bad_created = seal(
        vec![broken.genesis],
        vec![coinbase(1, 31)],
        vec![bad_fund.clone()],
        13_000,
    );
    commit(&broken.store, &bad_created, 1, None);
    let mut bad_spend = covenant_tx(
        OutPoint {
            tx_id: bad_fund.tx_id(),
            index: 0,
        },
        TLT_SEQUENCE_FINAL,
        vec![TltCovenantOutput {
            value: Amount::from_base_units(1),
            script_pubkey: script_p2pkh(&key(8).address()),
        }],
        0,
        32,
    );
    sign_input(&mut bad_spend, 0, &broken.owner, None);
    let err = match apply_block_batched_with_auth_at_blue_score(
        &broken.store,
        &seal(
            vec![bad_created.id()],
            vec![coinbase(1, 33)],
            vec![bad_spend],
            14_000,
        ),
        0,
        None,
        2,
    ) {
        Err(err) => err,
        Ok(_) => panic!("malformed covenant spend was accepted"),
    };
    let message = err.to_string();
    assert!(
        message.contains("unknown") || message.contains("opcode") || message.contains("script"),
        "{message}"
    );
}
