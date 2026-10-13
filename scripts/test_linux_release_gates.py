import copy
import json
import unittest
from pathlib import Path

from scripts.apply_main_ruleset import prepare_ruleset_payload, require_successful_checks
from scripts.check_workflow_security import assert_linux_native_gate, load_workflow


class LinuxReleaseGates(unittest.TestCase):
    def test_ruleset_application_changes_only_checks_and_preserves_server_protection(self):
        desired = json.loads((Path(__file__).resolve().parents[1] / ".github/rulesets/main-protected.json").read_text())
        current = copy.deepcopy(desired)
        status = next(rule for rule in current["rules"] if rule["type"] == "required_status_checks")
        status["parameters"]["required_status_checks"] = status["parameters"]["required_status_checks"][:-1]
        review = next(rule for rule in current["rules"] if rule["type"] == "pull_request")
        review["parameters"]["required_reviewers"] = []
        review["parameters"]["require_extra_approval_for_unattributed_changes"] = True
        payload = prepare_ruleset_payload(current, desired)
        updated_review = next(rule for rule in payload["rules"] if rule["type"] == "pull_request")
        self.assertEqual(updated_review, review)
        self.assertEqual(len(status["parameters"]["required_status_checks"]), 9)
        self.assertEqual(len(next(rule for rule in payload["rules"] if rule["type"] == "required_status_checks")["parameters"]["required_status_checks"]), 10)
        weak = copy.deepcopy(desired)
        next(rule for rule in weak["rules"] if rule["type"] == "pull_request")["parameters"]["required_approving_review_count"] = 0
        with self.assertRaises(SystemExit):
            prepare_ruleset_payload(current, weak)
        remove_check = copy.deepcopy(desired)
        next(rule for rule in remove_check["rules"] if rule["type"] == "required_status_checks")["parameters"]["required_status_checks"] = []
        with self.assertRaises(SystemExit):
            prepare_ruleset_payload(current, remove_check)

    def test_native_workflows_require_linux_packages_dependencies_and_dedicated_smokes(self):
        for name, job in (("ci.yml", "desktop-e2e"), ("development-preview.yml", "build"), ("release.yml", "build-unsigned")):
            workflow = load_workflow(name)
            assert_linux_native_gate(name, workflow, job)
            without_linux = copy.deepcopy(workflow)
            without_linux["jobs"][job]["strategy"]["matrix"]["include"] = [target for target in without_linux["jobs"][job]["strategy"]["matrix"]["include"] if target["platform"] != "linux-x64"]
            with self.assertRaises(SystemExit):
                assert_linux_native_gate(name, without_linux, job)
            no_dpkg = copy.deepcopy(workflow)
            smoke = next(step for step in no_dpkg["jobs"][job]["steps"] if step.get("name") == "Launch Linux application and exercise both packages")
            del smoke["env"]["HOI4_MOD_SETUP_DEDICATED_LINUX"]
            with self.assertRaises(SystemExit):
                assert_linux_native_gate(name, no_dpkg, job)

    def test_main_ruleset_requires_successful_github_actions_linux_evidence(self):
        desired = json.loads((Path(__file__).resolve().parents[1] / ".github/rulesets/main-protected.json").read_text())
        names = [check["context"] for rule in desired["rules"] if rule["type"] == "required_status_checks" for check in rule["parameters"]["required_status_checks"]]
        self.assertIn("Desktop E2E (ubuntu-22.04)", names)
        runs = [{"id": i, "name": name, "status": "completed", "conclusion": "success", "app": {"id": 15368}} for i, name in enumerate(names)]
        require_successful_checks(desired, runs)
        for conclusion in ("failure", "skipped", "cancelled", None):
            bad = copy.deepcopy(runs)
            next(run for run in bad if run["name"] == "Desktop E2E (ubuntu-22.04)")["conclusion"] = conclusion
            with self.assertRaises(SystemExit):
                require_successful_checks(desired, bad)
        foreign_app = copy.deepcopy(runs)
        foreign_app[-1]["app"]["id"] = 0
        with self.assertRaises(SystemExit):
            require_successful_checks(desired, foreign_app)
        rerun = {"id": 1000, "name": "Desktop E2E (ubuntu-22.04)", "status": "completed", "conclusion": "failure", "app": {"id": 15368}}
        with self.assertRaises(SystemExit):
            require_successful_checks(desired, [*runs, rerun])


if __name__ == "__main__":
    unittest.main()
