//! Operator datadir schema inspection and supported rebuilds.
//!
//! This does not freeze genesis, boot a network, or rewrite historical block
//! bytes. Supported migrations are the library helpers already in
//! `agora-state-machine`.

use std::path::PathBuf;

use agora_state_machine::{
    load_schema_version, migrate_drc_fee_burn_schema, migrate_drc_ledger_object_index_schema,
    put_schema_version_into, reindex_drc_ledger_objects, verify_drc_ledger_object_index,
    verify_supply_invariants, StateError, StateStore, WriteBatch, DRC_FEE_BURN_SCHEMA_VERSION,
    DRC_LEDGER_INDEX_DATADIR_SCHEMA, SCHEMA_VERSION,
};
use agora_types::Hash;

fn usage() -> ! {
    eprintln!(
        "Usage:
  agora-node schema report --data PATH
  agora-node schema migrate --data PATH [--applied-order FILE]
  agora-node schema reindex --data PATH

This CLI reports the datadir schema and runs supported library rebuilds.
It does not freeze genesis, invent receipts, or claim public-testnet readiness.

Supported rebuilds:
  19 → 20  DRC fee-burn counters (zero start; no historical fee inference)
  20 → 21  DRC ledger-object index (requires --applied-order)
  21/22    verify + optional reindex of the derivable object mirror
  21 → 22  marker-only when supply invariants already hold (no EVM rewrite)"
    );
    std::process::exit(2);
}

pub fn run(mut args: impl Iterator<Item = String>) -> ! {
    let cmd = args.next().unwrap_or_else(|| usage());
    match cmd.as_str() {
        "report" => report_cmd(args),
        "migrate" => migrate_cmd(args),
        "reindex" => reindex_cmd(args),
        "help" | "-h" | "--help" => usage(),
        other => {
            eprintln!("unknown schema subcommand: {other}");
            usage();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaReport {
    pub code_schema: u32,
    pub datadir_schema: u32,
    pub current: bool,
    pub future: bool,
    pub supported_rebuilds: Vec<&'static str>,
}

pub fn inspect_schema(store: &StateStore) -> Result<SchemaReport, StateError> {
    let datadir_schema = load_schema_version(store)?;
    let mut supported_rebuilds = Vec::new();
    if datadir_schema == DRC_FEE_BURN_SCHEMA_VERSION - 1 {
        supported_rebuilds.push("migrate 19→20 fee-burn counters");
    }
    if datadir_schema == DRC_LEDGER_INDEX_DATADIR_SCHEMA - 1 {
        supported_rebuilds.push("migrate 20→21 ledger-object index (applied order required)");
    }
    if datadir_schema == DRC_LEDGER_INDEX_DATADIR_SCHEMA && datadir_schema < SCHEMA_VERSION {
        supported_rebuilds.push("migrate 21→22 schema marker (no EVM rewrite, no genesis freeze)");
    }
    if (DRC_LEDGER_INDEX_DATADIR_SCHEMA..=SCHEMA_VERSION).contains(&datadir_schema) {
        supported_rebuilds.push("reindex DRC live-object mirror");
    }
    Ok(SchemaReport {
        code_schema: SCHEMA_VERSION,
        datadir_schema,
        current: datadir_schema == SCHEMA_VERSION,
        future: datadir_schema > SCHEMA_VERSION,
        supported_rebuilds,
    })
}

pub fn migrate_supported(
    store: &StateStore,
    applied_blocks: Option<&[(Hash, u64)]>,
) -> Result<u32, StateError> {
    let mut schema = load_schema_version(store)?;
    if schema > SCHEMA_VERSION {
        return Err(StateError::Storage(format!(
            "unsupported future datadir schema {schema}"
        )));
    }
    if schema == DRC_FEE_BURN_SCHEMA_VERSION - 1 {
        migrate_drc_fee_burn_schema(store)?;
        schema = load_schema_version(store)?;
    }
    if schema == DRC_LEDGER_INDEX_DATADIR_SCHEMA - 1 {
        let order = applied_blocks.ok_or_else(|| {
            StateError::Storage(
                "schema 20→21 requires canonical applied-blue order (--applied-order)".into(),
            )
        })?;
        migrate_drc_ledger_object_index_schema(store, order)?;
        schema = load_schema_version(store)?;
    }
    if schema == DRC_LEDGER_INDEX_DATADIR_SCHEMA && schema < SCHEMA_VERSION {
        verify_supply_invariants(store)?;
        verify_drc_ledger_object_index(store)?;
        // Marker only. OVL-EVM world stays absent until a later explicit commit.
        let mut batch = WriteBatch::new();
        put_schema_version_into(&mut batch, SCHEMA_VERSION);
        store.write_batch(batch)?;
        schema = load_schema_version(store)?;
    }
    if schema == SCHEMA_VERSION {
        verify_supply_invariants(store)?;
        if schema >= DRC_LEDGER_INDEX_DATADIR_SCHEMA {
            verify_drc_ledger_object_index(store)?;
        }
        return Ok(schema);
    }
    Err(StateError::Storage(format!(
        "no supported migration from schema {schema} to {SCHEMA_VERSION}"
    )))
}

pub fn reindex_supported(store: &StateStore) -> Result<(), StateError> {
    let schema = load_schema_version(store)?;
    if !(DRC_LEDGER_INDEX_DATADIR_SCHEMA..=SCHEMA_VERSION).contains(&schema) {
        return Err(StateError::Storage(format!(
            "DRC object reindex requires schema {DRC_LEDGER_INDEX_DATADIR_SCHEMA}..={SCHEMA_VERSION}, found {schema}"
        )));
    }
    reindex_drc_ledger_objects(store)
}

fn open_data(path: PathBuf) -> StateStore {
    match StateStore::open(&path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("open {}: {error}", path.display());
            std::process::exit(1);
        }
    }
}

fn parse_data_flags(mut args: impl Iterator<Item = String>) -> (PathBuf, Option<PathBuf>) {
    let mut data = None;
    let mut applied = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data" | "-d" => {
                data = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            "--applied-order" => {
                applied = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            other => {
                eprintln!("unknown schema flag: {other}");
                usage();
            }
        }
    }
    let data = data.unwrap_or_else(|| {
        eprintln!("schema commands require --data PATH");
        usage();
    });
    (data, applied)
}

fn parse_applied_order(path: &PathBuf) -> Vec<(Hash, u64)> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("read {}: {error}", path.display());
            std::process::exit(1);
        }
    };
    let mut out = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next().and_then(Hash::from_hex) else {
            eprintln!("applied-order line {}: missing 32-byte hex hash", index + 1);
            std::process::exit(1);
        };
        let score = parts
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        out.push((hash, score));
    }
    out
}

fn report_cmd(args: impl Iterator<Item = String>) -> ! {
    let (data, _) = parse_data_flags(args);
    let store = open_data(data);
    match inspect_schema(&store) {
        Ok(report) => {
            println!("code_schema: {}", report.code_schema);
            println!("datadir_schema: {}", report.datadir_schema);
            println!("current: {}", report.current);
            println!("future: {}", report.future);
            if report.supported_rebuilds.is_empty() {
                println!("supported_rebuilds: none");
            } else {
                println!("supported_rebuilds:");
                for item in report.supported_rebuilds {
                    println!("  - {item}");
                }
            }
            println!("genesis_freeze: not claimed");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("FAIL: {error}");
            std::process::exit(1);
        }
    }
}

fn migrate_cmd(args: impl Iterator<Item = String>) -> ! {
    let (data, applied_path) = parse_data_flags(args);
    let store = open_data(data);
    let applied = applied_path.as_ref().map(parse_applied_order);
    match migrate_supported(&store, applied.as_deref()) {
        Ok(schema) => {
            println!("migrated_schema: {schema}");
            println!("genesis_freeze: not claimed");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("FAIL: {error}");
            std::process::exit(1);
        }
    }
}

fn reindex_cmd(args: impl Iterator<Item = String>) -> ! {
    let (data, _) = parse_data_flags(args);
    let store = open_data(data);
    match reindex_supported(&store) {
        Ok(()) => {
            println!("reindex: ok");
            println!("genesis_freeze: not claimed");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("FAIL: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agora_state_machine::{put_burned_supply_into, DRC_FEE_BURN_SCHEMA_VERSION};
    use agora_types::NativeAssetId;

    fn store_with_schema(schema: u32) -> StateStore {
        let store = StateStore::open_in_memory();
        let mut batch = WriteBatch::new();
        put_schema_version_into(&mut batch, schema);
        if schema >= DRC_FEE_BURN_SCHEMA_VERSION {
            for asset in NativeAssetId::ALL {
                put_burned_supply_into(&mut batch, asset, 0);
            }
        }
        store.write_batch(batch).unwrap();
        store
    }

    #[test]
    fn report_names_supported_rebuilds_without_claiming_current() {
        let store = store_with_schema(DRC_FEE_BURN_SCHEMA_VERSION - 1);
        let report = inspect_schema(&store).unwrap();
        assert_eq!(report.datadir_schema, 19);
        assert_eq!(report.code_schema, SCHEMA_VERSION);
        assert!(!report.current);
        assert!(report
            .supported_rebuilds
            .iter()
            .any(|item| item.contains("19→20")));
    }

    #[test]
    fn migrate_19_to_20_then_refuses_20_without_applied_order() {
        let store = store_with_schema(DRC_FEE_BURN_SCHEMA_VERSION - 1);
        let err = migrate_supported(&store, None).unwrap_err();
        assert!(err.to_string().contains("applied-blue order"));
        assert_eq!(
            load_schema_version(&store).unwrap(),
            DRC_LEDGER_INDEX_DATADIR_SCHEMA - 1
        );
    }

    #[test]
    fn current_schema_report_and_reindex_are_idempotent() {
        let store = store_with_schema(SCHEMA_VERSION);
        let report = inspect_schema(&store).unwrap();
        assert!(report.current);
        migrate_supported(&store, None).unwrap();
        reindex_supported(&store).unwrap();
        assert_eq!(load_schema_version(&store).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn future_schema_fails_closed() {
        let store = store_with_schema(SCHEMA_VERSION + 1);
        let report = inspect_schema(&store).unwrap();
        assert!(report.future);
        assert!(migrate_supported(&store, None).is_err());
        assert!(reindex_supported(&store).is_err());
    }
}
