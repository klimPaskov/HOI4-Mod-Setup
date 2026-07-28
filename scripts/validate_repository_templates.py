#!/usr/bin/env python3
"""Validate the planning package or its repository-template subtree."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import yaml
from jsonschema import Draft202012Validator

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.10 and older
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]
FULL_PACKAGE = (ROOT / "schemas").is_dir() and (ROOT / "examples").is_dir()

COMMON_EXPECTED = [
    "README.md",
    "AGENTS.md",
    "GOAL_PROMPT.md",
    "CONTRIBUTING.md",
    "DEVELOPMENT.md",
    "RELEASING.md",
    "SECURITY.md",
    "CODE_OF_CONDUCT.md",
    "CHANGELOG.md",
    "LICENSE_SELECTION.md",
    "docs/26_open_source_github_workflow.md",
    "docs/27_repo_local_skill_strategy.md",
    "docs/28_agents_subagent_architecture.md",
    "docs/29_repository_template_inventory.md",
    "docs/30_codex_chatgpt_authentication.md",
    "docs/31_ai_provider_profiles_and_chat_sources.md",
    ".agents/skills/hoi4-mod-setup-codex-integration/SKILL.md",
    ".codex/agents/hoi4setup_codex_integration_auditor.toml",
    ".github/CODEOWNERS",
    ".github/PULL_REQUEST_TEMPLATE.md",
    ".github/dependabot.yml",
]

FULL_EXPECTED = [
    "PACKAGE_README.md",
    "docs/25_compact_goal_prompt.md",
    "prompts/hoi4_mod_setup_goal_prompt.md",
    "diagrams/codex_auth_analysis_flow.mmd",
    "schemas/codex-analysis.schema.json",
    "examples/codex-analysis.example.json",
    "source-audit/openai_codex_app_server.json",
    "scripts/release_build.mjs",
    "scripts/release_verify.mjs",
    "scripts/generate_third_party_notices.mjs",
    "scripts/prepare_release_assets.mjs",
    "scripts/prepare_preview_assets.mjs",
    "scripts/run_desktop_e2e.mjs",
    "THIRD_PARTY_NOTICES.md",
]

EXAMPLE_SCHEMA_MAP = {
    "codex-analysis.example.json": "codex-analysis.schema.json",
    "conflict-record.example.json": "conflict-record.schema.json",
    "installation-lock.example.json": "installation-lock.schema.json",
    "installation-plan.example.json": "installation-plan.schema.json",
    "project-state.example.json": "project-state.schema.json",
    "readiness-report.example.json": "readiness-report.schema.json",
    "repository-manifest.example.json": "remote-manifest.schema.json",
    "scan-result.existing.example.json": "scan-result.schema.json",
    "scan-result.new.example.json": "scan-result.schema.json",
    "transaction-journal.example.json": "transaction-journal.schema.json",
}


def fail(message: str) -> None:
    raise AssertionError(message)


def validate_expected_files() -> None:
    expected = COMMON_EXPECTED + (FULL_EXPECTED if FULL_PACKAGE else [])
    missing = [path for path in expected if not (ROOT / path).is_file()]
    if missing:
        fail(f"Missing expected files: {missing}")


def validate_json_examples() -> None:
    if not FULL_PACKAGE:
        return
    for example_name, schema_name in EXAMPLE_SCHEMA_MAP.items():
        example = json.loads((ROOT / "examples" / example_name).read_text(encoding="utf-8"))
        schema = json.loads((ROOT / "schemas" / schema_name).read_text(encoding="utf-8"))
        errors = sorted(Draft202012Validator(schema).iter_errors(example), key=lambda e: list(e.path))
        if errors:
            formatted = "; ".join(error.message for error in errors[:5])
            fail(f"{example_name} does not validate against {schema_name}: {formatted}")


def validate_schema_contracts() -> None:
    if not FULL_PACKAGE:
        return
    required_contracts = {
        "project-state.schema.json": "codex",
        "scan-result.schema.json": "semantic_analysis",
        "installation-plan.schema.json": "codex_analysis",
        "installation-lock.schema.json": "codex_analysis",
        "readiness-report.schema.json": "codex",
    }
    for schema_name, key in required_contracts.items():
        schema = json.loads((ROOT / "schemas" / schema_name).read_text(encoding="utf-8"))
        if key not in schema.get("required", []):
            fail(f"{schema_name} does not require {key}")
    lock_schema = json.loads((ROOT / "schemas" / "installation-lock.schema.json").read_text(encoding="utf-8"))
    if "credential_values" not in lock_schema.get("properties", {}):
        fail("installation-lock.schema.json must explicitly reject credential_values")
    for schema_name in ("installation-plan.schema.json", "installation-lock.schema.json"):
        schema = json.loads((ROOT / "schemas" / schema_name).read_text(encoding="utf-8"))
        for key in ("ai_provider", "ai_model", "ai_optimization_profile", "flatten_chat_sources"):
            if key not in schema.get("properties", {}):
                fail(f"{schema_name} is missing persisted AI/flatten property: {key}")


def validate_implementation_boundary() -> None:
    if not FULL_PACKAGE:
        return
    package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
    tauri = json.loads((ROOT / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
    if package.get("version") != tauri.get("version"):
        fail("package.json and src-tauri/tauri.conf.json versions must match")
    if package.get("scripts", {}).get("desktop:e2e") != "node scripts/run_desktop_e2e.mjs":
        fail("package.json must expose the repository-owned desktop E2E smoke command")
    commands = (ROOT / "src-tauri" / "src" / "commands.rs").read_text(encoding="utf-8")
    tauri_boundary = (ROOT / "src" / "lib" / "tauri.ts").read_text(encoding="utf-8")
    for token in (
        "codex_account_read",
        "codex_login_start",
        "codex_analyze",
        "ai_account_read",
        "store_ai_provider_credential",
        "ai_analyze",
        "confirm_codex_analysis",
        "pick_project_folder",
        "pick_launcher_folder",
        "preview_source_manifest",
        "preview_installation_conflict",
        "build_installation_plan",
    ):
        if token not in commands or token not in tauri_boundary:
            fail(f"typed desktop boundary is missing {token}")
    ci = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    release = (ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    for token in ("fuzz:", "desktop-e2e:", "libgtk-3-dev"):
        if token not in ci:
            fail(f"CI is missing the repository-owned gate: {token}")
    if "pnpm desktop:e2e" not in release:
        fail("release workflow must launch the built native application smoke test")


def validate_yaml() -> None:
    for path in sorted((ROOT / ".github").rglob("*.yml")):
        with path.open("r", encoding="utf-8") as stream:
            yaml.safe_load(stream)
    for path in sorted((ROOT / ".github").rglob("*.yaml")):
        with path.open("r", encoding="utf-8") as stream:
            yaml.safe_load(stream)


def validate_toml() -> None:
    agents = sorted((ROOT / ".codex" / "agents").glob("*.toml"))
    if len(agents) < 9:
        fail("Expected at least nine project subagent TOML files")
    for path in agents:
        with path.open("rb") as stream:
            data = tomllib.load(stream)
        if not data.get("name") or not data.get("description") or not data.get("developer_instructions"):
            fail(f"Incomplete subagent file: {path}")


def validate_skills() -> None:
    skills = sorted((ROOT / ".agents" / "skills").glob("*/SKILL.md"))
    if len(skills) < 10:
        fail("Expected at least ten living skill files")
    for path in skills:
        text = path.read_text(encoding="utf-8")
        if not text.startswith("---\n") or "\nname:" not in text or "\ndescription:" not in text:
            fail(f"Invalid skill frontmatter: {path}")
        if "Update this skill when" not in text and path.parent.name != "hoi4-mod-setup-skill-maintenance":
            fail(f"Missing update trigger section: {path}")


def validate_goal_prompt() -> None:
    text = (ROOT / "GOAL_PROMPT.md").read_text(encoding="utf-8")
    if not 3000 <= len(text) <= 4000:
        fail(f"GOAL_PROMPT.md is {len(text)} characters, expected 3000 to 4000")
    for required in ["chatgpt", "codex app-server", "descriptor.mod", "thumbnail.png"]:
        if required not in text.lower():
            fail(f"GOAL_PROMPT.md is missing required term: {required}")
    mirrors = [ROOT / "docs/25_compact_goal_prompt.md", ROOT / "prompts/hoi4_mod_setup_goal_prompt.md"]
    for mirror in mirrors:
        if mirror.is_file() and text != mirror.read_text(encoding="utf-8"):
            fail(f"Goal prompt mirror differs: {mirror.relative_to(ROOT)}")


def validate_readme_boundary() -> None:
    text = (ROOT / "README.md").read_text(encoding="utf-8").lower()
    forbidden = ["pnpm install", "cargo clippy", "git switch -c", "branch ruleset"]
    present = [item for item in forbidden if item in text]
    if present:
        fail(f"Root README contains contributor-only material: {present}")
    for required in ["chatgpt", "descriptor.mod", "thumbnail.png"]:
        if required not in text:
            fail(f"Root README is missing user-facing requirement: {required}")


def validate_no_prohibited_codex_modes() -> None:
    candidates = [
        ROOT / "README.md",
        ROOT / "AGENTS.md",
        ROOT / "GOAL_PROMPT.md",
        ROOT / "docs/01_product_requirements.md",
        ROOT / "docs/02_user_flows.md",
        ROOT / "docs/03_scanner_design.md",
        ROOT / "docs/30_codex_chatgpt_authentication.md",
    ]
    prohibited = ["deterministic_only", "codex_assisted", "optional codex analysis"]
    for path in candidates:
        if not path.is_file():
            continue
        lowered = path.read_text(encoding="utf-8").lower()
        present = [term for term in prohibited if term in lowered]
        if present:
            fail(f"Prohibited legacy Codex mode in {path.relative_to(ROOT)}: {present}")


def validate_repository_template_mirrors() -> None:
    template = ROOT / "repository-template"
    if not template.is_dir():
        return
    mirrored_roots = [
        "README.md", "AGENTS.md", "GOAL_PROMPT.md", "CONTRIBUTING.md", "DEVELOPMENT.md",
        "RELEASING.md", "SECURITY.md", "CODE_OF_CONDUCT.md", "CHANGELOG.md", "LICENSE", "LICENSE_SELECTION.md",
        ".editorconfig", ".gitattributes", ".gitignore",
    ]
    for rel in mirrored_roots:
        source = ROOT / rel
        target = template / rel
        if source.is_file() and (not target.is_file() or source.read_bytes() != target.read_bytes()):
            fail(f"Repository-template mirror differs: {rel}")
    for rel in [".github", ".agents/skills", ".codex/agents"]:
        source_root = ROOT / rel
        target_root = template / rel
        for source in source_root.rglob("*"):
            if not source.is_file():
                continue
            target = target_root / source.relative_to(source_root)
            if not target.is_file() or source.read_bytes() != target.read_bytes():
                fail(f"Repository-template mirror differs: {target.relative_to(template)}")
    for rel in [
        "docs/26_open_source_github_workflow.md",
        "docs/27_repo_local_skill_strategy.md",
        "docs/28_agents_subagent_architecture.md",
        "docs/29_repository_template_inventory.md",
        "docs/30_codex_chatgpt_authentication.md",
        "docs/31_ai_provider_profiles_and_chat_sources.md",
        "scripts/check_committed_secrets.py",
        "scripts/validate_repository_templates.py",
        "scripts/prepare_preview_assets.mjs",
        "docs/preview-release-notes.md",
    ]:
        source = ROOT / rel
        target = template / rel
        if source.is_file() and (not target.is_file() or source.read_bytes() != target.read_bytes()):
            fail(f"Repository-template mirror differs: {rel}")


def validate_checksums_shape() -> None:
    path = ROOT / "CHECKSUMS.sha256"
    if not path.is_file():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        digest, rel = line.split("  ", 1)
        if len(digest) != 64 or any(ch not in "0123456789abcdef" for ch in digest):
            fail(f"Invalid checksum line for {rel}")


def main() -> int:
    checks = [
        validate_expected_files,
        validate_json_examples,
        validate_schema_contracts,
        validate_implementation_boundary,
        validate_yaml,
        validate_toml,
        validate_skills,
        validate_goal_prompt,
        validate_readme_boundary,
        validate_no_prohibited_codex_modes,
        validate_repository_template_mirrors,
        validate_checksums_shape,
    ]
    for check in checks:
        check()
    mode = "full planning package" if FULL_PACKAGE else "repository template"
    print(f"Validated {len(checks)} integrity groups for {mode}.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"Validation failed: {exc}", file=sys.stderr)
        raise
