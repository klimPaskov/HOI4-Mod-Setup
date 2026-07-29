# HOI4 Mod Setup planning package

This directory contains the implementation plan, schemas, examples, UI
references, source evidence, coding prompts, and planning reports for HOI4 Mod
Setup. All planning-package material lives here. Application source and
community files remain at the repository root; the root `README.md` stays
user-facing.

## Main contents

- `GOAL_PROMPT.md`: standalone implementation goal
- `LICENSE_SELECTION.md`: license decision record
- `01_product_requirements.md` through `31_ai_provider_profiles_and_chat_sources.md`: accepted requirements and architecture
- `prompts/`: coding-agent and goal prompts
- `schemas/`: JSON Schemas for project, provider, source, and transaction data
- `examples/`: validated example payloads
- `diagrams/`: Mermaid diagrams
- `ui-references/`: full-resolution design references
- `screenshots/`: implementation screenshots used by the user-facing README
- `source-audit/`: source inventories and verification limits
- `development/`: historical handoffs and implementation evidence
- `HOI4_MOD_SETUP_COMBINED.md`: archived combined planning package
- `VALIDATION_REPORT.md`: package validation evidence

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

1. `../README.md`
2. `../AGENTS.md`
3. `GOAL_PROMPT.md`
4. `01_product_requirements.md`
5. `02_user_flows.md`
6. `03_scanner_design.md`
7. `04_remote_repository_manifest.md`
8. `14_transaction_rollback.md`
9. `17_ui_accessibility.md`
10. `26_open_source_github_workflow.md`
11. `27_repo_local_skill_strategy.md`
12. `28_agents_subagent_architecture.md`
13. `30_codex_chatgpt_authentication.md`
14. `31_ai_provider_profiles_and_chat_sources.md`
15. `24_coding_agent_prompt.md`
16. `22_acceptance_criteria.md`

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
`31_ai_provider_profiles_and_chat_sources.md`. Codex remains the default;
Claude, Kimi, GLM, DeepSeek, local, and explicitly configured compatible
providers use the same schema-bound proposal boundary. When Codex is selected,
the setup can optionally create a transaction-managed flattened ChatGPT source
folder with safe skill-name mappings and user-selected extra files.
