from __future__ import annotations

from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]


def load_workflow(name: str) -> dict:
    path = ROOT / ".github" / "workflows" / name
    workflow = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(workflow, dict) or not isinstance(workflow.get("jobs"), dict):
        raise SystemExit(f"{name} does not contain a jobs object")
    return workflow


def permission(job: dict, name: str) -> str | None:
    permissions = job.get("permissions", {})
    return permissions.get(name) if isinstance(permissions, dict) else None


def assert_write_jobs_execute_no_repository_code(workflow_name: str, workflow: dict) -> None:
    blocked_uses = ("actions/checkout@", "actions/setup-node@", "actions/setup-python@")
    blocked_run = ("scripts/", "pnpm ", "npm ", "node ", "python ", "cargo ")
    for job_name, job in workflow["jobs"].items():
        if permission(job, "contents") != "write":
            continue
        for step in job.get("steps", []):
            uses = str(step.get("uses", ""))
            command = str(step.get("run", "")).lower()
            if uses.startswith(blocked_uses):
                raise SystemExit(
                    f"{workflow_name}:{job_name} checks out or configures repository code with contents:write"
                )
            if any(token in command for token in blocked_run):
                raise SystemExit(
                    f"{workflow_name}:{job_name} executes repository tooling with contents:write"
                )


def assert_release_commands_name_repository(workflow_name: str, workflow: dict) -> None:
    for job_name, job in workflow["jobs"].items():
        if permission(job, "contents") != "write":
            continue
        for step in job.get("steps", []):
            logical_commands: list[str] = []
            pending: list[str] = []
            for raw_line in str(step.get("run", "")).splitlines():
                line = raw_line.strip()
                if not line:
                    continue
                continued = line.endswith("\\")
                pending.append(line[:-1].rstrip() if continued else line)
                if not continued:
                    logical_commands.append(" ".join(pending))
                    pending = []
            if pending:
                logical_commands.append(" ".join(pending))
            for command in logical_commands:
                if "gh release " in command and '--repo "$GITHUB_REPOSITORY"' not in command:
                    raise SystemExit(
                        f"{workflow_name}:{job_name} runs gh release without an explicit repository"
                    )


def main() -> None:
    preview = load_workflow("development-preview.yml")
    release = load_workflow("release.yml")

    assert_write_jobs_execute_no_repository_code("development-preview.yml", preview)
    assert_write_jobs_execute_no_repository_code("release.yml", release)
    assert_release_commands_name_repository("development-preview.yml", preview)
    assert_release_commands_name_repository("release.yml", release)

    preview_jobs = preview["jobs"]
    if preview_jobs["build"].get("needs") != "preflight":
        raise SystemExit("preview builds must depend on the unprivileged preflight")
    if preview_jobs["publish"].get("needs") != "curate":
        raise SystemExit("preview publication must consume the curated artifact")

    release_jobs = release["jobs"]
    release_events = release.get("on", release.get(True, {}))
    if not isinstance(release_events, dict) or "workflow_dispatch" in release_events:
        raise SystemExit("stable release must be tag-only; previews own manual dispatch")
    if release_jobs["build-unsigned"].get("needs") != "gate":
        raise SystemExit("unsigned release builds must depend on the unprivileged gate")
    if release_jobs["sign-windows"].get("needs") != "build-unsigned":
        raise SystemExit("Windows signing must consume unsigned build artifacts")
    if release_jobs["sign-macos"].get("needs") != "build-unsigned":
        raise SystemExit("macOS signing must consume unsigned build artifacts")
    if release_jobs["publish-release"].get("needs") != "curate":
        raise SystemExit("stable publication must consume the curated artifact")
    publish_script = "\n".join(
        str(step.get("run", ""))
        for step in release_jobs["publish-release"].get("steps", [])
    )
    for token in (
        'gh release download "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY"',
        "expected_names=",
        "remote_names=",
        "sha256sum",
    ):
        if token not in publish_script:
            raise SystemExit(f"stable publication is missing remote draft verification: {token}")
    if "environment" in release_jobs["gate"] or permission(release_jobs["gate"], "id-token"):
        raise SystemExit("release gate must not receive a protected environment or OIDC")

    oidc_jobs = {
        job_name
        for job_name, job in release_jobs.items()
        if permission(job, "id-token") == "write"
    }
    if oidc_jobs != {"sign-windows"}:
        raise SystemExit(f"OIDC must be scoped only to Windows signing; found {sorted(oidc_jobs)}")
    for job_name in ("sign-windows", "sign-macos"):
        if any(
            str(step.get("uses", "")).startswith("actions/checkout@")
            for step in release_jobs[job_name].get("steps", [])
        ):
            raise SystemExit(f"{job_name} must not check out repository code")
        updater_steps = [
            step for step in release_jobs[job_name].get("steps", [])
            if "TAURI_SIGNING_PRIVATE_KEY" in str(step.get("env", {}))
        ]
        if len(updater_steps) != 1:
            raise SystemExit(f"{job_name} must expose the updater key to exactly one bounded step")

    mac_steps = {
        step.get("name"): step for step in release_jobs["sign-macos"].get("steps", [])
    }
    community_step = mac_steps.get("Sign community macOS build")
    official_step = mac_steps.get("Sign and notarize official macOS build")
    if not isinstance(community_step, dict) or not isinstance(official_step, dict):
        raise SystemExit("macOS official and community signing steps must remain separate")
    if "secrets." in str(community_step):
        raise SystemExit("community macOS signing must not receive Apple secrets")
    if "HOI4_MOD_SETUP_APPLE_CERTIFICATE" not in str(official_step):
        raise SystemExit("official macOS signing must receive its certificate only in its bounded step")
    build_steps = release_jobs["build-unsigned"].get("steps", [])
    if not any(step.get("run") == "pnpm installer:e2e" for step in build_steps):
        raise SystemExit("native release builds must run the installer lifecycle smoke test")

    print("Release workflow authority and publication isolation checks passed.")


if __name__ == "__main__":
    main()
