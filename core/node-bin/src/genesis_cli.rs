//! `agora-node genesis dump|verify` — freeze and check canonical Block 0.

use std::path::PathBuf;

use agora_state_machine::{ChainParams, GenesisArtifact, NetworkId, TESTNET_GENESIS_HASH_HEX};

fn usage() -> ! {
    eprintln!(
        "Usage:
  agora-node genesis dump [--network testnet|dev] [--out PATH]
  agora-node genesis verify [--network testnet] [--file PATH]

Defaults: network=testnet, dump writes docs/genesis/<network>.genesis.json when --out omitted
          and CWD is the repo root; otherwise stdout."
    );
    std::process::exit(2);
}

pub fn run(mut args: impl Iterator<Item = String>) -> ! {
    let cmd = args.next().unwrap_or_else(|| usage());
    match cmd.as_str() {
        "dump" => dump(args),
        "verify" => verify(args),
        "help" | "-h" | "--help" => usage(),
        other => {
            eprintln!("unknown genesis subcommand: {other}");
            usage();
        }
    }
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
