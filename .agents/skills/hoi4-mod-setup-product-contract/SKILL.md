---
name: hoi4-mod-setup-product-contract
description: Use for broad HOI4 Mod Setup product changes, architecture decisions, new wizard capabilities, readiness behavior, optional workflow state, or completion reviews.
---

# HOI4 Mod Setup product contract

## Use this skill when

- adding or changing a major product capability
- changing the Rust, Tauri, React, or platform ownership boundary
- changing new-project or existing-project flow
- changing readiness or Open in Codex behavior
- changing optional workflow semantics
- reviewing whether a large implementation satisfies the product promise

Use the narrower owning skill for scanner, source, transaction, security, UI, test, or release details.

## Required sources

Read:

- `AGENTS.md`
- `docs/GOAL_PROMPT.md`
- `docs/01_product_requirements.md`
- `docs/02_user_flows.md`
- `docs/09_component_dependency_model.md`
- `docs/16_platform_architecture.md`
- `docs/22_acceptance_criteria.md`
- `docs/30_codex_chatgpt_authentication.md`
- relevant schemas and examples

## Core invariants

- The user selects an AI provider and model at the start; Codex/ChatGPT is the default, and selected-provider authentication plus confirmed schema-valid analysis are required before Create, Import, Update, or Repair planning.
- Provider optimization changes semantic conventions only; deterministic validation, source trust, transaction safety, and readiness rules are provider-independent.
- New projects create both descriptors, a valid replaceable thumbnail, and the selected folder profile.
- Script prefixes and primary namespaces are retained as project conventions in installation metadata and adapted guidance; neither is emitted as a key in the internal or launcher descriptor.
- Selected starter folders are real reviewed directory entries, not `.gitkeep`
  files; rollback removes only transaction-created folders that remain empty.
- On Windows and macOS, a new project defaults to the platform-resolved `Documents/Paradox Interactive/Hearts of Iron IV/mod` directory. A missing or untrusted standard directory is visible and requires one explicit manual destination choice.
- The launcher descriptor is `<project_id>.mod` in the confirmed HOI4 `mod` directory; existing-project launcher registration is discovered from bounded sibling evidence and remains unresolved when registrations are ambiguous.
- Existing projects are scanned before mutation.
- No target project file is written before dry-run approval.
- Agentic HOI4 Modding is selectively fetched, never fully cloned by the app.
- Latest mode records an exact commit.
- Pinned mode is reproducible.
- User-modified files require a visible decision.
- Secrets never enter target project files or locks.
- Transactions are staged and reversible.
- Optional workflows cannot block core readiness when unselected or incomplete.
- `workflow.super_events` is a provider-neutral optional component. Its
  recommendation uses the checked-in `codex-analysis` component ID/schema
  contract, unselected installs add no Super Events guidance to `AGENTS.md`,
  its verified runtime text/interface destinations use the confirmed mod
  prefix instead of the upstream `hoi4ms_*` template basenames, and its
  readiness check is non-blocking.
- The app's generic portrait workflow supports Cloud, Local, RunPod, and
  Disabled. Provider state and exact upstream revision are non-secret persisted
  state; enabled output installs the provider-neutral portrait contract and
  only the selected provider skill; disabled output strips portrait provider
  components and ComfyUI guidance. The optional-workflow row shows the honest
  minimum recommendation of 16 GB VRAM and 25 GB storage without making it a
  core readiness gate. See `docs/32_comfyui_portrait_pipeline.md` for the
  current contract and acceptance rules.
- The optional flattened ChatGPT project-sources export is selectable only for Codex on Components, maps skill `SKILL.md` files to `<skill>.md`, includes the adapted AGENTS/README/subagents, shows its files and sizes, and recommends Chat without starting an upload or planning action.
- Existing-project management separately offers ChatGPT source packaging when
  the scan finds an AGENTS.md, flattened skill, or subagent source; no
  installation lock is required. It defaults to Downloads, keeps detected
  AGENTS/README/flattened skills/subagents selected, leaves root Markdown
  unchecked, and writes a safe external ZIP without mutating the mod.
- The Windows desktop entry point uses the GUI subsystem, and the read-only
  scanner inventories only agentic setup surfaces: descriptors, approved
  instructions, skills, subagents, Codex/MCP configuration, managed setup
  state, and bounded Git evidence. Ordinary HOI4 gameplay, localisation,
  media, generated corpora, and unrelated root data files are not opened or
  counted.
- A newer signed app version shown at startup is installed and restarted
  automatically; a failed update leaves the current app usable and retryable.
- Adapted project instructions remove the source template's complete
  `## Placeholder Guide` section before staging, merge review, or flattened
  export; template setup directions never become final project guidance.
- Unsupported routes remain honest. Detect the current operating system
  internally; ordinary screens do not display generic platform facts and show
  **Not available on this computer** only when compatibility affects the
  selected route.
- Detect the current operating system internally. Ordinary screens do not
  display generic platform facts; when compatibility affects the selected
  route, show only **Not available on this computer**.
- The UI stays focused and uses progressive disclosure.
- New-project identity conventions are generated from the mod name and brief before review; project ID, script prefix, namespace, tags, and starter folders are never presented blank when a usable input exists. Every generated value remains editable, and explicit edits are preserved.

## Architecture rules

Keep domain behavior in Rust core modules. Keep UI components declarative and typed. The UI may edit draft state, but it does not decide filesystem safety, hashes, source trust, merge validity, transaction success, or credential policy.

Every desktop Tauri command must use `#[tauri::command(async)]`, including
read-only scans, readiness, health checks, planning, and maintenance. Blocking
Rust core work must not run on the UI event loop; retain the regression test
`every_desktop_command_uses_the_async_dispatcher` in `src-tauri/src/commands.rs`.

Platform APIs belong behind interfaces. Core tests should run with fakes. Do not let Windows path assumptions leak into platform-neutral types.

## Change workflow

1. Identify the affected user promise and acceptance criteria.
2. Update or add an architecture decision when the boundary changes.
3. Update schemas before code when persisted state changes.
4. Add migrations for existing project state and lock data.
5. Implement through the correct ownership layer.
6. Add happy-path, failure-path, and platform tests.
7. Update user docs only for user-visible behavior.
8. Update contributor docs and the owning skill for workflow changes.
9. Produce a requirement-to-code and requirement-to-test crosswalk.

## Completion evidence

A broad product change needs:

- affected requirements and criteria
- design decision and tradeoffs
- persisted state or migration impact
- transaction and rollback impact
- security impact
- Windows and macOS behavior
- UI evidence when visible
- tests and fault scenarios
- documentation and skill updates
- blockers and unsupported routes
- selected provider/model/profile and Codex-only flatten behavior

## Update this skill when

Update this skill when product invariants, architecture ownership, required documents, completion evidence, or the major change workflow changes.
