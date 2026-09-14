"""Owned SSH tunnel lifecycle for local VPS harness forwarding."""
from __future__ import annotations

import fcntl
import os
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import IO


@dataclass
class TunnelHandle:
    process: subprocess.Popen[bytes]
    local_port: int
    pid: int
    _lease: IO[bytes]
    _control_path: str
    _control_dir: str


def _acquire_port_lease(local_port: int) -> IO[bytes]:
    path = os.path.join(tempfile.gettempdir(), f"x0x-e2e-tunnel-{local_port}.lock")
    lease = open(path, "a+b")  # retained for the tunnel lifetime
    try:
        fcntl.flock(lease.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            probe.bind(("127.0.0.1", local_port))
            probe.listen(1)
    except (BlockingIOError, OSError) as error:
        lease.close()
        raise RuntimeError(f"local tunnel port {local_port} is already in use") from error
    return lease


def _child_error(proc: subprocess.Popen[bytes]) -> str:
    if proc.stderr is None:
        return "no stderr"
    return proc.stderr.read().decode("utf-8", errors="replace").strip() or "no stderr"


def _new_control_path() -> tuple[str, str]:
    """Create a private short directory with room for OpenSSH socket suffixes."""
    control_dir = tempfile.mkdtemp(prefix="x0x-", dir="/tmp")
    return control_dir, os.path.join(control_dir, "c")


def _remove_control_dir(control_dir: str) -> None:
    shutil.rmtree(control_dir, ignore_errors=True)


def _stop_child(proc: subprocess.Popen[bytes]) -> None:
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
    if proc.stderr is not None:
        proc.stderr.close()


def start_ssh_tunnel(
    ip: str,
    local_port: int,
    remote_port: int = 13600,
    *,
    readiness_timeout: float = 15.0,
    poll_interval: float = 0.1,
) -> TunnelHandle:
    """Start a private SSH master and obtain its owned forwarding acknowledgement."""
    ssh = shutil.which("ssh")
    if ssh is None:
        raise RuntimeError("ssh not on PATH")
    lease = _acquire_port_lease(local_port)
    control_dir, control_path = _new_control_path()
    target = f"root@{ip}"
    proc: subprocess.Popen[bytes] | None = None
    try:
        proc = subprocess.Popen(
            [
                ssh, "-N", "-M", "-S", control_path,
                "-o", "ControlPersist=no",
                "-o", "ConnectTimeout=10",
                "-o", "BatchMode=yes",
                "-o", "ServerAliveInterval=30",
                target,
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        deadline = time.monotonic() + readiness_timeout
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f"ssh master exited before readiness: {_child_error(proc)}")
            check = subprocess.run(
                [ssh, "-S", control_path, "-O", "check", target],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=2,
                check=False,
            )
            if check.returncode == 0:
                break
            time.sleep(poll_interval)
        else:
            raise RuntimeError(f"ssh master to {ip} not ready in {readiness_timeout:g}s")

        forward = f"127.0.0.1:{local_port}:127.0.0.1:{remote_port}"
        acknowledged = subprocess.run(
            [ssh, "-S", control_path, "-O", "forward", "-L", forward, target],
            capture_output=True,
            timeout=max(2.0, readiness_timeout),
            check=False,
        )
        if acknowledged.returncode != 0:
            detail = acknowledged.stderr.decode("utf-8", errors="replace").strip()
            raise RuntimeError(f"ssh did not establish owned forward on {local_port}: {detail}")

        while time.monotonic() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f"ssh master exited after forward acknowledgement: {_child_error(proc)}")
            try:
                with urllib.request.urlopen(
                    f"http://127.0.0.1:{local_port}/health", timeout=2
                ) as response:
                    if response.status in (200, 401):
                        return TunnelHandle(
                            proc, local_port, proc.pid, lease, control_path, control_dir
                        )
            except urllib.error.HTTPError as error:
                if error.code == 401:
                    return TunnelHandle(proc, local_port, proc.pid, lease, control_path)
            except (OSError, urllib.error.URLError):
                pass
            time.sleep(poll_interval)
        raise RuntimeError(
            f"owned ssh tunnel to {ip}:{remote_port} not healthy in {readiness_timeout:g}s"
        )
    except Exception:
        if proc is not None:
            _stop_child(proc)
        lease.close()
        _remove_control_dir(control_dir)
        raise


def stop_ssh_tunnel(tunnel: TunnelHandle) -> None:
    """Stop only this invocation's private control master and release its lease."""
    try:
        _stop_child(tunnel.process)
    finally:
        tunnel._lease.close()
        _remove_control_dir(tunnel._control_dir)
