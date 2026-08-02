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
The bundled manifest matches the manifest published on source revision
`6d4a84cf31004a3ba899535be433b42f962e7dee`; its selected-file evidence was
generated for revision `de725e52ec2cb8d2d5796e86a93bf14bf1bb5c6b`. The app still
resolves the remote default branch to one exact commit at runtime.

The application code is under `src/` and `src-tauri/`; living implementation
memory is under `.agents/skills/`; bounded subagents are under `.codex/agents/`.
