from __future__ import annotations

import hashlib
import json
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from scripts.extract_mod_fixture import extract


class FixtureExtractionTests(unittest.TestCase):
    def fixture(self, root, entries):
        archive = root / "mod.zip"
        with zipfile.ZipFile(archive, "w") as output:
            for name, body in entries:
                output.writestr(name, body)
        tree = root / "tree.json"
        payload = b'name="Example"\n'
        digest = hashlib.sha1(b"blob " + str(len(payload)).encode() + b"\0" + payload).hexdigest()
        tree.write_text(json.dumps({"sha": "a" * 40, "truncated": False, "tree": [{"path": "descriptor.mod", "type": "blob", "mode": "100644", "sha": digest}]}), encoding="utf-8")
        return archive, tree, payload

    @patch.dict("os.environ", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"})
    def test_valid_complete_pinned_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, tree, payload = self.fixture(root, [("mod/descriptor.mod", b'name="Example"\n')])
            result = extract(archive, root / "out", tree)
            self.assertEqual(result["files"], 1)
            self.assertEqual((root / "out/descriptor.mod").read_bytes(), payload)

    @patch.dict("os.environ", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"})
    def test_invalid_archive_inventory_is_rejected_before_extraction(self):
        for name in ["mod/../descriptor.mod", "mod/./descriptor.mod", "mod//descriptor.mod", "C:/descriptor.mod", "mod/NUL.txt", "mod/a:stream", "mod/descriptor.mod.", "mod/.git/config", "mod\\descriptor.mod"]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive, tree, _ = self.fixture(root, [(name, b'x')])
                with self.assertRaises(RuntimeError):
                    extract(archive, root / "out", tree)
                self.assertFalse((root / "out").exists())

    @patch.dict("os.environ", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"})
    def test_symlink_and_duplicate_are_rejected(self):
        for link in [False, True]:
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                info = zipfile.ZipInfo("mod/descriptor.mod")
                info.create_system = 3
                info.external_attr = (stat.S_IFLNK | 0o777) << 16
                entries = [(info, b"outside")] if link else [("mod/descriptor.mod", b"one"), ("mod/descriptor.mod", b"two")]
                archive, tree, _ = self.fixture(root, entries)
                with self.assertRaises(RuntimeError):
                    extract(archive, root / "out", tree)
                self.assertFalse((root / "out").exists())

    @patch.dict("os.environ", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"})
    def test_content_must_match_immutable_git_blob(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, tree, _ = self.fixture(root, [("mod/descriptor.mod", b"changed")])
            with self.assertRaisesRegex(RuntimeError, "differs from pinned Git blob"):
                extract(archive, root / "out", tree)

    @patch.dict("os.environ", {"RUNNER_ENVIRONMENT": "self-hosted"})
    def test_never_extracts_on_local_or_self_hosted_runner(self):
        with self.assertRaisesRegex(RuntimeError, "remote-runner-only"):
            extract(Path("unused.zip"), Path("unused"), Path("unused.json"))


if __name__ == "__main__":
    unittest.main()
