//! Release timings for the TLT covenant library, coin selection, and Merkle proofs.
//!
//! These numbers are local measurements of this process. They are not a network
//! benchmark and they do not change consensus.

use std::hint::black_box;
use std::time::Instant;

use agora_types::{
    eval_tlt_script, prove_tlt_tx_merkle, push_data, script_p2pkh, select_tlt_coins,
    tlt_tx_merkle_root, verify_tlt_tx_merkle, Address, Hash, OutPoint, SigChecker, TltOutputOrigin,
    TltSpendCoin, TltSpendContext, TLT_SEQUENCE_FINAL,
};
use sha2::{Digest, Sha256};

struct EqChecker;

impl SigChecker for EqChecker {
    fn check_sig(&self, pubkey: &[u8], signature: &[u8], _message: &[u8]) -> bool {
        pubkey == signature
    }
}

fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

fn main() {
    let pubkey = [0x02u8; 33];
    let address = Address(sha256(&pubkey)[..20].try_into().expect("20 bytes"));
    let mut script_sig = Vec::new();
    push_data(&mut script_sig, &pubkey).expect("sig push");
    push_data(&mut script_sig, &pubkey).expect("pubkey push");
    let script_pubkey = script_p2pkh(&address);
    let ctx = TltSpendContext {
        sighash_preimage: b"agora-tlt-perf".to_vec(),
        lock_time: 0,
        sequence: TLT_SEQUENCE_FINAL,
        spend_blue_score: 1,
        median_time_past_secs: 0,
        origin: TltOutputOrigin {
            blue_score: 0,
            median_time_secs: 0,
        },
    };
    let evals = 10_000u32;
    let started = Instant::now();
    for _ in 0..evals {
        eval_tlt_script(
            black_box(&script_sig),
            black_box(&script_pubkey),
            black_box(&ctx),
            &EqChecker,
        )
        .expect("p2pkh");
    }
    let eval_us = started.elapsed().as_micros();

    let coins: Vec<TltSpendCoin> = (0..12)
        .map(|i| TltSpendCoin {
            outpoint: OutPoint {
                tx_id: Hash([i; 32]),
                index: 0,
            },
            value: 1_000 + u64::from(i),
        })
        .collect();
    let selects = 1_000u32;
    let started = Instant::now();
    for _ in 0..selects {
        let picked = select_tlt_coins(black_box(&coins), 5_000, 1).expect("select");
        black_box(picked.change);
    }
    let select_us = started.elapsed().as_micros();

    let leaves: Vec<Hash> = (0..4_096u32)
        .map(|i| Hash(sha256(&i.to_le_bytes())))
        .collect();
    let proofs = 100usize;
    let started = Instant::now();
    for i in 0..proofs {
        let root = tlt_tx_merkle_root(black_box(&leaves));
        let proof = prove_tlt_tx_merkle(&leaves, i % leaves.len()).expect("proof");
        assert!(verify_tlt_tx_merkle(black_box(&root), black_box(&proof)));
    }
    let merkle_us = started.elapsed().as_micros();

    println!("profile=release");
    println!("p2pkh_evals={evals} elapsed_us={eval_us}");
    println!("coinselect_12_calls={selects} elapsed_us={select_us}");
    println!("merkle_4096_prove_verify={proofs} elapsed_us={merkle_us}");
}
