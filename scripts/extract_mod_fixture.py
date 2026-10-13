"""Extract untrusted public mod data only after validating the entire ZIP inventory."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys
import unicodedata
import zipfile


def extract(archive_path: Path, destination: Path, tree_path: Path) -> dict:
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise RuntimeError("Mod fixture extraction is remote-runner-only")
    if destination.exists():
        raise RuntimeError("Refusing to overwrite a fixture destination")
    tree = json.loads(tree_path.read_text(encoding="utf-8"))
    if tree.get("truncated") or len(tree["tree"]) > 100_000:
        raise RuntimeError("Unbounded or partial Git tree")
    expected = {item["path"]: item for item in tree["tree"] if item["type"] == "blob"}
    if any(item["type"] == "commit" or item.get("mode") == "120000" for item in tree["tree"]):
        raise RuntimeError("Submodules and links are not supported fixture inputs")
    with zipfile.ZipFile(archive_path) as archive:
        inventory = []
        seen = set()
        prefixes = set()
        total = 0
        for entry in archive.infolist():
            raw = entry.orig_filename
            path = PurePosixPath(raw)
            if raw != entry.filename or any(ord(char) < 32 for char in raw) or "\\" in raw or path.is_absolute() or len(path.parts) < 1 or len(path.parts) > 40 or any(part in {"", ".", ".."} for part in raw.rstrip("/").split("/")):
                raise RuntimeError("Invalid archive path")
            if any(part in {".", "..", ".git"} or ":" in part or part.rstrip(" .") != part or re.match(r"^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)", part, re.I) for part in path.parts):
                raise RuntimeError("Unsafe archive component")
            kind = stat.S_IFMT(entry.external_attr >> 16)
            if kind not in {0, stat.S_IFREG, stat.S_IFDIR} or entry.flag_bits & 1:
                raise RuntimeError("Special or encrypted archive member")
            prefixes.add(path.parts[0])
            if entry.is_dir():
                continue
            relative = PurePosixPath(*path.parts[1:]).as_posix()
            key = unicodedata.normalize("NFC", relative).casefold()
            if key in seen or relative not in expected:
                raise RuntimeError("Duplicate or unbound archive file")
            seen.add(key)
            total += entry.file_size
            if entry.file_size > 512 * 1024 * 1024 or total > 4 * 1024**3:
                raise RuntimeError("Expanded archive exceeds its bounds")
            inventory.append((entry, relative))
        if len(prefixes) != 1 or len(inventory) != len(expected):
            raise RuntimeError("Archive does not cover the complete pinned Git tree")
        destination.mkdir(parents=True)
        for entry, relative in inventory:
            target = destination.joinpath(*PurePosixPath(relative).parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            sha = hashlib.sha1(f"blob {entry.file_size}\0".encode("ascii"))
            count = 0
            with archive.open(entry) as source, target.open("xb") as output:
                while chunk := source.read(1024 * 1024):
                    count += len(chunk)
                    if count > entry.file_size:
                        raise RuntimeError("Archive member exceeded declared size")
                    sha.update(chunk)
                    output.write(chunk)
            if count != entry.file_size or sha.hexdigest() != expected[relative]["sha"]:
                raise RuntimeError(f"Archive file differs from pinned Git blob: {relative}")
    return {"files": len(expected), "bytes": total, "git_tree_sha": tree["sha"]}


if __name__ == "__main__":
    print(json.dumps(extract(*(Path(value) for value in sys.argv[1:4]))))
