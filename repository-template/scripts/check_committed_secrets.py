#!/usr/bin/env python3
"""Fail on common committed secret material while allowing documented placeholders."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SKIP_DIRS = {
    ".git",
    "node_modules",
    "target",
    "dist",
    "artifacts",
    "release-artifacts",
    "ui-references",
}
TEXT_SUFFIXES = {
    ".md",
    ".txt",
    ".json",
    ".toml",
    ".yaml",
    ".yml",
    ".rs",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".py",
    ".sh",
    ".ps1",
    ".cmd",
    ".bat",
    ".env",
}

PATTERNS = [
    ("private key", re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----")),
    ("GitHub token", re.compile(r"\bgh[pousr]_[A-Za-z0-9]{30,}\b")),
    ("AWS access key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("Meshy key", re.compile(r"\bmsy_(?!your_actual_key_here\b)[A-Za-z0-9_-]{12,}\b")),
]

ASSIGNMENT = re.compile(
    r"(?im)^\s*(MESHY_API_KEY|GITHUB_TOKEN|GH_TOKEN|APPLE_PASSWORD|WINDOWS_CERTIFICATE_PASSWORD)\s*=\s*[\"']?([^\s\"']+)")
PLACEHOLDER_WORDS = {
    "placeholder",
    "example",
    "your_actual_key_here",
    "redacted",
    "changeme",
    "<secret>",
    "${{",
}


def iter_text_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        if any(part in SKIP_DIRS for part in path.relative_to(ROOT).parts):
            continue
        if path.suffix.lower() in TEXT_SUFFIXES or path.name in {".gitignore", ".gitattributes", ".editorconfig"}:
            files.append(path)
    return files


def main() -> int:
    findings: list[str] = []
    for path in iter_text_files():
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        rel = path.relative_to(ROOT)
        for label, pattern in PATTERNS:
            for match in pattern.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                findings.append(f"{rel}:{line}: possible {label}")
        for match in ASSIGNMENT.finditer(text):
            value = match.group(2).lower()
            if not any(marker in value for marker in PLACEHOLDER_WORDS):
                line = text.count("\n", 0, match.start()) + 1
                findings.append(f"{rel}:{line}: possible literal value for {match.group(1)}")

    if findings:
        print("Potential committed secrets found:", file=sys.stderr)
        for finding in findings:
            print(f"- {finding}", file=sys.stderr)
        return 1

    print("No committed secret patterns found.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
