#!/usr/bin/env python3
"""Confirm the isolated daemon and a persistent synthetic write before Rust tests."""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import time
import uuid

BUS = "org.freedesktop.DBus"
SERVICE = "org.freedesktop.secrets"
SERVICE_INTERFACE = "org.freedesktop.Secret.Service"
COLLECTION_INTERFACE = "org.freedesktop.Secret.Collection"
PROBE_SERVICE = "com.klimpaskov.hoi4-mod-setup.synthetic-vault-readiness"
PROBE_VALUE = b"synthetic-readiness-value"


class ReadinessError(RuntimeError):
    pass


def run(argv: list[str], deadline: float, maximum: float = 2,
        input_bytes: bytes | None = None) -> subprocess.CompletedProcess:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise ReadinessError("Secret Service readiness deadline expired")
    try:
        options = {"input": input_bytes} if input_bytes is not None else {"stdin": subprocess.DEVNULL}
        result = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                timeout=min(maximum, remaining), check=False, **options)
    except subprocess.TimeoutExpired as error:
        raise ReadinessError("Secret Service readiness request timed out") from error
    if len(result.stdout) > 4096 or len(result.stderr) > 4096:
        raise ReadinessError("Secret Service readiness output exceeded its bound")
    return result


def dbus(destination: str, path: str, method: str, arguments: list[str], deadline: float) -> str:
    result = run(["/usr/bin/gdbus", "call", "--session", "--timeout", "1",
                  "--dest", destination, "--object-path", path, "--method", method,
                  *arguments], deadline)
    if result.returncode:
        return ""
    return result.stdout.decode("utf-8", errors="strict").strip()


def collection_ready(expected_pid: int, deadline: float) -> bool:
    # Query the bus first so an absent service does not auto-start a different
    # daemon. The one freshly launched foreground process must own this name.
    owner = dbus(BUS, "/org/freedesktop/DBus", f"{BUS}.GetNameOwner", [SERVICE], deadline)
    match = re.fullmatch(r"\('(:[0-9]+\.[0-9]+)',\)", owner)
    if not match:
        return False
    pid = dbus(BUS, "/org/freedesktop/DBus", f"{BUS}.GetConnectionUnixProcessID", [match[1]], deadline)
    match = re.fullmatch(r"\(uint32 ([0-9]+),\)", pid)
    if not match or int(match[1]) != expected_pid:
        raise ReadinessError("Secret Service is not owned by this isolated foreground daemon")
    alias = dbus(SERVICE, "/org/freedesktop/secrets", f"{SERVICE_INTERFACE}.ReadAlias", ["default"], deadline)
    match = re.fullmatch(r"\(objectpath '(/org/freedesktop/secrets/collection/[A-Za-z0-9_]+)',\)", alias)
    if not match:
        return False
    locked = dbus(SERVICE, match[1], "org.freedesktop.DBus.Properties.Get",
                  [COLLECTION_INTERFACE, "Locked"], deadline)
    return locked == "(<false>,)"


def probe_persistent_write(deadline: float) -> None:
    # One libsecret write/read/delete establishes that fresh-vault encryption
    # and persistence have finished. These operations are never retried; the
    # separate Rust round trip also runs exactly once and retains its failures.
    attributes = ["service", PROBE_SERVICE, "probe", uuid.uuid4().hex]
    try:
        saved = run(["/usr/bin/secret-tool", "store", "--label=HOI4 synthetic readiness",
                     *attributes], deadline, 15, PROBE_VALUE)
        if saved.returncode:
            raise ReadinessError("Synthetic Secret Service readiness write failed")
        read = run(["/usr/bin/secret-tool", "lookup", *attributes], deadline, 5)
        if read.returncode or read.stdout.rstrip(b"\n") != PROBE_VALUE:
            raise ReadinessError("Synthetic Secret Service readiness read failed")
    finally:
        cleared = run(["/usr/bin/secret-tool", "clear", *attributes], deadline, 5)
        if cleared.returncode:
            raise ReadinessError("Synthetic Secret Service readiness cleanup failed")


def wait_ready(expected_pid: int, seconds: float = 30) -> None:
    deadline = time.monotonic() + seconds
    while not collection_ready(expected_pid, deadline):
        if deadline - time.monotonic() <= 0:
            raise ReadinessError("Isolated Secret Service did not become unlocked before its deadline")
        time.sleep(min(0.1, max(0, deadline - time.monotonic())))
    probe_persistent_write(deadline)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected-pid", type=int, required=True)
    arguments = parser.parse_args()
    if os.environ.get("HOI4_LINUX_VAULT_TEST_SESSION") != "isolated" or arguments.expected_pid <= 0:
        parser.error("readiness requires the dedicated synthetic vault session")
    try:
        wait_ready(arguments.expected_pid)
    except (ReadinessError, UnicodeError, OSError) as error:
        print(f"Isolated vault readiness failed: {error}", file=sys.stderr)
        return 1
    print("Isolated Secret Service ownership, unlock, and persistence are ready.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
