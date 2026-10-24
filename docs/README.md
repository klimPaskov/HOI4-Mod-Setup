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
- `USER_GUIDE.md`: the full walkthrough moved out of the README, including the existing-mod rules
- `development/`: current implementation status

The workflow source is
[`klimPaskov/Agentic-HOI4-Modding`](https://github.com/klimPaskov/Agentic-HOI4-Modding).
The bundled manifest and its exact source/evidence revisions are recorded in
[`source-audit/live_repository_inventory.json`](source-audit/live_repository_inventory.json).
It includes complete native Claude Code, Codex, Cursor, Qoder, and OpenCode
packages, with `CLAUDE.md`, Claude skills, and Claude Markdown agents as the
canonical sources and other clients as generated projections. Manifest format 6
adds twelve more clients and per-client MCP support declarations; see
[`36_coding_environment_catalog.md`](36_coding_environment_catalog.md). The app resolves the remote
default branch to one exact commit at runtime. The source-owned publication
workflow refreshes selected-file evidence for changed skills, subagents, and
declared component trees, allowing compatible file-only additions to flow into
Latest mode without an app release. New executable command routes still require
an existing audited app-owned adapter contract or a matching app release.

The application code is under `src/` and `src-tauri/`; living implementation
memory is under `.agents/skills/`; bounded subagents are under `.codex/agents/`.
