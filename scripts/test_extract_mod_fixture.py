from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import stat
import shutil
import subprocess
import sys
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
        for name in ["mod/../descriptor.mod", "mod/./descriptor.mod", "mod//descriptor.mod", "C:/descriptor.mod", "mod/NUL.txt", "mod/a:stream", "mod/descriptor.mod.", "mod/.git/config", "mod\\descriptor.mod", "mod/" + "a" * 256, "mod/" + ("a" * 100 + "/") * 11 + "file", "mod/" + "a/" * 40 + "file"]:
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
            self.assertFalse((root / "out").exists())

    @patch.dict("os.environ", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"})
    def test_declared_text_crlf_is_verified_without_changing_archive_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, tree, _ = self.fixture(root, [("mod/descriptor.mod", b'name="Example"\r\n')])
            result = extract(archive, root / "out", tree)
            self.assertEqual(result["verified_crlf_files"], 1)
            self.assertEqual((root / "out/descriptor.mod").read_bytes(), b'name="Example"\r\n')

    @patch.dict("os.environ", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"})
    def test_absolute_isolated_python_ignores_path_and_module_shadows(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, tree, _ = self.fixture(root, [("mod/descriptor.mod", b'name="Example"\n')])
            script = root / "extract_mod_fixture.py"
            shutil.copyfile(Path(__file__).with_name("extract_mod_fixture.py"), script)
            (root / "zipfile.py").write_text("raise RuntimeError('shadow module ran')", encoding="utf-8")
            (root / "python.cmd").write_text("exit /b 99", encoding="ascii")
            env = {**os.environ, "PATH": str(root), "PYTHONPATH": str(root)}
            result = subprocess.run([sys.executable, "-I", "-S", str(script), str(archive), str(root / "out"), str(tree)], cwd=root, env=env, capture_output=True, text=True, timeout=30, check=True)
            self.assertEqual(json.loads(result.stdout)["files"], 1)

    @patch.dict("os.environ", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"})
    def test_size_count_and_ratio_limits_precede_extraction(self):
        for limit, value in [("MAX_ENTRIES", 0), ("MAX_FILE_BYTES", 1), ("MAX_TOTAL_BYTES", 1), ("MAX_ARCHIVE_BYTES", 1), ("MAX_RATIO", 0)]:
            with self.subTest(limit=limit), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive, tree, _ = self.fixture(root, [("mod/descriptor.mod", b'name="Example"\n')])
                with patch("scripts.extract_mod_fixture." + limit, value), self.assertRaises(RuntimeError):
                    extract(archive, root / "out", tree)
                self.assertFalse((root / "out").exists())

    @patch.dict("os.environ", {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"})
    def test_directory_entries_cannot_bypass_inventory_count(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, tree, _ = self.fixture(root, [("mod/", b""), ("mod/descriptor.mod", b'name="Example"\n')])
            with patch("scripts.extract_mod_fixture.MAX_ENTRIES", 1), self.assertRaisesRegex(RuntimeError, "entry count"):
                extract(archive, root / "out", tree)
            self.assertFalse((root / "out").exists())

    @patch.dict("os.environ", {"RUNNER_ENVIRONMENT": "self-hosted"})
    def test_never_extracts_on_local_or_self_hosted_runner(self):
        with self.assertRaisesRegex(RuntimeError, "remote-runner-only"):
            extract(Path("unused.zip"), Path("unused"), Path("unused.json"))


if __name__ == "__main__":
    unittest.main()
