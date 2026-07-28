# Open-source repository template inventory

This planning package includes a repository-ready development layer for HOI4 Mod Setup.

## Root files

| File | Audience | Purpose |
| --- | --- | --- |
| `README.md` | users | Product description, download, quick start, requirements, privacy, support |
| `CONTRIBUTING.md` | contributors | Branch, commit, pull request, test, docs, and skill rules |
| `DEVELOPMENT.md` | contributors | Local source setup and architecture orientation |
| `RELEASING.md` | maintainers | Version, tag, build, signing, verification, and publication |
| `SECURITY.md` | researchers and maintainers | Private vulnerability reporting and security expectations |
| `CODE_OF_CONDUCT.md` | community | Contributor conduct |
| `CHANGELOG.md` | users and maintainers | Release-visible change history |
| `LICENSE` | users and contributors | Apache License 2.0 terms |
| `LICENSE_SELECTION.md` | maintainers | Open-source license decision and release gate |
| `THIRD_PARTY_NOTICES.md` | maintainers and binary users | Generated dependency-license inventory; complete license text review remains required |
| `AGENTS.md` | Codex and maintainers | Repository-wide implementation rules |
| `GOAL_PROMPT.md` | Codex | Compact implementation goal |

## Git configuration

| File | Purpose |
| --- | --- |
| `.gitignore` | Exclude dependencies, build output, credentials, local state, logs, and release artifacts |
| `.gitattributes` | Stable line endings and binary classification |
| `.editorconfig` | Shared basic editor formatting |

## GitHub community files

| File | Purpose |
| --- | --- |
| `.github/CODEOWNERS` | Sensitive path review ownership |
| `.github/PULL_REQUEST_TEMPLATE.md` | Reviewable pull request evidence |
| `.github/ISSUE_TEMPLATE/01_bug_report.yml` | Reproducible user bug reports |
| `.github/ISSUE_TEMPLATE/02_feature_request.yml` | Product proposals |
| `.github/ISSUE_TEMPLATE/03_source_manifest_issue.yml` | Workflow source, manifest, wiki, MCP, and dependency reports |
| `.github/ISSUE_TEMPLATE/config.yml` | Issue chooser policy |
| `.github/dependabot.yml` | npm, Cargo, and Actions version updates |

## Workflow templates

| File | Purpose |
| --- | --- |
| `.github/workflows/ci.yml` | Planning integrity plus frontend and Rust checks when source manifests exist |
| `.github/workflows/security.yml` | Secret-pattern, npm audit, and Cargo audit checks |
| `.github/workflows/release.yml` | Tag-based Windows and macOS build and draft release flow |

The build workflows call repository-owned scripts. The current package includes
validation, desktop smoke, release build/verification, dependency-notice
generation, and publication-asset preparation scripts; keep their command names
stable before making jobs required.

## Agent development layer

| Path | Purpose |
| --- | --- |
| `.agents/skills/` | Living workflow knowledge updated with implementation |
| `.codex/agents/` | Narrow project subagents |
| `docs/development/handoffs/` | Subagent reports and patch handoffs |

## Activation checklist

Before using the templates in the application source repository:

1. Replace any repository name or owner that differs from the final GitHub location.
2. Select and add a real `LICENSE`.
3. Confirm CODEOWNERS identities have write access.
4. Implement repository scripts used by CI and release workflows.
5. Pin production actions to reviewed immutable revisions.
6. Configure branch rulesets and required check names.
7. Enable private vulnerability reporting and dependency security features.
8. Configure release environments and signing secrets.
9. Verify the root README remains user-facing.
10. Run `scripts/validate_repository_templates.py`.

## Revision 3 additions

- `.agents/skills/hoi4-mod-setup-codex-integration/SKILL.md`
- `.codex/agents/hoi4setup_codex_integration_auditor.toml`
- `docs/30_codex_chatgpt_authentication.md`
- `diagrams/codex_auth_analysis_flow.mmd`
- `schemas/codex-analysis.schema.json`
- `examples/codex-analysis.example.json`
- `source-audit/openai_codex_app_server.json`

## Revision 4 additions

- required ChatGPT sign-in and Codex App Server contract
- `docs/30_codex_chatgpt_authentication.md`
- Codex analysis schema and example
- Codex integration living skill and auditor
- launcher scaffold ownership for both descriptors and `thumbnail.png`
- updated plan, lock, scan, project-state, and readiness schemas

## Current implementation additions

- provider-neutral planning profiles and explicit endpoint/vault boundaries
- selected-provider/model bindings in adapted instructions, plans, locks, and readiness
- `docs/31_ai_provider_profiles_and_chat_sources.md`
- Codex-only optional flattened ChatGPT project-sources output
- checksum coverage for the new planning document and its repository-template mirror
