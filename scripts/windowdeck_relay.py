"""Runs the secure_relay test helper so unencrypted Python peers can talk to the
real WindowDeck client or host. Build it with:

    cargo build -p windowdeck-transport --example secure_relay
"""
import os
import subprocess
import tempfile
from pathlib import Path

HIDDEN = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}


class PairedDevices:
    """Temporary host and client trust stores that already trust each other."""

    def __init__(self, relay):
        self.relay = str(Path(relay).resolve())
        self._temp = tempfile.TemporaryDirectory(prefix="windowdeck-trust-")
        root = Path(self._temp.name)
        self.host_dir = root / "host"
        self.client_dir = root / "client"
        subprocess.run([self.relay, "provision", str(self.host_dir), str(self.client_dir)],
                       check=True, timeout=30, **HIDDEN)

    def relay_process(self, role, upstream):
        """Starts the relay ('host': encrypted in, plain out to upstream;
        'client': plain in, encrypted out). Returns (process, address)."""
        data = self.host_dir if role == "host" else self.client_dir
        process = subprocess.Popen([self.relay, role, str(data), "127.0.0.1:0", upstream],
                                   stdout=subprocess.PIPE, text=True, **HIDDEN)
        line = process.stdout.readline().strip()
        prefix = "secure_relay_listening="
        if not line.startswith(prefix):
            process.kill()
            raise RuntimeError(f"secure_relay did not start: {line!r}")
        return process, line[len(prefix):]

    def close(self):
        self._temp.cleanup()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def stop(process):
    process.kill()
    process.wait(timeout=10)
