import subprocess
import unittest
from unittest.mock import patch

import check_linux_secret_service as probe


class SecretServiceReadinessTests(unittest.TestCase):
    def test_unlocked_collection_uses_the_exact_secret_service_interfaces(self):
        with patch.object(probe, "dbus", side_effect=["(':1.2',)", "(uint32 123,)",
                         "(objectpath '/org/freedesktop/secrets/collection/login',)", "(<false>,)"]) as call:
            self.assertTrue(probe.collection_ready(123, 30))
        self.assertEqual(call.call_args_list[2].args[2], "org.freedesktop.Secret.Service.ReadAlias")
        self.assertEqual(call.call_args_list[3].args[3], ["org.freedesktop.Secret.Collection", "Locked"])

    def test_only_the_fresh_daemon_can_authorize_a_readiness_write(self):
        with patch.object(probe, "dbus", side_effect=["(':1.2',)", "(uint32 999,)"]) as call:
            with self.assertRaisesRegex(probe.ReadinessError, "isolated foreground daemon"):
                probe.collection_ready(123, 30)
        self.assertEqual(call.call_count, 2)

    def test_locked_or_absent_collection_does_not_authorize_a_write(self):
        for replies in [[""], ["(':1.2',)", "(uint32 123,)", "(objectpath '/',)"],
                        ["(':1.2',)", "(uint32 123,)",
                         "(objectpath '/org/freedesktop/secrets/collection/login',)", "(<true>,)"]]:
            with patch.object(probe, "dbus", side_effect=replies):
                self.assertFalse(probe.collection_ready(123, 30))

    def test_deadline_failure_never_starts_the_write_or_retries_the_rust_test(self):
        with patch.object(probe, "collection_ready", return_value=False), \
             patch.object(probe, "probe_persistent_write") as write, \
             patch.object(probe.time, "monotonic", side_effect=[0, 1]):
            with self.assertRaisesRegex(probe.ReadinessError, "deadline"):
                probe.wait_ready(123, 0.5)
            write.assert_not_called()

    def test_failed_readiness_write_is_not_retried_and_its_unique_item_is_cleared(self):
        failure = subprocess.CompletedProcess([], 1, b"", b"")
        success = subprocess.CompletedProcess([], 0, b"", b"")
        with patch.object(probe, "run", side_effect=[failure, success]) as call:
            with self.assertRaisesRegex(probe.ReadinessError, "write failed"):
                probe.probe_persistent_write(30)
        self.assertEqual(call.call_count, 2)
        self.assertEqual(call.call_args_list[0].args[0][1], "store")
        self.assertEqual(call.call_args_list[1].args[0][1], "clear")

    def test_mismatched_read_fails_once_and_is_cleared_without_printing_the_value(self):
        success = subprocess.CompletedProcess([], 0, b"", b"")
        mismatch = subprocess.CompletedProcess([], 0, b"different-value", b"")
        with patch.object(probe, "run", side_effect=[success, mismatch, success]) as call:
            with self.assertRaisesRegex(probe.ReadinessError, "read failed"):
                probe.probe_persistent_write(30)
        self.assertEqual([row.args[0][1] for row in call.call_args_list], ["store", "lookup", "clear"])


if __name__ == "__main__":
    unittest.main()
