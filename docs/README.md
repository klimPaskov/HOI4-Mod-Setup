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
`de7dab486e99ce926de60905e8930b20fb0eab04`; its selected-file evidence was
generated from immutable source revision
`78da3473fa7a6260944ed1df9febdab85644083e`. The app still resolves the remote
default branch to one exact commit at runtime. The source-owned publication
workflow refreshes selected-file evidence for changed skills, subagents, and
declared component trees, allowing compatible additions to flow into Latest
mode without an app release.

The application code is under `src/` and `src-tauri/`; living implementation
memory is under `.agents/skills/`; bounded subagents are under `.codex/agents/`.
