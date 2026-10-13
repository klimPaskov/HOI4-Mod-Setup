"""Extract untrusted public mod data only after validating the entire ZIP inventory."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import sys
import unicodedata
import zipfile

MAX_ENTRIES = 100_000
MAX_FILE_BYTES = 512 * 1024 * 1024
MAX_TOTAL_BYTES = 4 * 1024**3
MAX_ARCHIVE_BYTES = 1536 * 1024 * 1024
MAX_RATIO = 1000
MAX_PATH_BYTES = 1024
MAX_SEGMENT_BYTES = 255
MAX_DEPTH = 40
TEXT_EOL_SUFFIXES = {".txt", ".info", ".lua", ".yml", ".gui", ".gfx", ".asset", ".csv", ".mod"}


def extract(archive_path: Path, destination: Path, tree_path: Path) -> dict:
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise RuntimeError("Mod fixture extraction is remote-runner-only")
    if destination.exists():
        raise RuntimeError("Refusing to overwrite a fixture destination")
    tree = json.loads(tree_path.read_text(encoding="utf-8"))
    if tree.get("truncated") or len(tree["tree"]) > MAX_ENTRIES:
        raise RuntimeError("Unbounded or partial Git tree")
    expected = {item["path"]: item for item in tree["tree"] if item["type"] == "blob"}
    if any(item["type"] == "commit" or item.get("mode") == "120000" for item in tree["tree"]):
        raise RuntimeError("Submodules and links are not supported fixture inputs")
    if archive_path.stat().st_size > MAX_ARCHIVE_BYTES:
        raise RuntimeError("Compressed archive exceeds its bound")
    with zipfile.ZipFile(archive_path) as archive:
        if len(archive.infolist()) > MAX_ENTRIES:
            raise RuntimeError("Archive entry count exceeds its bound")
        inventory = []
        seen = set()
        prefixes = set()
        total = 0
        crlf_files = 0
        for entry in archive.infolist():
            raw = entry.orig_filename
            path = PurePosixPath(raw)
            if raw != entry.filename or any(ord(char) < 32 for char in raw) or "\\" in raw or path.is_absolute() or len(path.parts) < 1 or len(path.parts) > MAX_DEPTH or any(part in {"", ".", ".."} for part in raw.rstrip("/").split("/")):
                raise RuntimeError("Invalid archive path")
            if len(raw.encode("utf-8")) > MAX_PATH_BYTES or any(len(part.encode("utf-8")) > MAX_SEGMENT_BYTES for part in path.parts):
                raise RuntimeError("Archive path exceeds its length bound")
            if any(part in {".", "..", ".git"} or ":" in part or part.rstrip(" .") != part or re.match(r"^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)", part, re.I) for part in path.parts):
                raise RuntimeError("Unsafe archive component")
            kind = stat.S_IFMT(entry.external_attr >> 16)
            if kind not in {0, stat.S_IFREG, stat.S_IFDIR} or entry.flag_bits & 1:
                raise RuntimeError("Special or encrypted archive member")
            prefixes.add(path.parts[0])
            if entry.is_dir():
                if entry.file_size:
                    raise RuntimeError("Archive directory contains data")
                continue
            relative = PurePosixPath(*path.parts[1:]).as_posix()
            key = unicodedata.normalize("NFC", relative).casefold()
            if key in seen or relative not in expected:
                raise RuntimeError("Duplicate or unbound archive file")
            seen.add(key)
            total += entry.file_size
            if entry.file_size > MAX_FILE_BYTES or total > MAX_TOTAL_BYTES:
                raise RuntimeError("Expanded archive exceeds its bounds")
            if entry.file_size > max(1, entry.compress_size) * MAX_RATIO:
                raise RuntimeError("Archive compression ratio exceeds its bound")
            inventory.append((entry, relative))
        if len(prefixes) != 1 or len(inventory) != len(expected):
            raise RuntimeError("Archive does not cover the complete pinned Git tree")
        destination.mkdir(parents=True)
        owned_destination = destination.resolve(strict=True)
        try:
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
                if count != entry.file_size:
                    raise RuntimeError("Archive file length changed")
                if sha.hexdigest() != expected[relative]["sha"]:
                    # git archive honors the pinned repository's text eol=crlf attributes.
                    # Authenticate the LF blob but retain the official archive's original bytes.
                    if target.suffix.lower() not in TEXT_EOL_SUFFIXES or count > 16 * 1024 * 1024:
                        raise RuntimeError(f"Archive file differs from pinned Git blob: {relative}")
                    data = target.read_bytes()
                    normalized = data.replace(b"\r\n", b"\n")
                    digest = hashlib.sha1(f"blob {len(normalized)}\0".encode("ascii") + normalized).hexdigest()
                    if b"\0" in data or digest != expected[relative]["sha"]:
                        raise RuntimeError(f"Archive file differs from pinned Git blob: {relative}")
                    crlf_files += 1
        except BaseException:
            if destination.resolve(strict=True) != owned_destination:
                raise RuntimeError("Fixture destination changed during extraction")
            shutil.rmtree(owned_destination)
            raise
    return {"files": len(expected), "bytes": total, "git_tree_sha": tree["sha"], "verified_crlf_files": crlf_files}


if __name__ == "__main__":
    print(json.dumps(extract(*(Path(value) for value in sys.argv[1:4]))))
