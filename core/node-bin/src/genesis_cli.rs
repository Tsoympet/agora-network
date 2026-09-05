//! Offline genesis inspection commands.

use std::path::PathBuf;

use agora_state_machine::{
    ChainParams, GenesisArtifact, NetworkId, TridentGenesisArtifact, TESTNET_GENESIS_HASH_HEX,
};

fn usage() -> ! {
    eprintln!(
        "Usage:
  agora-node genesis dump [--network testnet|dev] [--out PATH]
  agora-node genesis verify [--network testnet] [--file PATH]
  agora-node genesis trident verify --file PATH --mode draft|freeze-ready

Defaults: network=testnet, dump writes docs/genesis/<network>.genesis.json when --out omitted
          and CWD is the repo root; otherwise stdout.

The Trident command is offline-only and cannot boot or freeze a v3 artifact."
    );
    std::process::exit(2);
}

pub fn run(mut args: impl Iterator<Item = String>) -> ! {
    let cmd = args.next().unwrap_or_else(|| usage());
    match cmd.as_str() {
        "dump" => dump(args),
        "verify" => verify(args),
        "trident" => trident(args),
        "help" | "-h" | "--help" => usage(),
        other => {
            eprintln!("unknown genesis subcommand: {other}");
            usage();
        }
    }
}

#[derive(Clone, Copy)]
enum TridentValidationMode {
    Draft,
    FreezeReady,
}

fn trident(mut args: impl Iterator<Item = String>) -> ! {
    match args.next().as_deref() {
        Some("verify") => trident_verify(args),
        Some(other) => {
            eprintln!("unknown Trident genesis subcommand: {other}");
            usage();
        }
        None => usage(),
    }
}

fn trident_verify(mut args: impl Iterator<Item = String>) -> ! {
    let mut file = None;
    let mut mode = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--file" | "-f" => {
                file = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            "--mode" => {
                let value = args.next().unwrap_or_else(|| usage());
                mode = Some(match value.as_str() {
                    "draft" => TridentValidationMode::Draft,
                    "freeze-ready" => TridentValidationMode::FreezeReady,
                    _ => {
                        eprintln!("invalid Trident verification mode: {value}");
                        usage();
                    }
                });
            }
            other => {
                eprintln!("unknown Trident verification flag: {other}");
                usage();
            }
        }
    }

    let file = file.unwrap_or_else(|| {
        eprintln!("Trident verification requires --file");
        usage();
    });
    let mode = mode.unwrap_or_else(|| {
        eprintln!("Trident verification requires explicit --mode draft|freeze-ready");
        usage();
    });
    let raw = match std::fs::read_to_string(&file) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("read {}: {error}", file.display());
            std::process::exit(1);
        }
    };
    let artifact = match TridentGenesisArtifact::from_json(&raw) {
        Ok(artifact) => artifact,
        Err(error) => {
            eprintln!("FAIL: parse Trident artifact: {error}");
            std::process::exit(1);
        }
    };
    let result = match mode {
        TridentValidationMode::Draft => artifact.validate_draft(),
        TridentValidationMode::FreezeReady => artifact.validate_freeze_ready(),
    };
    if let Err(error) = result {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }

    println!("network: {}", artifact.network);
    println!(
        "computed_genesis_identity: {}",
        artifact.consensus_identity_hash().to_hex()
    );
    println!(
        "computed_network_fingerprint: {}",
        artifact.compute_network_fingerprint().to_hex()
    );
    match mode {
        TridentValidationMode::Draft => {
            println!("DRAFT VALID: offline structure checks passed; NOT FREEZE-READY");
        }
        TridentValidationMode::FreezeReady => {
            println!(
                "FREEZE-READY CHECKS PASSED: this command did not freeze or boot the artifact"
            );
        }
    }
    std::process::exit(0);
}

fn parse_flags(mut args: impl Iterator<Item = String>) -> (NetworkId, Option<PathBuf>) {
    let mut network = NetworkId::Testnet;
    let mut out: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--network" | "-n" => {
                let v = args.next().unwrap_or_else(|| usage());
                network = NetworkId::parse(&v).unwrap_or_else(|| {
                    eprintln!("invalid network: {v}");
                    usage();
                });
            }
            "--out" | "-o" | "--file" | "-f" => {
                out = Some(PathBuf::from(args.next().unwrap_or_else(|| usage())));
            }
            other => {
                eprintln!("unknown flag: {other}");
                usage();
            }
        }
    }
    (network, out)
}

fn params_for(network: NetworkId) -> ChainParams {
    match ChainParams::for_network(network) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}

fn dump(args: impl Iterator<Item = String>) -> ! {
    let (network, out) = parse_flags(args);
    let params = params_for(network);
    let artifact = GenesisArtifact::from_params(&params);
    let json = match artifact.to_json_pretty() {
        Ok(s) => format!("{s}\n"),
        Err(e) => {
            eprintln!("serialize failed: {e}");
            std::process::exit(1);
        }
    };

    let path = out.or_else(|| {
        let candidate = PathBuf::from(format!("docs/genesis/{}.genesis.json", network.as_str()));
        if PathBuf::from("docs/genesis").is_dir() {
            Some(candidate)
        } else {
            None
        }
    });

    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, &json) {
            eprintln!("write {}: {e}", path.display());
            std::process::exit(1);
        }
        println!(
            "wrote {} (genesis {})",
            path.display(),
            artifact.genesis_hash
        );
    } else {
        print!("{json}");
    }
    std::process::exit(0);
}

fn verify(args: impl Iterator<Item = String>) -> ! {
    let (network, file) = parse_flags(args);
    let params = params_for(network);
    let computed = params.compute_genesis_hash();

    if let Some(path) = file {
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("read {}: {e}", path.display());
                std::process::exit(1);
            }
        };
        let artifact = match GenesisArtifact::from_json(&raw) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("parse artifact: {e}");
                std::process::exit(1);
            }
        };
        match artifact.to_params() {
            Ok(from_file) => {
                let h = from_file.compute_genesis_hash();
                if h != computed {
                    eprintln!(
                        "FAIL: file genesis {} != network {} genesis {}",
                        h.to_hex(),
                        network,
                        computed.to_hex()
                    );
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("FAIL: {e}");
                std::process::exit(1);
            }
        }
        println!(
            "OK {} genesis {} matches {}",
            network,
            computed.to_hex(),
            path.display()
        );
    } else if network == NetworkId::Testnet {
        if computed.to_hex() != TESTNET_GENESIS_HASH_HEX {
            eprintln!(
                "FAIL: recomputed {} != TESTNET_GENESIS_HASH_HEX {}",
                computed.to_hex(),
                TESTNET_GENESIS_HASH_HEX
            );
            std::process::exit(1);
        }
        println!(
            "OK testnet genesis {} matches embedded constant",
            computed.to_hex()
        );
    } else {
        println!("OK {} genesis {}", network, computed.to_hex());
    }
    std::process::exit(0);
}
