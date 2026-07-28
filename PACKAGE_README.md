# HOI4 Mod Setup planning package

This package contains the implementation plan, schemas, examples, UI references, open-source repository files, coding prompts, `AGENTS.md`, living development skills, and bounded subagent templates for HOI4 Mod Setup.

The root `README.md` is deliberately user-facing. Contributor and package-maintainer material lives in separate files.

## Main contents

- `README.md`: user-facing application README
- `AGENTS.md`: application repository rules for Codex
- `GOAL_PROMPT.md`: standalone compact implementation goal
- `CONTRIBUTING.md`: contribution and Git workflow
- `DEVELOPMENT.md`: local development setup
- `RELEASING.md`: release and artifact publication
- `SECURITY.md`: vulnerability reporting and security expectations
- `CODE_OF_CONDUCT.md`: contributor conduct
- `CHANGELOG.md`: release-visible change history
- `LICENSE`: Apache License 2.0 for the application repository
- `LICENSE_SELECTION.md`: recorded open-source license decision and release gate
- `THIRD_PARTY_NOTICES.md`: generated dependency-license inventory for source and release review
- `.agents/skills/`: ten living development skill templates
- `.codex/agents/`: nine narrow project subagent templates
- `.github/`: issue forms, pull request template, CODEOWNERS, Dependabot, CI, security, and release workflows
- `docs/`: requirements, architecture, GitHub workflow, skills, subagents, testing, roadmap, risks, and prompts
- `schemas/`: JSON Schemas for project and transaction data
- `examples/`: validated example payloads
- `diagrams/`: Mermaid diagrams
- `ui-references/`: 17 full-resolution minimal desktop screen references
- `source-audit/`: source inventories and verification limits
- `scripts/`: repository validation, native desktop smoke, release build/verification, dependency notices, and publication-asset preparation
- `HOI4_MOD_SETUP_COMBINED.md`: combined text version
- `CHECKSUMS.sha256`: package-content checksums

## Important source decisions

- Workflow source repository: `klimPaskov/Agentic-HOI4-Modding`
- Inspected planning revision: `599497ea2f93612d9094461c6fde114fc87a5c0f`
- The application never clones the complete workflow source repository
- Latest and pinned modes record exact source commits
- The wiki installs under `<mod_project>/paradox_wiki/`
- Credentials stay outside the project repository
- Optional 3D and LoRA states never block core readiness
- Every mutation uses a staged and reversible transaction
- The desktop UI uses seven grouped phases and progressive disclosure
- The application source repository is designed for public GitHub development
- Repo-local skills are updated with the implementation

## Suggested reading order

1. `README.md`
2. `AGENTS.md`
3. `GOAL_PROMPT.md`
4. `docs/01_product_requirements.md`
5. `docs/02_user_flows.md`
6. `docs/03_scanner_design.md`
7. `docs/04_remote_repository_manifest.md`
8. `docs/14_transaction_rollback.md`
9. `docs/17_ui_accessibility.md`
10. `docs/26_open_source_github_workflow.md`
11. `docs/27_repo_local_skill_strategy.md`
12. `docs/28_agents_subagent_architecture.md`
13. `docs/30_codex_chatgpt_authentication.md`
14. `docs/24_coding_agent_prompt.md`
15. `docs/22_acceptance_criteria.md`

## Validation

From the extracted package root:

```bash
python -m pip install jsonschema PyYAML
python scripts/validate_repository_templates.py
python scripts/check_committed_secrets.py
sha256sum -c CHECKSUMS.sha256
```

The checksum file excludes itself and generated ZIP containers.

## Revision 3 additions

This revision makes ChatGPT-managed Codex authentication a core setup requirement and makes Codex responsible for every semantic project proposal. It also formalizes the launcher-ready new-mod output, including both `.mod` descriptors and the replaceable thumbnail placeholder.

## Revision 4 additions

This revision makes ChatGPT-authenticated Codex analysis mandatory for semantic setup, adds the official App Server integration contract, and promotes both descriptors plus `thumbnail.png` to first-class generated and lock-managed artifacts.

## Current implementation additions

The implementation package also documents provider-neutral semantic planning in
`docs/31_ai_provider_profiles_and_chat_sources.md`. Codex remains the default;
Claude, Kimi, GLM, DeepSeek, local, and explicitly configured compatible
providers use the same schema-bound proposal boundary. When Codex is selected,
the setup can optionally create a transaction-managed flattened ChatGPT source
folder with safe skill-name mappings and user-selected extra files.
