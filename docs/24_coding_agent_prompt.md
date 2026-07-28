# Detailed coding-agent implementation prompt

Implement **HOI4 Mod Setup**, a production Windows and macOS desktop application that prepares Hearts of Iron IV mod projects for agentic development with a selected AI provider. Codex is the default.

Read every document, schema, example, Mermaid diagram, UI reference, source audit, and acceptance criterion in this package before coding. Maintain a requirement-to-code matrix.

## Product boundary

Create a new mod or import an existing one. Selectively install the current package from `https://github.com/klimPaskov/Agentic-HOI4-Modding` without cloning the complete repository or requiring a checkout.

The planning audit inspected commit `599497ea2f93612d9094461c6fde114fc87a5c0f`. Do not hardcode it as permanent latest. Implement exact latest and pinned resolution.

## Architecture

Use a Tauri shell with Rust core and TypeScript React UI unless a documented spike proves another native architecture safer. The UI cannot write project files directly.

Create modules for project identity, scanner, source resolver, manifest, components, cache, hashes, merge, transactions, validators, credentials, MCP, Git, readiness, recovery, and platform adapters.

## AI provider and Codex integration

Implement the semantic layer through the official local `codex app-server` process over stdio JSONL. Complete initialize, account state, browser ChatGPT login, device-code fallback, cancellation, logout, account updates, rate-limit checks, thread lifecycle, turn lifecycle, streamed events, and clean shutdown.

Do not add an OpenAI API key field or API-key fallback to the Codex route. Add bounded provider profiles for Claude, Kimi, GLM, DeepSeek, local, and another explicitly configured OpenAI-compatible route. Non-Codex hosted routes use a user endpoint and OS-vault key; local uses loopback HTTP. Do not invent provider URLs, OAuth routes, packages, commands, model names, or platform support. Do not read Codex token files. Keep full email, account ID, plan, usage, rate limits, tokens, keys, thread history, and hidden reasoning out of project files and locks.

Use the selected model and optimization profile without hardcoding a non-Codex model. Every semantic turn is read-only and uses the current `codex-analysis` output schema. The selected provider proposes the description, display name, project ID, prefix, namespace, tags, folder profile, project instructions, components, and existing-project conventions. Deterministic Rust validates and renders after confirmation.

Missing provider configuration, usage availability, or valid analysis blocks Create, Import, Update, and Repair planning. Preserve drafts and scans. Keep recovery, rollback, backup inspection, and managed removal available offline.

## Repository contract

Implement `hoi4-mod-setup.manifest.json` using the supplied schema. The live repository currently has no manifest. Add it to the repository first, or isolate a temporary built-in bootstrap manifest behind a removable compatibility layer. Production should depend on the remote contract.

The resolver must:

1. read default branch in latest mode
2. resolve an exact commit
3. fetch manifest at that commit
4. expand only selected component trees at that commit
5. fetch selected files or a manifest-declared release bundle
6. verify SHA-256
7. record revision in plan and lock

Never clone Agentic-HOI4-Modding. Repository-declared external dependency scripts may use Git only after dry-run approval.

## New project

Implement the 17 required screen states inside a seven-phase wizard: Project, Review, Components, Integrations, Git, Install, and Ready. Collect a normal-language brief, review suggestions, confirm identity and paths, preview both descriptors on demand, propose editable folders, select source and components, ask both exact optional questions, configure MCP and Git, show dry run, transact, and show readiness.

Create no project file before approval. After approval, generate and validate the internal `descriptor.mod`, the external launcher `<project_id>.mod`, a deterministic replaceable `thumbnail.png`, and the selected folder profile. Preview external destinations. Track every generated artifact and external path in the plan, lock, backup, and rollback record. Never fabricate a Workshop ID or overwrite a user-replaced thumbnail silently.

When Codex is selected, make the final Git phase checkbox optional: prepare
`chatgpt_project_sources/` by flattening each skill to `<skill>.md` and adding
selected subagents, adapted AGENTS, the created or existing README, and only
user-entered project-relative extras. Use the full transaction and recommend
starting planning with ChatGPT “Chat”; do not upload or start planning.

## Existing project

The scanner is bounded and read-only. Detect structure, descriptors, Git, IDs, namespaces, naming, localisation, docs, skills, subagents, Codex, MCP, absolute paths, and conflicts. Every finding includes evidence and confidence. Group findings into small editable steps.

## Components

Implement dependency resolution, platform support, tools, environment, validation, reverse dependencies, and ownership types `managed`, `merged`, `generated`, and `external`.

## Wiki

Install the selected repository tree under `<mod_project>/paradox_wiki/`. Validate hash, containment, core page coverage, and media policy. Do not invent source or license metadata.

## 3D

Ask exactly:

**Do you want to set up the 3D models workflow?**

When yes, explain Meshy.ai and possible cost, store the key in Windows Credential Manager or macOS Keychain, save only an opaque reference, inject only as `MESHY_API_KEY`, and never log or serialize it.

Derive every package, command, version, adapter, and health check from the manifest or verified repository script. Install the repository-declared skill, subagent, bootstrap, wrappers, adapters, and support files. Show external actions in dry run and run approved health checks.

The current repository route is Windows-oriented. Mark it unsupported on macOS until the repository supplies a verified route. Do not invent one.

Missing key leaves 3D incomplete without blocking core readiness. Add Configure key to Update and Repair.

## LoRA and ComfyUI

Ask exactly:

**Do you want to set up LoRAs and ComfyUI for portrait generation?**

Version 1 records interest only. Create no ComfyUI, model, LoRA, Python, GPU, or driver action. Report planned or unavailable, never installed. Keep the component interface ready for a future real implementation.

## Conflicts

Compare base, local, and incoming. Offer keep, replace, merge, rename, or skip where valid. Binary files cannot use text merge. TOML and JSON use structured merge. AGENTS uses three-way merge plus project adaptation.

Never silently overwrite local changes.

## Transaction

Implement all 12 required stages, durable journaling, operation checkpoints, backup, staging validation, atomic apply where possible, resume, rollback, and discard staging according to state. Write the lock after final verification.

Fault-inject every stage and operation boundary.

## Git

Support initialize, preserve, or skip. Merge `.gitignore`, select branch, optionally commit, and optionally configure remote. Never create an online repository or push without a separate explicit approval.

## Readiness

Verify both descriptors, launcher path and discoverability, descriptor consistency, thumbnail decode and hash, structure, AGENTS, skills, subagents, selected provider, MCP, wiki, Git, environment, hashes, conflicts, dependencies, 3D, and LoRA placeholder. Verify provider configuration or Codex ChatGPT authentication and confirmed provider analysis as blocking core checks. Enable Open in Codex only for Codex when core blocking checks pass.

## Security

Use exact commit, SHA-256, root containment, link defense, argument arrays rather than shell strings, allowlisted processes, no core elevation, secret redaction, no telemetry, and explicit review of broad Codex security policies.

## UI

Follow all 17 full-resolution references and `docs/17_ui_accessibility.md`. Use a clean dark desktop UI with a seven-phase setup rail. Each screen should have one focal task, one title, no more than one supporting sentence, and no more than two visible content regions by default. Hide evidence, hashes, file lists, logs, dependency graphs, and advanced settings until requested. Do not repeat obvious information or explain controls that are already clear from their labels. Keep Back and the primary action persistent. Preserve conflict comparison detail only on the conflict screen. Support keyboard use, WCAG 2.2 AA, reduced motion, and 200 percent scaling.

## Schemas and tests

Implement and validate all supplied schemas. Use atomic JSON writes and explicit migrations. Add unit, property, fuzz, integration, end-to-end, security, performance, accessibility, visual regression, and transaction fault tests.

## Open-source repository bootstrap

Develop the application in a public GitHub-ready repository using the supplied `README.md`, `CONTRIBUTING.md`, `DEVELOPMENT.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `RELEASING.md`, `CHANGELOG.md`, `LICENSE_SELECTION.md`, `.github/`, `.gitignore`, `.gitattributes`, and `.editorconfig`. Keep the root README user-facing. Select and add a real `LICENSE` before public release.

Configure protected `main`, pull requests, stable required checks, CODEOWNERS, issue forms, private vulnerability reporting, Dependabot for npm, Cargo, and GitHub Actions, and tag-based draft releases. Use read-only default workflow permissions. Release credentials belong in protected environments and are never exposed to fork pull requests. Implement repository-owned scripts used by CI and release workflows before making those jobs required.

## AGENTS, living skills, and subagents

Use the supplied root `AGENTS.md` for application development. Keep it distinct from AGENTS files installed into target mod projects.

Treat `.agents/skills/` as living implementation memory. When a pull request changes a repeated workflow, command, path, invariant, schema, platform rule, validation step, security boundary, or recovery method, update the owning skill in the same change. Do not put ticket-specific details into skills.

Use the supplied `.codex/agents/` only for bounded audits, including the Codex integration auditor, and narrow documentation or UI patches. Spawn every project subagent with `fork_context=false`, explicit files, constraints, allowed writes, tests, and handoff path. The parent owns final integration and completion.

## Completion

Before completion, satisfy every criterion, provide a requirement-to-code matrix, validate every example and repository template, prove new and existing flows on both platforms, prove missing 3D key is non-blocking, prove LoRA creates zero forbidden actions, prove recovery, prove secret absence, review living-skill alignment, and report every unsupported source route or unresolved metadata issue.

Do not reduce the product to a file copier. The scanner, reviewed plan, conflict engine, transaction, lock, optional states, maintenance, and readiness gate are core product features.
