//! Offline historical-layer snapshot exporter and verifier.

use std::fs;
use std::path::{Path, PathBuf};

use agora_layers_runtime::{
    export_migration_snapshot, load_and_verify_migration_snapshot, LayersCheckpoint,
    MigrationSnapshot,
};

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("agora-trident-migration: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(usage());
    };
    match command {
        "export" => {
            let checkpoint_dir = required_path(&args, "--checkpoint-dir")?;
            let output = required_path(&args, "--output")?;
            let checkpoint = LayersCheckpoint::load(&checkpoint_dir)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    format!(
                        "no layers-checkpoint.json under {}",
                        checkpoint_dir.display()
                    )
                })?;
            let snapshot =
                export_migration_snapshot(&checkpoint).map_err(|error| error.to_string())?;
            write_snapshot(&output, &snapshot)?;
            print_summary(&output, &snapshot);
            Ok(())
        }
        "verify" => {
            let snapshot_path = required_path(&args, "--snapshot")?;
            let snapshot = load_and_verify_migration_snapshot(&snapshot_path)
                .map_err(|error| error.to_string())?;
            print_summary(&snapshot_path, &snapshot);
            if args.iter().any(|arg| arg == "--require-ready")
                && !snapshot.body.audit.ready_for_claim_design
            {
                return Err(format!(
                    "snapshot is valid but has {} unresolved blocker(s)",
                    snapshot.body.audit.blockers.len()
                ));
            }
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn required_path(args: &[String], flag: &str) -> Result<PathBuf, String> {
    let index = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| format!("missing {flag}\n{}", usage()))?;
    args.get(index + 1)
        .filter(|value| !value.starts_with("--"))
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn write_snapshot(path: &Path, snapshot: &MigrationSnapshot) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("create output directory: {error}"))?;
    let bytes =
        serde_json::to_vec_pretty(snapshot).map_err(|error| format!("encode snapshot: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(|error| format!("write snapshot: {error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("commit snapshot: {error}"))
}

fn print_summary(path: &Path, snapshot: &MigrationSnapshot) {
    println!("snapshot={}", path.display());
    println!("snapshot_root={}", snapshot.snapshot_root);
    println!("claim_root={}", snapshot.body.claim_root);
    println!(
        "OVL minted={} ledger={} proposed_claims={} retired_or_burned={}",
        snapshot.body.audit.ovl.source_minted,
        snapshot.body.audit.ovl.source_ledger,
        snapshot.body.audit.ovl.proposed_claims,
        snapshot.body.audit.ovl.retired_or_burned
    );
    println!(
        "DRC minted={} ledger={} proposed_claims={} retired_or_burned={}",
        snapshot.body.audit.drc.source_minted,
        snapshot.body.audit.drc.source_ledger,
        snapshot.body.audit.drc.proposed_claims,
        snapshot.body.audit.drc.retired_or_burned
    );
    println!(
        "ready_for_claim_design={}",
        snapshot.body.audit.ready_for_claim_design
    );
    for blocker in &snapshot.body.audit.blockers {
        println!("blocker={blocker}");
    }
}

fn usage() -> String {
    "usage:\n  agora-trident-migration export --checkpoint-dir <dir> --output <snapshot.json>\n  agora-trident-migration verify --snapshot <snapshot.json> [--require-ready]".into()
}
