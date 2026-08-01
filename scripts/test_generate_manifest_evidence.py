from __future__ import annotations

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("generate_manifest_evidence.py")
SPEC = importlib.util.spec_from_file_location("generate_manifest_evidence", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
GENERATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GENERATOR)


class ManifestEvidenceTests(unittest.TestCase):
    def make_repository(self) -> tuple[tempfile.TemporaryDirectory[str], Path, str]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        subprocess.run(["git", "-C", str(root), "config", "user.name", "Test"], check=True)
        subprocess.run(
            ["git", "-C", str(root), "config", "user.email", "test@example.invalid"],
            check=True,
        )
        (root / ".gitattributes").write_bytes(b"*.txt text\n")
        (root / "example.txt").write_bytes(b"first\r\nsecond\r\n")
        subprocess.run(["git", "-C", str(root), "add", "."], check=True)
        subprocess.run(["git", "-C", str(root), "commit", "-qm", "fixture"], check=True)
        revision = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
        ).strip()
        return temporary, root, revision

    def test_snapshot_reads_exact_git_blob_bytes_not_worktree_line_endings(self) -> None:
        temporary, root, revision = self.make_repository()
        self.addCleanup(temporary.cleanup)

        snapshot = GENERATOR.git_snapshot(root, revision)

        self.assertEqual(snapshot["example.txt"], b"first\nsecond\n")
        self.assertNotEqual(snapshot["example.txt"], (root / "example.txt").read_bytes())

    def test_snapshot_requires_an_exact_commit(self) -> None:
        temporary, root, _revision = self.make_repository()
        self.addCleanup(temporary.cleanup)

        with self.assertRaisesRegex(SystemExit, "exact lowercase 40-character commit"):
            GENERATOR.git_snapshot(root, "HEAD")

    def test_declared_file_missing_from_commit_fails_closed(self) -> None:
        component = {"source": {"kind": "file", "path": "missing.txt"}}

        with self.assertRaisesRegex(SystemExit, "missing at the selected revision"):
            GENERATOR.git_evidence_for(component, {"example.txt": b"data"})

    def test_wiki_link_validation_accepts_markdown_escapes_and_media(self) -> None:
        snapshot = {
            "paradox_wiki/Overview.md": b"[Objects](loc\\_objects.md) ![Map](media/map.png) [[a]](#note-a) [[Modifiers|modifier]]",
            "paradox_wiki/loc_objects.md": b"# Objects\n",
            "paradox_wiki/media/map.png": b"\x89PNG\r\n\x1a\n",
        }

        GENERATOR.validate_offline_wiki_links(snapshot)

    def test_wiki_link_validation_rejects_missing_local_targets(self) -> None:
        snapshot = {
            "paradox_wiki/Overview.md": b"[Missing](missing.md)",
        }

        with self.assertRaisesRegex(SystemExit, "offline wiki link is missing"):
            GENERATOR.validate_offline_wiki_links(snapshot)


if __name__ == "__main__":
    unittest.main()
