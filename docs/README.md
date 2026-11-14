# HOI4 Mod Setup documentation

## For users

- [Quick start](QUICKSTART.md): install and set up a mod, step by step
- [User guide](USER_GUIDE.md): the full walkthrough, requirements and the rules for existing mods
- [Linux guide](LINUX.md) and [Linux live testing](LIVE_TESTING_LINUX.md)
- [Run anyway](RUN_ANYWAY.md): the Windows "Run anyway" and macOS "Open Anyway" steps
- [Troubleshooting](TROUBLESHOOTING.md)
- [Image generation providers](image-generation-providers.md)

## For contributors

This directory also holds the implementation contract and planning material
used to build and review the application. Start with
[the compact goal](GOAL_PROMPT.md), then read the numbered documents in order:
`00_source_audit.md` through `39_service_facade.md` are the accepted
requirements and architecture (there is no 24 or 25).

- `prompts/`: implementation prompts
- `schemas/`: JSON Schemas
- `examples/`: validated example payloads
- `diagrams/`: Mermaid source
- `source-manifest/`: one bundled remote-manifest snapshot kept as a test
  fixture; it is historical and may list retired routes such as GPTImage
- `source-audit/`: source inventories and verification boundaries
- `ui-references/`: design references
- `screenshots/`: walkthrough images
- `development/`: implementation status, test notes and archived release validations

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
