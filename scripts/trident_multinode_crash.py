#!/usr/bin/env python3
"""Bounded multi-node convergence and hard-restart smoke test.

The harness only runs prebuilt Agora binaries. Every child has a recorded PID,
unique loopback ports, and an isolated temporary datadir.
"""

from __future__ import annotations

import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TIMEOUT = 90.0


def reserve_ports(count: int) -> list[int]:
    sockets: list[socket.socket] = []
    try:
        for _ in range(count):
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.bind(("127.0.0.1", 0))
            sockets.append(sock)
        return [int(sock.getsockname()[1]) for sock in sockets]
    finally:
        for sock in sockets:
            sock.close()


def http_json(url: str, method: str, params: Any = None) -> dict[str, Any]:
    body = json.dumps(
        {"id": 1, "method": method, "params": [] if params is None else params}
    ).encode()
    request = urllib.request.Request(
        url,
        data=body,
        headers={"content-type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=2) as response:
        decoded = json.loads(response.read())
    if decoded.get("error"):
        raise RpcError(str(decoded["error"].get("message", decoded["error"])))
    if "result" not in decoded:
        raise RpcError(f"{method} response omitted result")
    return decoded


def health(url: str) -> bool:
    try:
        with urllib.request.urlopen(url, timeout=1) as response:
            return response.status == 200 and b"ok" in response.read().lower()
    except (OSError, urllib.error.URLError):
        return False


class RpcError(RuntimeError):
    pass


@dataclass
class Child:
    name: str
    process: subprocess.Popen[bytes]
    log_path: Path
    log_file: Any

    @property
    def pid(self) -> int:
        return self.process.pid

    def stop(self, hard: bool = False) -> None:
        if self.process.poll() is not None:
            return
        signum = signal.SIGKILL if hard else signal.SIGTERM
        os.kill(self.pid, signum)
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.kill(self.pid, signal.SIGKILL)
            self.process.wait(timeout=5)

    def close(self) -> None:
        self.log_file.close()


class Harness:
    def __init__(self, args: argparse.Namespace, work: Path) -> None:
        self.args = args
        self.work = work
        self.deadline = time.monotonic() + args.timeout
        self.children: dict[str, Child] = {}
        (
            self.seeder_port,
            self.rpc_a_port,
            self.rpc_b_port,
            self.p2p_a_port,
            self.p2p_b_port,
        ) = reserve_ports(5)
        self.rpc_a = f"http://127.0.0.1:{self.rpc_a_port}/rpc"
        self.rpc_b = f"http://127.0.0.1:{self.rpc_b_port}/rpc"
        self.seeder = f"http://127.0.0.1:{self.seeder_port}"

    def remaining(self) -> float:
        return max(0.0, self.deadline - time.monotonic())

    def spawn(self, name: str, command: list[str], env: dict[str, str]) -> Child:
        previous = self.children.pop(name, None)
        if previous is not None:
            previous.close()
        log_path = self.work / f"{name}.log"
        log_file = log_path.open("ab")
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env={**os.environ, **env},
            stdin=subprocess.DEVNULL,
            stdout=log_file,
            stderr=subprocess.STDOUT,
        )
        child = Child(name, process, log_path, log_file)
        self.children[name] = child
        print(f"PROCESS name={name} pid={child.pid} log={log_path}", flush=True)
        return child

    def wait_for(self, label: str, predicate: Any, interval: float = 0.25) -> Any:
        last_error: Exception | None = None
        while self.remaining() > 0:
            self.assert_children_alive()
            try:
                value = predicate()
                if value:
                    print(f"ASSERT {label}=PASS", flush=True)
                    return value
            except (OSError, urllib.error.URLError, RpcError, ValueError) as error:
                last_error = error
            time.sleep(interval)
        detail = f": {last_error}" if last_error else ""
        raise RuntimeError(f"timeout waiting for {label}{detail}")

    def assert_children_alive(self) -> None:
        for child in self.children.values():
            code = child.process.poll()
            if code is not None:
                raise RuntimeError(f"{child.name} exited unexpectedly with {code}")

    def node_env(self, node: str) -> dict[str, str]:
        is_a = node == "node-a"
        return {
            "AGORA_NETWORK": "dev",
            "AGORA_DATA": str(self.work / node),
            "AGORA_LISTEN": (
                f"/ip4/127.0.0.1/tcp/{self.p2p_a_port if is_a else self.p2p_b_port}"
            ),
            "AGORA_RPC_BIND": (
                f"127.0.0.1:{self.rpc_a_port if is_a else self.rpc_b_port}"
            ),
            "AGORA_DNS_SEEDER": self.seeder,
            "AGORA_SEEDER_REFRESH_SECS": "1",
            "AGORA_TEMPLATE_BITS": "0",
            "AGORA_GENESIS_BITS": "0",
            "AGORA_GENESIS_TIMESTAMP_MS": "1700000000000",
            "AGORA_POW_ALGO": "randomx",
            "AGORA_ARCHIVAL": "1",
            "RUST_LOG": "info",
        }

    def start_node(self, name: str) -> Child:
        return self.spawn(name, [str(self.args.node_bin)], self.node_env(name))

    def node_info(self, rpc: str) -> dict[str, Any]:
        return dict(http_json(rpc, "agora_getNodeInfo")["result"])

    def tips(self, rpc: str) -> tuple[str, ...]:
        result = http_json(rpc, "agora_getDagTips")["result"]
        if not isinstance(result, list) or not result:
            raise RpcError("tip set is empty")
        return tuple(sorted(str(item) for item in result))

    def wait_healthy(self, name: str, rpc: str) -> None:
        self.wait_for(f"{name}_health", lambda: health(rpc.removesuffix("/rpc") + "/health"))

    def wait_connected(self) -> None:
        def connected() -> bool:
            return (
                int(self.node_info(self.rpc_a).get("connected_peers") or 0) >= 1
                and int(self.node_info(self.rpc_b).get("connected_peers") or 0) >= 1
            )

        self.wait_for("bidirectional_peer_connection", connected)

    def wait_converged(self, label: str) -> tuple[str, ...]:
        def converged() -> tuple[str, ...] | None:
            tips_a = self.tips(self.rpc_a)
            tips_b = self.tips(self.rpc_b)
            return tips_a if tips_a == tips_b else None

        return self.wait_for(label, converged)

    def mine(self, blocks: int) -> None:
        child = self.spawn(
            f"miner-{time.monotonic_ns()}",
            [str(self.args.miner_bin)],
            {
                "AGORA_RPC_URL": self.rpc_a,
                "AGORA_MINE_MAX_BLOCKS": str(blocks),
                "AGORA_MINE_POLL_MS": "20",
                "RUST_LOG": "info",
            },
        )
        try:
            code = child.process.wait(timeout=min(self.remaining(), 30))
        except subprocess.TimeoutExpired as error:
            raise RuntimeError(f"miner timed out after requesting {blocks} blocks") from error
        if code != 0:
            raise RuntimeError(f"miner exited with {code}")
        child.close()
        self.children.pop(child.name, None)
        print(f"ASSERT mined_blocks={blocks}=PASS", flush=True)

    def assert_trident_surface(self, tip: str) -> None:
        try:
            result = http_json(self.rpc_a, "agora_getFinality", {"hash": tip})["result"]
        except RpcError as error:
            if not self.args.allow_pre_trident:
                raise RuntimeError(f"Trident finality RPC required: {error}") from error
            print("ASSERT trident_finality_surface=SKIP(pre-Trident branch)", flush=True)
            return
        required = {"block_hash", "state", "pow_work_met", "finalized"}
        missing = sorted(required - set(result))
        if missing:
            raise RuntimeError(f"finality response missing fields: {missing}")
        if str(result["block_hash"]) != tip:
            raise RuntimeError("finality response is not bound to the queried tip")
        print(
            "ASSERT trident_finality_surface=PASS "
            f"state={result['state']} finalized={result['finalized']}",
            flush=True,
        )

    def run(self) -> None:
        print(
            "SUITE trident-multinode-crash "
            f"timeout={self.args.timeout:.0f}s workdir={self.work}",
            flush=True,
        )
        self.spawn(
            "seeder",
            [str(self.args.seeder_bin)],
            {
                "AGORA_SEEDER_BIND": f"127.0.0.1:{self.seeder_port}",
                "RUST_LOG": "info",
            },
        )
        self.wait_for("seeder_health", lambda: health(self.seeder + "/health"))

        self.start_node("node-a")
        self.wait_healthy("node-a", self.rpc_a)
        self.start_node("node-b")
        self.wait_healthy("node-b", self.rpc_b)
        self.wait_connected()

        genesis_a = self.node_info(self.rpc_a).get("genesis_hash")
        genesis_b = self.node_info(self.rpc_b).get("genesis_hash")
        if not genesis_a or genesis_a != genesis_b:
            raise RuntimeError(f"genesis mismatch: A={genesis_a} B={genesis_b}")
        print(f"ASSERT shared_genesis=PASS hash={genesis_a}", flush=True)

        initial = self.wait_converged("initial_tip_convergence")
        self.mine(1)
        first_mined = self.wait_converged("gossip_tip_convergence")
        if first_mined == initial:
            raise RuntimeError("tip did not advance after mining")

        node_b = self.children["node-b"]
        crashed_pid = node_b.pid
        node_b.stop(hard=True)
        node_b.close()
        self.children.pop("node-b")
        print(f"CRASH node-b pid={crashed_pid} signal=SIGKILL", flush=True)

        self.mine(2)
        ahead = self.tips(self.rpc_a)
        if ahead == first_mined:
            raise RuntimeError("node-a did not advance while node-b was offline")
        print("ASSERT offline_tip_advance=PASS", flush=True)

        restarted = self.start_node("node-b")
        if restarted.pid == crashed_pid:
            raise RuntimeError("restart unexpectedly reused the crashed PID")
        self.wait_healthy("node-b_restart", self.rpc_b)
        self.wait_connected()
        recovered = self.wait_converged("restart_ibd_convergence")
        if recovered != ahead:
            raise RuntimeError("restarted node converged to an unexpected tip set")
        if self.node_info(self.rpc_b).get("genesis_hash") != genesis_a:
            raise RuntimeError("restarted node changed genesis")
        print(
            f"ASSERT crash_restart_recovery=PASS old_pid={crashed_pid} "
            f"new_pid={restarted.pid}",
            flush=True,
        )
        self.assert_trident_surface(recovered[0])
        print("SUITE_RESULT PASS", flush=True)

    def cleanup(self) -> None:
        for child in reversed(list(self.children.values())):
            try:
                child.stop()
            except (OSError, subprocess.TimeoutExpired):
                pass
            child.close()
        self.children.clear()

    def dump_failure_logs(self) -> None:
        for path in sorted(self.work.glob("*.log")):
            try:
                lines = path.read_text(errors="replace").splitlines()
            except OSError:
                continue
            print(f"--- {path.name} (last 40 lines) ---", file=sys.stderr)
            print("\n".join(lines[-40:]), file=sys.stderr)


def binary(path: str) -> Path:
    resolved = Path(path)
    if not resolved.is_absolute():
        resolved = ROOT / resolved
    if not resolved.is_file() or not os.access(resolved, os.X_OK):
        raise argparse.ArgumentTypeError(f"prebuilt executable not found: {resolved}")
    return resolved


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--node-bin", default="target/debug/agora-node")
    parser.add_argument("--miner-bin", default="target/debug/agora-miner")
    parser.add_argument(
        "--seeder-bin", default="target/debug/agora-dns-seeder"
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=float(os.environ.get("AGORA_CRASH_SUITE_TIMEOUT", DEFAULT_TIMEOUT)),
    )
    parser.add_argument(
        "--allow-pre-trident",
        action="store_true",
        help="permit the architecture-base node to lack agora_getFinality",
    )
    args = parser.parse_args()
    if args.timeout < 10:
        parser.error("--timeout must be at least 10 seconds")
    try:
        args.node_bin = binary(args.node_bin)
        args.miner_bin = binary(args.miner_bin)
        args.seeder_bin = binary(args.seeder_bin)
    except argparse.ArgumentTypeError as error:
        parser.error(str(error))
    return args


def main() -> int:
    args = parse_args()
    with tempfile.TemporaryDirectory(prefix="agora-trident-crash-") as directory:
        harness = Harness(args, Path(directory))
        try:
            harness.run()
            return 0
        except Exception as error:
            print(f"SUITE_RESULT FAIL error={error}", file=sys.stderr, flush=True)
            harness.dump_failure_logs()
            return 1
        finally:
            harness.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
