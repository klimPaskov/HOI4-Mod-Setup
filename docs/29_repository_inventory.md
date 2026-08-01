# Public repository inventory

HOI4 Mod Setup keeps one authoritative public repository layout. The former
bootstrap mirror was removed once the repository became the distributable
source, eliminating duplicate community files, skills, workflows, scripts, and
screenshots.

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
| `docs/LICENSE_SELECTION.md` | maintainers | Open-source license decision and release gate |
| `THIRD_PARTY_NOTICES.md` | maintainers and binary users | Generated dependency-license inventory; complete license text review remains required |
| `AGENTS.md` | Codex and maintainers | Repository-wide implementation rules |
| `docs/GOAL_PROMPT.md` | Codex | Compact implementation goal |

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

## Workflows

| File | Purpose |
| --- | --- |
| `.github/workflows/ci.yml` | Repository integrity, frontend, Rust, fuzz, and native desktop checks |
| `.github/workflows/security.yml` | Secret-pattern, npm audit, and Cargo audit checks |
| `.github/workflows/release.yml` | Exact-tag Windows and macOS build, signing, verification, and release publication |
| `.github/workflows/development-preview.yml` | Manually requested cross-platform prerelease evidence |

All workflows call the repository-owned commands in `package.json`. Production
actions are pinned to reviewed immutable revisions.

## Agent development layer

| Path | Purpose |
| --- | --- |
| `.agents/skills/` | Living workflow knowledge updated with implementation |
| `.codex/agents/` | Narrow project subagents |
| `docs/development/handoffs/` | Temporary local audit handoffs; these are removed before release |

## Integrity command

Run `pnpm validate` to check schemas, examples, workflows, skills, subagents,
the compact goal prompt, and the public README boundary.
