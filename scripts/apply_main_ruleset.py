"""Apply the checked-in main ruleset only after every required CI check succeeds.

Without --apply this prints a reviewable ruleset and performs read-only checks.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def gh_json(path: str) -> object:
    result = subprocess.run(["gh", "api", path], check=True, capture_output=True, text=True, timeout=60)
    return json.loads(result.stdout)


def required_contexts(ruleset: dict) -> set[str]:
    return {
        check["context"]
        for rule in ruleset.get("rules", []) if rule.get("type") == "required_status_checks"
        for check in rule.get("parameters", {}).get("required_status_checks", [])
    }


def require_successful_checks(desired: dict, runs: list[dict]) -> None:
    # The check name alone is insufficient: require GitHub Actions evidence.
    latest = {}
    for run in sorted(runs, key=lambda item: item.get("id", 0)):
        if run.get("app", {}).get("id") == 15368:
            latest[run["name"]] = run
    successful = {name for name, run in latest.items() if run.get("conclusion") == "success" and run.get("status") == "completed"}
    missing = required_contexts(desired) - successful
    if missing:
        raise SystemExit(f"Ruleset application is blocked by missing successful CI evidence: {', '.join(sorted(missing))}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="klimPaskov/HOI4-Mod-Setup")
    parser.add_argument("--sha", required=True, help="Exact current main commit with successful required CI")
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repo) or not re.fullmatch(r"[0-9a-fA-F]{40}", args.sha):
        raise SystemExit("An explicit GitHub repository and exact commit are required")
    desired = json.loads((ROOT / ".github/rulesets/main-protected.json").read_text(encoding="utf-8"))
    main_sha = gh_json(f"repos/{args.repo}/git/ref/heads/main")["object"]["sha"]
    if main_sha.lower() != args.sha.lower():
        raise SystemExit("Ruleset evidence must bind the current main revision")
    runs = []
    page = 1
    while True:
        batch = gh_json(f"repos/{args.repo}/commits/{args.sha}/check-runs?per_page=100&page={page}")["check_runs"]
        runs.extend(batch)
        if len(batch) < 100:
            break
        page += 1
    require_successful_checks(desired, runs)
    candidates = [rule for rule in gh_json(f"repos/{args.repo}/rulesets") if rule.get("name") == desired["name"] and rule.get("target") == "branch"]
    if len(candidates) != 1:
        raise SystemExit("Expected exactly one existing main protection ruleset; refusing to replace unrelated policy")
    current = gh_json(f"repos/{args.repo}/rulesets/{candidates[0]['id']}")
    if not required_contexts(current).issubset(required_contexts(desired)):
        raise SystemExit("Checked-in ruleset would remove an existing required check")
    if current.get("conditions") != desired.get("conditions") or current.get("enforcement") != desired.get("enforcement"):
        raise SystemExit("Ruleset target or enforcement drift requires review")
    # Preserve server policy while adding reviewed required checks only.
    old_rules = {rule["type"]: rule for rule in current["rules"]}
    new_rules = {rule["type"]: rule for rule in desired["rules"]}
    if {key: value for key, value in old_rules.items() if key != "required_status_checks"} != {key: value for key, value in new_rules.items() if key != "required_status_checks"}:
        raise SystemExit("Ruleset application may only change required CI checks")
    payload = {key: current[key] for key in ("name", "target", "enforcement", "conditions", "bypass_actors") if key in current}
    payload["rules"] = desired["rules"]
    print(json.dumps(payload, indent=2))
    if args.apply:
        # Rebind immediately before the write to avoid using stale main evidence.
        if gh_json(f"repos/{args.repo}/git/ref/heads/main")["object"]["sha"].lower() != args.sha.lower():
            raise SystemExit("Main changed before ruleset application; rerun with fresh CI evidence")
        subprocess.run(["gh", "api", "--method", "PUT", f"repos/{args.repo}/rulesets/{candidates[0]['id']}", "--input", "-"], input=json.dumps(payload), text=True, check=True, timeout=60)


if __name__ == "__main__":
    main()
