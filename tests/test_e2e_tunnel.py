#!/usr/bin/env python3
"""Deterministic ownership tests for the VPS harness SSH tunnel helper."""
from __future__ import annotations

import importlib.util
import os
import socket
import stat
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock


def load_tunnel_module():
    source = Path(__file__).with_name("e2e_tunnel.py")
    spec = importlib.util.spec_from_file_location("e2e_tunnel_under_test", source)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


FAKE_SSH = f"""#!{sys.executable}
import http.server
import os
from pathlib import Path
import signal
import sys
import time

args = sys.argv[1:]
if '-S' not in args:
    marker = os.environ.get('FAKE_SSH_STARTED_MARKER')
    if marker:
        Path(marker).touch()
    time.sleep(0.3)
    sys.exit(17)
control = Path(args[args.index('-S') + 1])
if '-O' in args:
    operation = args[args.index('-O') + 1]
    if operation == 'check':
        sys.exit(0 if control.exists() else 1)
    if operation == 'forward':
        spec = args[args.index('-L') + 1]
        control.with_suffix('.forward').write_text(spec)
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if control.with_suffix('.ready').exists():
                sys.exit(0)
            if control.with_suffix('.error').exists():
                print(control.with_suffix('.error').read_text(), file=sys.stderr)
                sys.exit(1)
            time.sleep(0.01)
        sys.exit(2)
    sys.exit(2)

marker = os.environ.get('FAKE_SSH_STARTED_MARKER')
if marker:
    Path(marker).touch()
control.touch()
forward = control.with_suffix('.forward')
while not forward.exists():
    time.sleep(0.01)
spec = forward.read_text()
port = int(spec.split(':')[1])
try:
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            self.send_response(200)
            self.end_headers()
        def log_message(self, *_):
            pass
    server = http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler)
except OSError as error:
    control.with_suffix('.error').write_text(str(error))
    time.sleep(0.2)
    sys.exit(17)
control.with_suffix('.ready').touch()
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
server.serve_forever()
"""



class TunnelOwnershipTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.tunnel = load_tunnel_module()

    def fake_path(self, source: str):
        directory = tempfile.TemporaryDirectory()
        ssh = Path(directory.name, "ssh")
        ssh.write_text(source)
        ssh.chmod(ssh.stat().st_mode | stat.S_IXUSR)
        return directory, mock.patch.dict(os.environ, {"PATH": directory.name})

    def test_generated_control_path_binds_as_real_unix_socket(self) -> None:
        control_dir, control_path = self.tunnel._new_control_path()
        self.addCleanup(self.tunnel._remove_control_dir, control_dir)
        self.assertEqual(stat.S_IMODE(os.stat(control_dir).st_mode), 0o700)
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as control:
            control.bind(control_path)
        os.unlink(control_path)

    def test_occupied_port_fails_before_spawning_or_reusing_listener(self) -> None:
        directory, path = self.fake_path(FAKE_SSH)
        self.addCleanup(directory.cleanup)
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            port = listener.getsockname()[1]
            with path, self.assertRaisesRegex(RuntimeError, "already in use"):
                self.tunnel.start_ssh_tunnel("example.invalid", port)

    def test_simultaneous_acquire_has_one_owner_and_one_bounded_failure(self) -> None:
        directory, path = self.fake_path(FAKE_SSH)
        self.addCleanup(directory.cleanup)
        port = free_port()
        barrier = threading.Barrier(2)
        handles = []
        errors = []

        def acquire() -> None:
            barrier.wait()
            try:
                handles.append(
                    self.tunnel.start_ssh_tunnel(
                        "example.invalid", port, readiness_timeout=3, poll_interval=0.02
                    )
                )
            except RuntimeError as error:
                errors.append(str(error))

        with path:
            workers = [threading.Thread(target=acquire) for _ in range(2)]
            for worker in workers:
                worker.start()
            for worker in workers:
                worker.join(timeout=5)
        self.addCleanup(lambda: [self.tunnel.stop_ssh_tunnel(h) for h in handles])
        self.assertEqual(len(handles), 1)
        self.assertEqual(len(errors), 1)
        self.assertIn("already in use", errors[0])

    def test_child_bind_failure_is_not_credited_from_unrelated_health(self) -> None:
        directory, path = self.fake_path(FAKE_SSH)
        self.addCleanup(directory.cleanup)
        port = free_port()
        ready_to_race = threading.Event()
        marker = Path(directory.name, "master-started")

        class HealthHandler(__import__("http.server").server.BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(200)
                self.end_headers()
            def log_message(self, *_):
                pass

        server = __import__("http.server").server.ThreadingHTTPServer(
            ("127.0.0.1", port), HealthHandler, bind_and_activate=False
        )

        def occupy_after_master_starts() -> None:
            for _ in range(300):
                if marker.exists():
                    break
                __import__("time").sleep(0.01)
            if marker.exists():
                server.server_bind()
                server.server_activate()
                threading.Thread(target=server.serve_forever, daemon=True).start()
                ready_to_race.set()
        racer = threading.Thread(target=occupy_after_master_starts)
        racer.start()

        with path, mock.patch.dict(os.environ, {"FAKE_SSH_STARTED_MARKER": str(marker)}):
            with self.assertRaisesRegex(RuntimeError, "did not establish owned forward"):
                self.tunnel.start_ssh_tunnel(
                    "example.invalid", port, readiness_timeout=3, poll_interval=0.02
                )
        racer.join(timeout=3)
        self.assertTrue(ready_to_race.is_set())
        server.shutdown()
        server.server_close()

    def test_successful_child_is_owned_stopped_and_port_can_be_reacquired(self) -> None:
        directory, path = self.fake_path(FAKE_SSH)
        self.addCleanup(directory.cleanup)
        port = free_port()
        with path:
            first = self.tunnel.start_ssh_tunnel(
                "example.invalid", port, readiness_timeout=3, poll_interval=0.02
            )
            self.assertIsNone(first.process.poll())
            self.assertEqual(first.pid, first.process.pid)
            self.tunnel.stop_ssh_tunnel(first)
            self.assertIsNotNone(first.process.poll())

            second = self.tunnel.start_ssh_tunnel(
                "example.invalid", port, readiness_timeout=3, poll_interval=0.02
            )
            self.tunnel.stop_ssh_tunnel(second)


if __name__ == "__main__":
    unittest.main()
