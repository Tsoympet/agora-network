use std::time::Instant;

use agora_types::{OvlWei, OVL_EVM_DEV_CHAIN_ID};
use alloy_primitives::keccak256;
use k256::ecdsa::SigningKey;

use crate::exec::{apply_raw_transaction, eth_call, measure_shanghai_gas};
use crate::index::index_receipts;
use crate::rpc::{dispatch, dispatch_with_view, EthNodeView, EthSyncStatus, PendingTx};
use crate::tx::{
    dev_signing_key, ethereum_address_from_signing_key, parse_raw_transaction, sign_eip1559,
    sign_legacy,
};
use crate::world::OvlEvmWorld;
use crate::{reject_foreign_native_asset, PROFILE_JSON};

fn funded() -> (OvlEvmWorld, SigningKey, [u8; 20]) {
    let mut world = OvlEvmWorld::dev();
    world.block.base_fee = 1;
    world.base_fee = 1;
    let key = dev_signing_key();
    let caller = ethereum_address_from_signing_key(&key);
    world
        .fund(caller, OvlWei::from_u128(10u128.pow(24)))
        .unwrap();
    (world, key, caller)
}

fn value_word(value: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..].copy_from_slice(&value.to_be_bytes());
    out
}

fn address_word(address: [u8; 20]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(&address);
    out
}

fn selector(signature: &str) -> Vec<u8> {
    keccak256(signature.as_bytes())[..4].to_vec()
}

fn bytecode(name: &str) -> Vec<u8> {
    let text = match name {
        "Store" => include_str!("../fixtures/bin/Store.bin"),
        "Erc20" => include_str!("../fixtures/bin/Erc20.bin"),
        "Erc721" => include_str!("../fixtures/bin/Erc721.bin"),
        "Erc1155" => include_str!("../fixtures/bin/Erc1155.bin"),
        "Erc4626" => include_str!("../fixtures/bin/Erc4626.bin"),
        "MultiSig" => include_str!("../fixtures/bin/MultiSig.bin"),
        "Counter" => include_str!("../fixtures/bin/Counter.bin"),
        "Proxy" => include_str!("../fixtures/bin/Proxy.bin"),
        "Timelock" => include_str!("../fixtures/bin/Timelock.bin"),
        "Amm" => include_str!("../fixtures/bin/Amm.bin"),
        "PrecompileProbe" => include_str!("../fixtures/bin/PrecompileProbe.bin"),
        "Create2Factory" => include_str!("../fixtures/bin/Create2Factory.bin"),
        "StaticProbe" => include_str!("../fixtures/bin/StaticProbe.bin"),
        _ => panic!("unknown fixture"),
    };
    hex::decode(text.trim()).unwrap()
}

fn deploy(world: &mut OvlEvmWorld, key: &SigningKey, init: &[u8]) -> [u8; 20] {
    let caller = ethereum_address_from_signing_key(key);
    let raw = sign_eip1559(
        key,
        world.chain_id,
        world.nonce(&caller),
        0,
        u128::from(world.block.base_fee),
        4_000_000,
        None,
        [0u8; 32],
        init,
        &[],
    );
    let receipt = apply_raw_transaction(world, &raw).unwrap();
    assert!(
        receipt.status,
        "deploy failed: {}",
        hex::encode(&receipt.output)
    );
    receipt.contract_address.expect("created")
}

fn call(
    world: &mut OvlEvmWorld,
    key: &SigningKey,
    to: [u8; 20],
    data: &[u8],
    value: [u8; 32],
) -> crate::EvmReceipt {
    let caller = ethereum_address_from_signing_key(key);
    let raw = sign_eip1559(
        key,
        world.chain_id,
        world.nonce(&caller),
        1,
        u128::from(world.block.base_fee) + 1,
        2_000_000,
        Some(to),
        value,
        data,
        &[],
    );
    let receipt = apply_raw_transaction(world, &raw).unwrap();
    assert!(
        receipt.status,
        "call failed: {}",
        hex::encode(&receipt.output)
    );
    receipt
}

fn encode_words(selector_sig: &str, words: &[[u8; 32]]) -> Vec<u8> {
    let mut out = selector(selector_sig);
    for word in words {
        out.extend_from_slice(word);
    }
    out
}

fn encode_address_bytes_word(
    signature: &str,
    address: [u8; 20],
    data: &[u8],
    extra: [u8; 32],
) -> Vec<u8> {
    let mut out = selector(signature);
    out.extend_from_slice(&address_word(address));
    out.extend_from_slice(&value_word(0x60));
    out.extend_from_slice(&extra);
    out.extend_from_slice(&value_word(data.len() as u64));
    out.extend_from_slice(data);
    let pad = (32 - (data.len() % 32)) % 32;
    out.extend(std::iter::repeat_n(0u8, pad));
    out
}

#[test]
fn inactive_gate_rejects_execution_and_foreign_assets() {
    let mut world = OvlEvmWorld::inactive();
    let key = dev_signing_key();
    let raw = sign_legacy(
        &key,
        OVL_EVM_DEV_CHAIN_ID,
        0,
        1,
        21_000,
        Some([0x22; 20]),
        [0u8; 32],
        &[],
    );
    assert!(apply_raw_transaction(&mut world, &raw)
        .unwrap_err()
        .to_string()
        .contains("inactive"));
    assert!(reject_foreign_native_asset("DRC").is_err());
    assert!(reject_foreign_native_asset("TLT").is_err());
    assert!(reject_foreign_native_asset("OVL").is_ok());
    assert!(parse_raw_transaction(&borsh_like_drc()).is_err());
    let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
    assert_eq!(profile["profile"], "OVL-EVM-v1");
    assert_eq!(profile["engine"]["spec_id"], "SHANGHAI");
    assert!(profile["exclusions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item.as_str().unwrap().contains("DRC")));
}

fn borsh_like_drc() -> Vec<u8> {
    let mut raw = b"DRC-payment".to_vec();
    raw.extend_from_slice(&[0u8; 40]);
    raw
}

#[test]
fn eip1559_burns_base_fee_and_pays_tip_to_beneficiary() {
    let (mut world, key, caller) = funded();
    let beneficiary = world.block.beneficiary;
    let recipient = [0x33u8; 20];
    let before = world.balance(&caller);
    let raw = sign_eip1559(
        &key,
        world.chain_id,
        0,
        2,
        3,
        21_000,
        Some(recipient),
        value_word(5),
        &[],
        &[],
    );
    let receipt = apply_raw_transaction(&mut world, &raw).unwrap();
    assert!(receipt.status);
    assert_eq!(receipt.gas_used, 21_000);
    assert_eq!(receipt.effective_gas_price, 3);
    assert_eq!(world.balance(&recipient), OvlWei::from_u64(5));
    assert_eq!(world.balance(&beneficiary), OvlWei::from_u64(42_000));
    assert_eq!(world.burned, OvlWei::from_u64(21_000));
    let paid = OvlWei::from_u64(21_000 * 3 + 5);
    assert_eq!(world.balance(&caller), before.checked_sub(paid).unwrap());
    world.check_supply().unwrap();
}

#[test]
fn legacy_transfer_matches_pinned_revm_gas() {
    let (mut world, key, caller) = funded();
    let raw = sign_legacy(
        &key,
        world.chain_id,
        0,
        2,
        21_000,
        Some([0x44; 20]),
        value_word(1),
        &[],
    );
    let before = world.clone();
    let receipt = apply_raw_transaction(&mut world, &raw).unwrap();
    assert_eq!(receipt.gas_used, 21_000);
    assert_eq!(world.nonce(&caller), 1);
    let direct = measure_shanghai_gas(&before, &raw).unwrap();
    assert_eq!(direct, receipt.gas_used);
}

#[test]
fn fixtures_execute_create_calls_tokens_and_precompiles() {
    let (mut world, key, caller) = funded();
    let store_init = bytecode("Store");
    let store = deploy(&mut world, &key, &store_init);
    call(
        &mut world,
        &key,
        store,
        &encode_words("set(uint256)", &[value_word(9)]),
        [0u8; 32],
    );
    let stored = eth_call(
        &world,
        caller,
        Some(store),
        [0u8; 32],
        &selector("value()"),
        100_000,
    )
    .unwrap();
    assert_eq!(&stored[stored.len() - 1], &9);

    let erc20_init = {
        let mut init = bytecode("Erc20");
        init.extend_from_slice(&value_word(1_000));
        init
    };
    let erc20 = deploy(&mut world, &key, &erc20_init);
    let recipient = [0x55u8; 20];
    call(
        &mut world,
        &key,
        erc20,
        &encode_words(
            "transfer(address,uint256)",
            &[address_word(recipient), value_word(40)],
        ),
        [0u8; 32],
    );
    let balance = eth_call(
        &world,
        caller,
        Some(erc20),
        [0u8; 32],
        &encode_words("balanceOf(address)", &[address_word(recipient)]),
        100_000,
    )
    .unwrap();
    assert_eq!(balance[31], 40);

    let erc721 = deploy(&mut world, &key, &bytecode("Erc721"));
    call(
        &mut world,
        &key,
        erc721,
        &encode_words(
            "mint(address,uint256)",
            &[address_word(caller), value_word(7)],
        ),
        [0u8; 32],
    );
    let owner = eth_call(
        &world,
        caller,
        Some(erc721),
        [0u8; 32],
        &encode_words("ownerOf(uint256)", &[value_word(7)]),
        100_000,
    )
    .unwrap();
    assert_eq!(&owner[12..], &caller);

    let erc1155 = deploy(&mut world, &key, &bytecode("Erc1155"));
    call(
        &mut world,
        &key,
        erc1155,
        &encode_words(
            "mint(address,uint256,uint256)",
            &[address_word(caller), value_word(3), value_word(8)],
        ),
        [0u8; 32],
    );

    let vault_init = {
        let mut init = bytecode("Erc4626");
        init.extend_from_slice(&address_word(erc20));
        init
    };
    let vault = deploy(&mut world, &key, &vault_init);
    call(
        &mut world,
        &key,
        erc20,
        &encode_words(
            "approve(address,uint256)",
            &[address_word(vault), value_word(15)],
        ),
        [0u8; 32],
    );
    call(
        &mut world,
        &key,
        vault,
        &encode_words(
            "deposit(uint256,address)",
            &[value_word(15), address_word(caller)],
        ),
        [0u8; 32],
    );
    let shares = eth_call(
        &world,
        caller,
        Some(vault),
        [0u8; 32],
        &encode_words("balanceOf(address)", &[address_word(caller)]),
        200_000,
    )
    .unwrap();
    assert_eq!(shares[31], 15);

    let counter = deploy(&mut world, &key, &bytecode("Counter"));
    let proxy_init = {
        let mut init = bytecode("Proxy");
        init.extend_from_slice(&address_word(counter));
        init
    };
    let proxy = deploy(&mut world, &key, &proxy_init);
    call(&mut world, &key, proxy, &selector("inc()"), [0u8; 32]);
    let proxied = eth_call(
        &world,
        caller,
        Some(proxy),
        [0u8; 32],
        &selector("value()"),
        100_000,
    )
    .unwrap();
    assert_eq!(proxied[31], 1);
    let direct = eth_call(
        &world,
        caller,
        Some(counter),
        [0u8; 32],
        &selector("value()"),
        100_000,
    )
    .unwrap();
    assert_eq!(direct[31], 0);

    let other = SigningKey::from_bytes((&[0x22u8; 32]).into()).unwrap();
    let other_addr = ethereum_address_from_signing_key(&other);
    world
        .fund(other_addr, OvlWei::from_u128(10u128.pow(21)))
        .unwrap();
    let multisig_init = {
        let mut init = bytecode("MultiSig");
        init.extend_from_slice(&address_word(caller));
        init.extend_from_slice(&address_word(other_addr));
        init
    };
    let multisig = deploy(&mut world, &key, &multisig_init);
    let payload = selector("inc()");
    let confirm = encode_address_bytes_word("confirm(address,bytes)", counter, &payload, [0u8; 32]);
    // confirm's third head word is unused; the helper places extra at the salt slot.
    // Rebuild with the ABI the contract expects: address, bytes.
    let confirm_a = encode_address_and_bytes("confirm(address,bytes)", counter, &payload);
    call(&mut world, &key, multisig, &confirm_a, [0u8; 32]);
    call(&mut world, &other, multisig, &confirm_a, [0u8; 32]);
    let execute = encode_address_and_bytes("execute(address,bytes)", counter, &payload);
    call(&mut world, &key, multisig, &execute, [0u8; 32]);
    let after = eth_call(
        &world,
        caller,
        Some(counter),
        [0u8; 32],
        &selector("value()"),
        100_000,
    )
    .unwrap();
    assert_eq!(after[31], 1);
    let _ = confirm;

    let timelock = deploy(&mut world, &key, &bytecode("Timelock"));
    let queued = encode_address_bytes_word(
        "queue(address,bytes,uint256)",
        counter,
        &payload,
        value_word(0),
    );
    call(&mut world, &key, timelock, &queued, [0u8; 32]);
    let execute_lock = encode_address_bytes_word(
        "execute(address,bytes,uint256)",
        counter,
        &payload,
        value_word(0),
    );
    call(&mut world, &key, timelock, &execute_lock, [0u8; 32]);
    let locked = eth_call(
        &world,
        caller,
        Some(counter),
        [0u8; 32],
        &selector("value()"),
        100_000,
    )
    .unwrap();
    assert_eq!(locked[31], 2);

    let amm = deploy(&mut world, &key, &bytecode("Amm"));
    call(
        &mut world,
        &key,
        amm,
        &encode_words(
            "add(uint256,uint256)",
            &[value_word(1_000), value_word(1_000)],
        ),
        [0u8; 32],
    );
    call(
        &mut world,
        &key,
        amm,
        &encode_words("swap0for1(uint256)", &[value_word(10)]),
        [0u8; 32],
    );
    let reserve = eth_call(
        &world,
        caller,
        Some(amm),
        [0u8; 32],
        &selector("reserve0()"),
        100_000,
    )
    .unwrap();
    assert_eq!(u64::from_be_bytes(reserve[24..].try_into().unwrap()), 1_010);

    let probe = deploy(&mut world, &key, &bytecode("PrecompileProbe"));
    let keccak_call = encode_bytes("keccakOf(bytes)", b"hi");
    let got = eth_call(
        &world,
        caller,
        Some(probe),
        [0u8; 32],
        &keccak_call,
        200_000,
    )
    .unwrap();
    assert_eq!(got.as_slice(), keccak256(b"hi").as_slice());
    let sha_call = encode_bytes("sha256Of(bytes)", b"hi");
    let sha = eth_call(&world, caller, Some(probe), [0u8; 32], &sha_call, 200_000).unwrap();
    assert_eq!(sha.len(), 32);
    assert_ne!(sha, got);

    let static_probe = deploy(&mut world, &key, &bytecode("StaticProbe"));
    let read = encode_words("read(address)", &[address_word(static_probe)]);
    let echoed = eth_call(
        &world,
        caller,
        Some(static_probe),
        [0u8; 32],
        &read,
        200_000,
    )
    .unwrap();
    assert_eq!(echoed[31], 7);

    let factory = deploy(&mut world, &key, &bytecode("Create2Factory"));
    let runtime = hex::decode("6001600055600060005500").unwrap();
    let init = wrap_runtime(&runtime);
    let salt = value_word(4);
    let create2 = encode_bytes_and_word("deploy(bytes,bytes32)", &init, salt);
    let created = call(&mut world, &key, factory, &create2, [0u8; 32]);
    let created_addr = {
        let out = &created.output;
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&out[out.len() - 20..]);
        addr
    };
    assert_ne!(created_addr, [0u8; 20]);
    call(&mut world, &key, created_addr, &[], [0u8; 32]);

    let indexed = index_receipts(&world.receipts);
    assert!(!indexed["erc20"].as_array().unwrap().is_empty());
    assert!(!indexed["erc721"].as_array().unwrap().is_empty());
    assert!(!indexed["erc1155"].as_array().unwrap().is_empty());
}

fn encode_address_and_bytes(signature: &str, address: [u8; 20], data: &[u8]) -> Vec<u8> {
    let mut out = selector(signature);
    out.extend_from_slice(&address_word(address));
    out.extend_from_slice(&value_word(0x40));
    out.extend_from_slice(&value_word(data.len() as u64));
    out.extend_from_slice(data);
    let pad = (32 - (data.len() % 32)) % 32;
    out.extend(std::iter::repeat_n(0u8, pad));
    out
}

fn encode_bytes(signature: &str, data: &[u8]) -> Vec<u8> {
    let mut out = selector(signature);
    out.extend_from_slice(&value_word(0x20));
    out.extend_from_slice(&value_word(data.len() as u64));
    out.extend_from_slice(data);
    let pad = (32 - (data.len() % 32)) % 32;
    out.extend(std::iter::repeat_n(0u8, pad));
    out
}

fn encode_bytes_and_word(signature: &str, data: &[u8], word: [u8; 32]) -> Vec<u8> {
    let mut out = selector(signature);
    out.extend_from_slice(&value_word(0x40));
    out.extend_from_slice(&word);
    out.extend_from_slice(&value_word(data.len() as u64));
    out.extend_from_slice(data);
    let pad = (32 - (data.len() % 32)) % 32;
    out.extend(std::iter::repeat_n(0u8, pad));
    out
}

fn wrap_runtime(runtime: &[u8]) -> Vec<u8> {
    let len = runtime.len() as u8;
    let offset = 12u8;
    let mut init = vec![
        0x60, len, 0x60, offset, 0x60, 0x00, 0x39, 0x60, len, 0x60, 0x00, 0xf3,
    ];
    init.extend_from_slice(runtime);
    init
}

#[test]
fn reorg_restart_and_two_worlds_share_a_subroot() {
    let (mut left, key, caller) = funded();
    let snapshot = left.to_bytes();
    let raw = sign_eip1559(
        &key,
        left.chain_id,
        0,
        0,
        1,
        21_000,
        Some([0x66; 20]),
        value_word(9),
        &[],
        &[],
    );
    apply_raw_transaction(&mut left, &raw).unwrap();
    let changed = left.execution_subroot();
    let restored = OvlEvmWorld::from_bytes(&snapshot).unwrap();
    assert_ne!(restored.execution_subroot(), changed);
    let mut right = OvlEvmWorld::from_bytes(&snapshot).unwrap();
    apply_raw_transaction(&mut right, &raw).unwrap();
    assert_eq!(left.execution_subroot(), right.execution_subroot());
    let restarted = OvlEvmWorld::from_bytes(&left.to_bytes()).unwrap();
    assert_eq!(restarted.balance(&caller), left.balance(&caller));
}

#[test]
fn rpc_reads_canonical_state_and_pending_does_not_commit() {
    let (mut world, key, caller) = funded();
    let raw = sign_eip1559(
        &key,
        world.chain_id,
        0,
        0,
        1,
        21_000,
        Some([0x77; 20]),
        value_word(4),
        &[],
        &[],
    );
    let before = world.execution_subroot();
    let mut pending = std::collections::BTreeMap::<[u8; 32], PendingTx>::new();
    let sent = dispatch(
        &world,
        &mut pending,
        "eth_sendRawTransaction",
        &serde_json::json!([format!("0x{}", hex::encode(&raw))]),
    )
    .unwrap();
    assert_eq!(world.execution_subroot(), before);
    assert!(sent.as_str().unwrap().starts_with("0x"));
    let pending_nonce = dispatch(
        &world,
        &mut pending,
        "eth_getTransactionCount",
        &serde_json::json!([format!("0x{}", hex::encode(caller)), "pending"]),
    )
    .unwrap();
    assert_eq!(pending_nonce, "0x1");
    let pending_balance = dispatch(
        &world,
        &mut pending,
        "eth_getBalance",
        &serde_json::json!([format!("0x{}", hex::encode(caller)), "pending"]),
    );
    assert!(pending_balance.unwrap_err().to_string().contains("pending"));
    let looked_up = dispatch(
        &world,
        &mut pending,
        "eth_getTransactionByHash",
        &serde_json::json!([sent.as_str().unwrap()]),
    )
    .unwrap();
    assert!(looked_up.get("blockNumber").unwrap().is_null());
    assert_eq!(looked_up["gas"], "0x5208");
    let receipt = apply_raw_transaction(&mut world, &raw).unwrap();
    let mut pending = std::collections::BTreeMap::new();
    let chain = dispatch(&world, &mut pending, "eth_chainId", &serde_json::json!([])).unwrap();
    assert_eq!(chain, "0x12110");
    let balance = dispatch(
        &world,
        &mut pending,
        "eth_getBalance",
        &serde_json::json!([format!("0x{}", hex::encode(caller)), "latest"]),
    )
    .unwrap();
    assert!(balance.as_str().unwrap().starts_with("0x"));
    let got = dispatch(
        &world,
        &mut pending,
        "eth_getTransactionReceipt",
        &serde_json::json!([format!("0x{}", hex::encode(receipt.hash))]),
    )
    .unwrap();
    assert_eq!(got["status"], "0x1");
    let version = dispatch(
        &world,
        &mut pending,
        "web3_clientVersion",
        &serde_json::json!([]),
    )
    .unwrap();
    assert!(version.as_str().unwrap().contains("OVL-EVM-v1"));
    let syncing = dispatch(&world, &mut pending, "eth_syncing", &serde_json::json!([])).unwrap();
    assert_eq!(syncing, false);
    let foreign = dispatch(
        &world,
        &mut pending,
        "eth_call",
        &serde_json::json!([{ "asset": "DRC" }]),
    );
    assert!(foreign.unwrap_err().to_string().contains("DRC"));
    let _ = dispatch(&world, &mut pending, "net_version", &serde_json::json!([])).unwrap();
    let _ = dispatch(
        &world,
        &mut pending,
        "eth_blockNumber",
        &serde_json::json!([]),
    )
    .unwrap();
    let _ = dispatch(&world, &mut pending, "eth_gasPrice", &serde_json::json!([])).unwrap();
    let _ = dispatch(
        &world,
        &mut pending,
        "eth_maxPriorityFeePerGas",
        &serde_json::json!([]),
    )
    .unwrap();
    let _ = dispatch(
        &world,
        &mut pending,
        "eth_feeHistory",
        &serde_json::json!([1, "latest", []]),
    )
    .unwrap();
    let head = dispatch(
        &world,
        &mut pending,
        "eth_getBlockByNumber",
        &serde_json::json!(["latest", false]),
    )
    .unwrap();
    assert!(head.get("number").is_some());
    let missing = dispatch(
        &world,
        &mut pending,
        "eth_getBlockByNumber",
        &serde_json::json!(["0x99", false]),
    )
    .unwrap();
    assert!(missing.is_null());
    let history = dispatch(
        &world,
        &mut pending,
        "eth_feeHistory",
        &serde_json::json!([4, "latest", [50]]),
    )
    .unwrap();
    assert_eq!(history["gasUsedRatio"].as_array().unwrap().len(), 1);
    assert_eq!(history["baseFeePerGas"].as_array().unwrap().len(), 2);
    assert_eq!(history["reward"].as_array().unwrap().len(), 1);
    let proof = dispatch(&world, &mut pending, "eth_getProof", &serde_json::json!([])).unwrap_err();
    assert!(matches!(proof, crate::EvmError::MethodNotFound(_)));
    let blob = dispatch(
        &world,
        &mut pending,
        "eth_sendRawTransaction",
        &serde_json::json!(["0x03"]),
    );
    assert!(blob.unwrap_err().to_string().contains("blob"));
    let tlt = dispatch(
        &world,
        &mut pending,
        "eth_getCode",
        &serde_json::json!([{ "asset": "TLT" }]),
    );
    assert!(tlt.unwrap_err().to_string().contains("TLT"));
    let caller_hex = format!("0x{}", hex::encode(caller));
    let code = dispatch(
        &world,
        &mut pending,
        "eth_getCode",
        &serde_json::json!([&caller_hex, "0x0"]),
    )
    .unwrap();
    assert_eq!(code, serde_json::json!("0x"));
    let slot = dispatch(
        &world,
        &mut pending,
        "eth_getStorageAt",
        &serde_json::json!([&caller_hex, "0x0", "latest"]),
    )
    .unwrap();
    assert_eq!(slot, serde_json::json!(format!("0x{}", "00".repeat(32))));
    let _ = dispatch(
        &world,
        &mut pending,
        "eth_getLogs",
        &serde_json::json!([{}]),
    )
    .unwrap();
    let _ = dispatch(
        &world,
        &mut pending,
        "net_peerCount",
        &serde_json::json!([]),
    )
    .unwrap();
}

#[test]
fn node_view_reports_real_peers_and_refuses_raw_submission_when_closed() {
    let (world, key, _) = funded();
    let mut pending = std::collections::BTreeMap::new();
    let view = EthNodeView {
        listening: true,
        peer_count: 3,
        syncing: Some(EthSyncStatus {
            starting_block: 1,
            current_block: 2,
            highest_block: 4,
        }),
        accept_raw_transactions: false,
    };
    let syncing = dispatch_with_view(
        &world,
        &mut pending,
        &view,
        "eth_syncing",
        &serde_json::json!([]),
    )
    .unwrap();
    assert_eq!(syncing["startingBlock"], "0x1");
    assert_eq!(syncing["currentBlock"], "0x2");
    assert_eq!(syncing["highestBlock"], "0x4");
    assert_eq!(
        dispatch_with_view(
            &world,
            &mut pending,
            &view,
            "net_peerCount",
            &serde_json::json!([])
        )
        .unwrap(),
        "0x3"
    );
    assert_eq!(
        dispatch_with_view(
            &world,
            &mut pending,
            &view,
            "net_listening",
            &serde_json::json!([])
        )
        .unwrap(),
        true
    );
    let raw = sign_eip1559(
        &key,
        world.chain_id,
        0,
        0,
        1,
        21_000,
        Some([0x11; 20]),
        [0u8; 32],
        &[],
        &[],
    );
    let rejected = dispatch_with_view(
        &world,
        &mut pending,
        &view,
        "eth_sendRawTransaction",
        &serde_json::json!([format!("0x{}", hex::encode(raw))]),
    );
    assert!(rejected.unwrap_err().to_string().contains("dev and test"));
    assert!(pending.is_empty());
}

#[test]
fn malformed_gas_grief_and_replay_are_rejected() {
    let (mut world, key, _) = funded();
    let over = sign_eip1559(
        &key,
        world.chain_id,
        0,
        0,
        1,
        world.block.gas_limit + 1,
        Some([0x11; 20]),
        [0u8; 32],
        &[],
        &[],
    );
    assert!(apply_raw_transaction(&mut world, &over)
        .unwrap_err()
        .to_string()
        .contains("block gas"));
    let bad = vec![0x02, 0x01];
    assert!(parse_raw_transaction(&bad).is_err());
    let raw = sign_eip1559(
        &key,
        world.chain_id,
        0,
        0,
        1,
        21_000,
        Some([0x11; 20]),
        [0u8; 32],
        &[],
        &[],
    );
    apply_raw_transaction(&mut world, &raw).unwrap();
    assert!(apply_raw_transaction(&mut world, &raw)
        .unwrap_err()
        .to_string()
        .contains("nonce"));
}

#[test]
fn shanghai_precompiles_are_the_pinned_engine() {
    use sha2::Digest;

    let (world, key, caller) = funded();
    let sha = eth_call(
        &world,
        caller,
        Some(precompile(2)),
        [0u8; 32],
        b"hi",
        100_000,
    )
    .unwrap();
    let mut expected_sha = [0u8; 32];
    expected_sha.copy_from_slice(&sha2::Sha256::digest(b"hi"));
    assert_eq!(sha, expected_sha);
    let ident = eth_call(
        &world,
        caller,
        Some(precompile(4)),
        [0u8; 32],
        b"hi",
        100_000,
    )
    .unwrap();
    assert_eq!(ident, b"hi");
    let ripemd = eth_call(
        &world,
        caller,
        Some(precompile(3)),
        [0u8; 32],
        b"hi",
        100_000,
    )
    .unwrap();
    assert_eq!(
        hex::encode(ripemd),
        "000000000000000000000000242485ab6bfd3502bcb3442ea2e211687b8e4d89"
    );
    let mut modexp = vec![0u8; 96];
    modexp[31] = 1;
    modexp[63] = 1;
    modexp[95] = 1;
    modexp.extend_from_slice(&[2, 3, 5]);
    let powered = eth_call(
        &world,
        caller,
        Some(precompile(5)),
        [0u8; 32],
        &modexp,
        100_000,
    )
    .unwrap();
    assert_eq!(powered, vec![3]);
    let (sig, recid) = key
        .sign_prehash_recoverable(keccak256(b"ovl").as_slice())
        .unwrap();
    let mut recover = [0u8; 128];
    recover[..32].copy_from_slice(keccak256(b"ovl").as_slice());
    recover[63] = 27 + recid.to_byte();
    recover[64..].copy_from_slice(&sig.to_bytes());
    let got = eth_call(
        &world,
        caller,
        Some(precompile(1)),
        [0u8; 32],
        &recover,
        100_000,
    )
    .unwrap();
    assert_eq!(&got[12..], &caller);
    let pairing = eth_call(&world, caller, Some(precompile(8)), [0u8; 32], &[], 200_000).unwrap();
    assert_eq!(pairing.last().copied(), Some(1));
}

fn precompile(id: u8) -> [u8; 20] {
    let mut address = [0u8; 20];
    address[19] = id;
    address
}

#[test]
fn measured_transfer_batch() {
    let (mut world, key, _) = funded();
    let started = Instant::now();
    let count = 30u64;
    for nonce in 0..count {
        let raw = sign_eip1559(
            &key,
            world.chain_id,
            nonce,
            0,
            1,
            21_000,
            Some([0x99; 20]),
            value_word(1),
            &[],
            &[],
        );
        apply_raw_transaction(&mut world, &raw).unwrap();
    }
    let elapsed = started.elapsed();
    eprintln!(
        "ovl-evm-perf transfers={count} elapsed_us={} gas_used={}",
        elapsed.as_micros(),
        world.block.gas_used
    );
    assert_eq!(world.block.gas_used, 21_000 * count);
    world.check_supply().unwrap();
}
