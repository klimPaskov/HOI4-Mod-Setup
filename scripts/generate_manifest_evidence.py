"""Populate manifest file evidence from one audited source tree.

This script only hashes paths already declared by the manifest. It never
discovers a checkout and never adds a source path or package name. Run it from
the repository with an explicitly selected source root when the upstream
manifest is regenerated for a new immutable revision.
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import io
import json
import re
import subprocess
import tarfile
from pathlib import Path, PurePosixPath


def glob_matches(pattern: str, value: str) -> bool:
    expression = "^"
    index = 0
    while index < len(pattern):
        if pattern.startswith("**", index):
            expression += ".*"
            index += 2
        elif pattern[index] == "*":
            expression += "[^/]*"
            index += 1
        elif pattern[index] == "?":
            expression += "[^/]"
            index += 1
        else:
            expression += re.escape(pattern[index])
            index += 1
    return re.fullmatch(expression[1:], value) is not None


def declared_paths(component: dict, source_root: Path) -> list[Path]:
    source = component["source"]
    kind = source["kind"]
    if kind == "generated":
        return []
    source_path = PurePosixPath(source["path"])
    root = source_root.joinpath(*source_path.parts)
    if kind == "file":
        if not root.is_file() or root.is_symlink():
            raise SystemExit(f"declared source file is missing or linked: {root}")
        return [root]
    if kind != "tree" or not root.is_dir() or root.is_symlink():
        raise SystemExit(f"declared source tree is missing or linked: {root}")
    include = source.get("include", [])
    exclude = source.get("exclude", [])
    paths: list[Path] = []
    for candidate in sorted(root.rglob("*")):
        if not candidate.is_file() or candidate.is_symlink():
            continue
        relative = candidate.relative_to(root).as_posix()
        included = not include or any(glob_matches(pattern, relative) for pattern in include)
        excluded = any(glob_matches(pattern, relative) for pattern in exclude)
        if included and not excluded:
            paths.append(candidate)
    return paths


def evidence_for(component: dict, source_root: Path) -> list[dict]:
    entries = []
    source_root = source_root.resolve()
    for path in declared_paths(component, source_root):
        relative = path.relative_to(source_root).as_posix()
        data = path.read_bytes()
        entries.append(
            {
                "path": relative,
                "sha256": hashlib.sha256(data).hexdigest(),
                "size": len(data),
            }
        )
    return entries


def git_snapshot(source_root: Path, revision: str) -> dict[str, bytes]:
    """Read only tracked bytes from one explicit immutable Git revision."""
    archive = subprocess.check_output(
        ["git", "-C", str(source_root), "archive", "--format=tar", revision]
    )
    snapshot: dict[str, bytes] = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as tar:
        for member in tar:
            if not member.isfile():
                continue
            handle = tar.extractfile(member)
            if handle is not None:
                snapshot[member.name] = handle.read()
    return snapshot


def git_evidence_for(component: dict, snapshot: dict[str, bytes]) -> list[dict]:
    source = component["source"]
    if source["kind"] == "generated":
        return []
    prefix = PurePosixPath(source["path"])
    if source["kind"] == "file":
        names = [prefix.as_posix()]
    else:
        include = source.get("include", [])
        exclude = source.get("exclude", [])
        names = []
        for name in sorted(snapshot):
            if name != prefix.as_posix() and not name.startswith(f"{prefix.as_posix()}/"):
                continue
            relative = name[len(prefix.as_posix()):].lstrip("/")
            if include and not any(glob_matches(pattern, relative) for pattern in include):
                continue
            if any(glob_matches(pattern, relative) for pattern in exclude):
                continue
            names.append(name)
    missing = [name for name in names if name not in snapshot]
    if missing:
        raise SystemExit(f"declared source file is missing at the selected revision: {missing[0]}")
    return [
        {"path": name, "sha256": hashlib.sha256(snapshot[name]).hexdigest(), "size": len(snapshot[name])}
        for name in names
    ]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument(
        "--revision",
        help="Populate evidence from tracked Git bytes at this immutable revision.",
    )
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    source_root = args.source_root.resolve()
    if not source_root.is_dir():
        raise SystemExit(f"source root is not a directory: {source_root}")
    snapshot = git_snapshot(source_root, args.revision) if args.revision else None
    if args.revision:
        manifest["generated_for_revision"] = args.revision
    for component in manifest["components"]:
        component["expected_files"] = (
            git_evidence_for(component, snapshot)
            if snapshot is not None
            else evidence_for(component, source_root)
        )
    args.manifest.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
