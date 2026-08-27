# Detailed coding-agent implementation prompt

Implement **HOI4 Mod Setup**, a production Windows and macOS desktop application that prepares Hearts of Iron IV mod projects for agentic development with a selected AI provider. Codex is the default.

Read every document, schema, example, Mermaid diagram, UI reference, source audit, and acceptance criterion in this package before coding. Maintain a requirement-to-code matrix.

## Product boundary

Create a new mod or import an existing one. Selectively install the current package from `https://github.com/klimPaskov/Agentic-HOI4-Modding` without cloning the complete repository or requiring a checkout.

The planning audit inspected commit `599497ea2f93612d9094461c6fde114fc87a5c0f`. Do not hardcode it as permanent latest. Implement exact latest and pinned resolution.

## Architecture

Use a Tauri shell with Rust core and TypeScript React UI unless a documented spike proves another native architecture safer. The UI cannot write project files directly.

Create modules for project identity, scanner, source resolver, manifest, components, cache, hashes, merge, transactions, validators, credentials, MCP, Git, readiness, recovery, and platform adapters.

Every Tauri desktop command must use async/thread-pool dispatch so filesystem,
network, Git, provider, and child-process waits never block the desktop event
loop. Add a regression test with blocking fakes that proves the event loop and
cancellation remain responsive while representative commands are pending.

## AI provider and Codex integration

Implement the semantic layer through the official local `codex app-server` process over stdio JSONL. Complete initialize, account state, browser ChatGPT login, device-code fallback, cancellation, logout, account updates, rate-limit checks, thread lifecycle, turn lifecycle, streamed events, and clean shutdown.

Do not add an OpenAI API key field or API-key fallback to the Codex route. Add bounded provider profiles for Claude, Kimi, GLM, DeepSeek, local, and a custom explicitly configured OpenAI-compatible route. Non-Codex hosted routes use a user endpoint and OS-vault key; local uses loopback HTTP. Do not invent provider URLs, OAuth routes, packages, commands, model names, or platform support. Do not read Codex token files. Keep full email, account ID, plan, usage, rate limits, tokens, keys, thread history, and hidden reasoning out of project files and locks.

Use the selected setup model and optimization profile without hardcoding a non-Codex model. Every semantic turn is read-only and uses the current `codex-analysis` output schema. The selected setup assistant proposes the description, display name, project ID, prefix, namespace, tags, folder profile, provider-neutral project instructions, components, and existing-project conventions. Deterministic Rust validates and renders after confirmation. Never write the setup provider/model/profile into generated guidance or use it to select the later development client.

Missing provider configuration, usage availability, or valid analysis blocks Create, Import, Update, and Repair planning. Preserve drafts and scans. Keep recovery, rollback, backup inspection, and managed removal available offline.

## Repository contract

Use the published `hoi4-mod-setup.manifest.json` contract and its checked-in
schema for offline bootstrap evidence. Runtime still resolves the remote
manifest at one exact revision and must not substitute the bundled copy for a
new remote resolution.

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

Implement the bounded screen states inside a seven-phase wizard: Project,
Review, Components, Integrations, Git, Install, and Ready. Collect a
normal-language brief, review suggestions, confirm identity and paths, preview
both descriptors on demand, propose editable folders, select source and
components, show the **3D models workflow** title followed immediately by the
**Super Events workflow** title, configure MCP and Git, show dry run, transact, and show
readiness.

Create no project file before approval. After approval, generate and validate the internal `descriptor.mod`, the external launcher `<project_id>.mod`, a deterministic replaceable `thumbnail.png`, and the selected folder profile. Preview external destinations. Track every generated artifact and external path in the plan, lock, backup, and rollback record. Never fabricate a Workshop ID or overwrite a user-replaced thumbnail silently.

Resolve the HOI4 user `mod` directory from the native redirected Documents
location on Windows or macOS. After validating the project ID, auto-fill the
project root and launcher descriptor path, preserve explicit overrides, and
create an absent root as exactly one reviewed leaf only at apply. Rollback may
remove that leaf only when empty and must preserve unknown content.

Independently of the selected setup assistant, show the optional checkbox in the Install review: prepare
`chatgpt_project_sources/` by flattening each skill to `<skill>.md` and adding
selected subagents, adapted AGENTS, and the created or existing README. Use the
full transaction and recommend
starting planning with ChatGPT “Chat”; do not upload or start planning.

## Existing project

The scanner is bounded and read-only. Before scanning, inspect only direct
launcher descriptor candidates in the selected root's immediate parent. Show
the candidate and matching evidence and require visible confirmation before
reading it. Detect structure, descriptors, Git, IDs, namespaces, naming,
localisation, docs, skills, subagents, Codex, MCP, absolute paths, and
conflicts. Every finding includes evidence and confidence. Group findings into
small editable steps.

## Components

Implement dependency resolution, platform support, tools, environment, validation, reverse dependencies, and ownership types `managed`, `merged`, `generated`, and `external`.

Implement `workflow.super_events` as a provider-neutral optional component from
the verified manifest. It selectively installs the managed
`.agents/skills/hoi4-super-events/` tree, depends only on `core.skills`, has no
credential, environment variable, external command, or provider-specific health
route, and remains non-blocking. Exclude its tree from unselected core-skills
installs and do not add Super Events-specific `AGENTS.md` guidance unless the
component is selected. Preserve its state through the managed lock and scan;
Update may add it from the target manifest, while Repair may do so only from the
same immutable locked source revision.

## Wiki

Install the selected repository tree under `<mod_project>/paradox_wiki/`. Validate hash, containment, core page coverage, and media policy. Do not invent source or license metadata.

## 3D

Show the title:

**3D models workflow**

When yes, explain Meshy.ai and possible cost, store the key in Windows Credential Manager or macOS Keychain, save only an opaque reference, inject only as `MESHY_API_KEY`, and never log or serialize it.

Derive every package, command, version, adapter, and health check from the manifest or verified repository script. Install the repository-declared skill, subagent, bootstrap, wrappers, adapters, and support files. Show external actions in dry run and run approved health checks.

The current repository route is Windows-oriented. Mark it unsupported on macOS until the repository supplies a verified route. Do not invent one.

Missing key leaves 3D incomplete without blocking core readiness. Add Configure key to Update and Repair.

## ComfyUI HOI4 portrait workflow

Implement the optional ComfyUI portrait workflow described in
`docs/32_comfyui_portrait_pipeline.md`. Generic projects support Cloud, Local,
RunPod, and Disabled; persist the provider and exact upstream revision without
secrets; register Cloud MCP; expose bounded local discovery/setup and RunPod
browser guidance; preserve durable source/prompt packages; and keep
source-based fallback honest. Disabled output must remove provider components,
marker sections, Cloud MCP configuration, and ComfyUI-specific instructions.
Portrait state remains non-blocking to core AI readiness but must not claim
final provider completion before its own checks pass.

## Conflicts

Compare base, local, and incoming. Offer keep, replace, merge, rename, or skip where valid. Binary files cannot use text merge. TOML and JSON use structured merge. AGENTS uses three-way merge plus project adaptation.

Never silently overwrite local changes.

## Transaction

Implement all 12 required stages, durable journaling, operation checkpoints, backup, staging validation, atomic apply where possible, resume, rollback, and discard staging according to state. Write the lock after final verification.

Fault-inject every stage and operation boundary.

## Git

Support initialize, preserve, or skip. Merge `.gitignore`, select branch, optionally commit, and optionally configure remote. Never create an online repository or push without a separate explicit approval.

## Readiness

Verify both descriptors, launcher path and discoverability, descriptor consistency, thumbnail decode and hash, structure, AGENTS, skills, subagents, setup-analysis provenance, independently installed client integrations, MCP, wiki, Git, environment, hashes, conflicts, dependencies, `workflow.3d`, and `workflow.super_events`. Verify setup-assistant configuration or Codex ChatGPT authentication and confirmed provider analysis as blocking core checks; optional workflow states remain non-blocking. Enable Open in Codex when its project integration is installed and core blocking checks pass, regardless of the setup assistant.

## Security

Use exact commit, SHA-256, root containment, link defense, argument arrays rather than shell strings, allowlisted processes, no core elevation, secret redaction, no telemetry, and explicit review of broad Codex security policies.

## UI

Follow all 14 full-resolution references and `docs/17_ui_accessibility.md`. Use a clean dark desktop UI with a seven-phase setup rail. Each screen should have one focal task, one title, no more than one supporting sentence, and no more than two visible content regions by default. Hide evidence, hashes, file lists, logs, dependency graphs, and advanced settings until requested. Do not repeat obvious information or explain controls that are already clear from their labels. Keep Back and the primary action persistent. Preserve conflict comparison detail only on the conflict screen. Support keyboard use, WCAG 2.2 AA, reduced motion, and 200 percent scaling.

## Schemas and tests

Implement and validate all supplied schemas. Use atomic JSON writes and explicit migrations. Add unit, property, fuzz, integration, end-to-end, security, performance, accessibility, visual regression, and transaction fault tests.

## Open-source repository bootstrap

Develop the application in a public GitHub-ready repository using the supplied `README.md`, `CONTRIBUTING.md`, `DEVELOPMENT.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `RELEASING.md`, `CHANGELOG.md`, `docs/LICENSE_SELECTION.md`, `.github/`, `.gitignore`, `.gitattributes`, and `.editorconfig`. Keep the root README user-facing. Select and add a real `LICENSE` before public release.

Configure protected `main`, pull requests, stable required checks, CODEOWNERS, issue forms, private vulnerability reporting, Dependabot for npm, Cargo, and GitHub Actions, and exact-tag release publication after platform verification. Use read-only default workflow permissions. Release credentials belong in protected environments and are never exposed to fork pull requests. Implement repository-owned scripts used by CI and release workflows before making those jobs required.

## AGENTS, living skills, and subagents

Use the supplied root `AGENTS.md` for application development. Keep it distinct from AGENTS files installed into target mod projects.

Treat `.agents/skills/` as living implementation memory. When a pull request changes a repeated workflow, command, path, invariant, schema, platform rule, validation step, security boundary, or recovery method, update the owning skill in the same change. Do not put ticket-specific details into skills.

Use the supplied `.codex/agents/` only for bounded audits, including the Codex integration auditor, and narrow documentation or UI patches. Spawn every project subagent with `fork_context=false`, explicit files, constraints, allowed writes, tests, and handoff path. The parent owns final integration and completion.

## Completion

Before completion, satisfy every criterion, provide a requirement-to-code matrix, validate every example and repository template, prove new and existing flows on both platforms, prove missing 3D key is non-blocking, prove portrait provider persistence/readiness and the conditional Ready-screen link, prove recovery, prove secret absence, review living-skill alignment, and report every unsupported source route or unresolved metadata issue.

Do not reduce the product to a file copier. The scanner, reviewed plan, conflict engine, transaction, lock, optional states, maintenance, and readiness gate are core product features.
