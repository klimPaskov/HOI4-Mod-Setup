# HOI4 Mod Setup documentation

The root [README](../README.md) is the user guide. This directory contains
the implementation contract and planning material used to build and review the
application.

Start with [the compact goal](GOAL_PROMPT.md), then read the numbered product
documents in order. Schemas, examples, prompts, diagrams, source audits, UI
references, screenshots, and development handoffs are grouped beside the
documents that define them.

## Contents

- `01_product_requirements.md` through
  `31_ai_provider_profiles_and_chat_sources.md`: accepted requirements and
  architecture
- `prompts/`: implementation prompts
- `schemas/`: JSON Schemas
- `examples/`: validated example payloads
- `diagrams/`: Mermaid source
- `source-manifest/`: the one bundled remote-manifest snapshot
- `source-audit/`: source inventories and verification boundaries
- `ui-references/`: design references
- `screenshots/`: README walkthrough images
- `development/`: current implementation status

The workflow source is
[`klimPaskov/Agentic-HOI4-Modding`](https://github.com/klimPaskov/Agentic-HOI4-Modding).
The bundled manifest matches the Agentic source publication at revision
`0bb4917dca228886aa9a52963bc09e7078666d64`; its selected-file evidence was
generated from immutable source revision
`7e50711580a3fa377f1acb834bb19a51128eacd2`. It includes complete native
Codex, Claude Code, Cursor, Qoder, and OpenCode packages projected from the
canonical Codex TOML agents. The app still resolves the remote
default branch to one exact commit at runtime. The source-owned publication
workflow refreshes selected-file evidence for changed skills, subagents, and
declared component trees, allowing compatible file-only additions to flow into
Latest mode without an app release. New executable command routes still require
an existing audited app-owned adapter contract or a matching app release.

The application code is under `src/` and `src-tauri/`; living implementation
memory is under `.agents/skills/`; bounded subagents are under `.codex/agents/`.
