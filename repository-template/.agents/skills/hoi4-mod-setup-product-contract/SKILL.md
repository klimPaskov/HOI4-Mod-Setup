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
- Existing projects are scanned before mutation.
- No target project file is written before dry-run approval.
- Agentic HOI4 Modding is selectively fetched, never fully cloned by the app.
- Latest mode records an exact commit.
- Pinned mode is reproducible.
- User-modified files require a visible decision.
- Secrets never enter target project files or locks.
- Transactions are staged and reversible.
- Optional workflows cannot block core readiness when unselected or incomplete.
- The optional flattened ChatGPT project-sources export is visible only for Codex, maps skill `SKILL.md` files to `<skill>.md`, includes the adapted AGENTS/README/subagents and approved extras, and recommends Chat without uploading or planning.
- Unsupported platform routes remain honest and visible.
- The UI stays focused and uses progressive disclosure.
- New-project identity conventions are generated from the mod name and brief before review; project ID, script prefix, namespace, tags, and starter folders are never presented blank when a usable input exists. Every generated value remains editable, and explicit edits are preserved.

## Architecture rules

Keep domain behavior in Rust core modules. Keep UI components declarative and typed. The UI may edit draft state, but it does not decide filesystem safety, hashes, source trust, merge validity, transaction success, or credential policy.

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
