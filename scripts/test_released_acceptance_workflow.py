import copy
import unittest

from scripts.check_workflow_security import assert_candidate_acceptance, load_workflow


class CandidateAuthorityTests(unittest.TestCase):
    def test_current_workflows_preserve_the_authority_chain(self):
        assert_candidate_acceptance(load_workflow("ci.yml"), load_workflow("released-app-e2e.yml"))

    def test_authority_and_artifact_drift_fail_closed(self):
        original = load_workflow("ci.yml")
        historical = load_workflow("released-app-e2e.yml")
        for change in ["permissions", "branches", "needs", "condition", "environment", "job_permission", "checkout", "unpinned", "foreign_run", "foreign_repo", "artifact"]:
            with self.subTest(change=change):
                workflow = copy.deepcopy(original)
                job = workflow["jobs"]["candidate-acceptance"]
                events = workflow.get("on", workflow.get(True))
                download = next(step for step in job["steps"] if step.get("uses", "").startswith("actions/download-artifact@"))
                if change == "permissions":
                    workflow["permissions"]["contents"] = "write"
                elif change == "branches":
                    events["push"]["branches"] = ["*"]
                elif change == "needs":
                    job["needs"] = "frontend"
                elif change == "condition":
                    job["if"] = "always()"
                elif change == "environment":
                    job["environment"] = "release"
                elif change == "job_permission":
                    job["permissions"] = {"contents": "write"}
                elif change == "checkout":
                    next(step for step in job["steps"] if step.get("uses", "").startswith("actions/checkout@"))["with"]["persist-credentials"] = True
                elif change == "unpinned":
                    download["uses"] = "actions/download-artifact@main"
                elif change == "foreign_run":
                    download["with"]["run-id"] = 1
                elif change == "foreign_repo":
                    download["with"]["repository"] = "unreviewed/repository"
                elif change == "artifact":
                    download["with"]["name"] = "other-artifact"
                with self.assertRaises(SystemExit):
                    assert_candidate_acceptance(workflow, historical)

    def test_historical_failures_do_not_become_candidate_claims(self):
        historical = copy.deepcopy(load_workflow("released-app-e2e.yml"))
        historical.get("on", historical.get(True))["pull_request"] = {}
        with self.assertRaises(SystemExit):
            assert_candidate_acceptance(load_workflow("ci.yml"), historical)
