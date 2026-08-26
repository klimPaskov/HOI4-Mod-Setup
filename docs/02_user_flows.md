# Complete user flows

## Common entry

1. Start the app.
2. Check for an incomplete local transaction.
3. Open recovery first when a journal is incomplete.
4. Otherwise show Welcome.
5. Show the provider, live model catalog, and model-supported reasoning-effort selection before semantic planning.
6. For Codex, start the local Codex App Server and read account state; for a known hosted provider, fill its verified defaults and validate the vault reference; for local or custom providers, validate the entered address.
7. Require the selected provider to be configured before Create, Import, Update, or Repair planning.
8. Display current remote source status without downloading selected components yet.
9. Choose new or existing project.

## ChatGPT sign-in flow

1. Start the local Codex App Server.
2. Read current account state.
3. When signed out, show one compact **Sign in with ChatGPT** action.
4. Open the returned browser URL.
5. Wait for the login completion event.
6. Offer the device-code path when the browser callback fails or the user selects it.
7. Show the active account and continue without exposing tokens.
8. Preserve recovery and rollback access when sign-in is unavailable.

The app does not show an OpenAI API key field.

For a known hosted provider, show its official API-key page, one provider API
key field, and a **Connect** action. Fill its verified model and address
automatically and keep them under **Advanced**. Store the key in the OS vault,
show names and connection state only, and use the first schema-validated
semantic request as the capability check. Local models ask for the address
shown by the local model app, use loopback HTTP, and are not described as
hosted accounts.

## New mod flow

### Natural-language description

The user enters a mod name and describes the mod. The app immediately fills a
valid editable project ID, script prefix, primary namespace, descriptor tags,
and starter folder profile from those inputs. The app verifies the selected provider and sends
the approved brief and wizard constraints to a schema-constrained semantic
turn. The selected provider may refine the normalized description, display name,
project ID, script prefix, namespace, tags, folder profile, likely systems, 3D
relevance, and component selection. Deterministic validators check every
field. The user edits only when desired or when the app identifies ambiguity,
then confirms the proposals before rendering.

Descriptor tags are selected only from the official Hearts of Iron IV
Workshop categories; generated and provider-proposed values use the same
deterministic allowlist.

### Identity and paths

The identity screen opens populated. Review or edit the generated display name,
stable project ID, script prefix, namespace, tags, and folder profile. Once the
ID is valid, the app auto-fills the project root and launcher descriptor path
from the ID and the resolved HOI4 user `mod` directory. Both are editable. A
changed ID updates only untouched auto-filled fields; explicit overrides stay
in place and are revalidated. Version and supported game version retain
verified defaults or appear as advanced fields when they need confirmation. The
project ID uses a lowercase stable slug and remains independent of future
display-name changes after confirmation.

### Descriptors

Preview and validate `descriptor.mod`, `<project_id>.mod`, and the deterministic 600×600 solid-black `thumbnail.png`. Check duplicate keys, quoting, supported-version syntax, project path, destination, descriptor consistency, picture reference, PNG decoding, dimensions, and replacement policy. The script prefix and primary namespace remain project conventions and do not appear as descriptor keys.

### Folder profile

Propose a minimal editable structure. A total conversion can include bookmarks, map, and history. A focused event mod can start with `events/`, `localisation/english/`, `common/`, `interface/`, `gfx/`, `docs/`, and tests. The transaction creates the selected directories directly and does not add `.gitkeep` marker files. Missing unselected folders are not defects.

### Source and components

Choose latest, pinned commit, or pinned release. Select components and review automatically selected dependencies.

### Optional workflows

On one Optional workflows screen, show these titles in order:

1. **3D models workflow**
2. **Super Events workflow**
3. **ComfyUI portrait production**

Compatible additional optional `workflow.*` components published by the
resolved manifest appear after these stable product rows. They are explicit
choices and inherit the same provider, platform, dependency, review, and
readiness rules; the app does not need a release merely to render their
source-declared title and description.

The first row installs the exact Windows 3D package, prepares its Meshy and
bounded Blender MCP routes, and includes one reviewed automatic bootstrap in
the dry run. When installation reaches post-install checks, the core verifies
the installed script, arguments, declared network/writes/privilege/rollback
boundary, Python identity, and vault reference before running it. Exit-zero
verified configuration persists `ready`; missing credentials or prerequisites
persist optional `incomplete` and keep core setup usable.

The second row selects provider-neutral `workflow.super_events`. When selected,
resolve the verified manifest at the one source revision and install its skill
plus the hidden dependency components containing the reusable GUI, GFX,
scripted registration and selectors, example event, localisation, editable
templates, assets, and guide. Adapt text identifiers to the confirmed project
namespace only after source verification. It has no credential, environment
variable, external command, or provider-specific route. Its state is optional
and non-blocking, appears in readiness, and is remembered by the managed lock
and the existing-project scan. If it is declined, do not install any part of
the dependency closure or add Super Events-specific guidance to `AGENTS.md`.

The third row asks whether the user wants the HOI4 portrait workflow. When
enabled, show the provider choice **Comfy Cloud**, **Local ComfyUI**, or
**RunPod**; Disabled is the explicit generic-project default. Persist the
choice and show only the provider-specific setup state needed for the selected
route. Restore the selected provider after import or reopening an installed
project. The expanded portrait workflow row also shows the minimum
recommendation: 16 GB VRAM and 25 GB storage.

### MCP, credentials, and Git

Review server requirements, credential storage, external commands, Git mode, ignore rules, branch, optional commit, and optional remote.
The MCP package and app-owned health check remain available for every planning
provider; only Codex-specific client registration depends on `codex.config`.

### Dry run and apply

Show every create, merge, replace, rename, skip, external command, Git action, source revision, hash, and recovery rule. Next stays disabled while a blocking conflict is unresolved.

Run the 12-stage transaction. Verify the launcher descriptor resolves to the project, both descriptors agree, the thumbnail decodes, and the selected scaffold exists. Then show readiness. Offer Open in Codex only for the Codex profile.

### Optional flattened Chat sources

Only when Codex is selected, show the optional checkbox on Components under
**Choose what to install**: **Prepare a flattened ChatGPT project-sources
folder**. Show its filenames and available sizes there. Review its generated
`chatgpt_project_sources/` operations. Skills become `<skill>.md`; selected
subagents, adapted `AGENTS.md`, and created or existing `README.md` are
included. Reject links, collisions, secret-shaped content, and oversized files
before staging.
After setup, recommend starting planning using ChatGPT “Chat”. Do not upload,
open a conversation, or start planning automatically.

## Existing project flow

### Root selection

The user selects one explicit root. Before scanning, the app checks only direct
`*.mod` descriptor candidates in that root's immediate parent. It displays each
unique matching candidate and its normalized `path=` evidence, or reports a
duplicate/mismatched registration for review. The user then confirms the
candidate, scans without an external descriptor, or cancels.
Selecting the root authorizes this bounded candidate parse only; it does not
authorize the declared target path or make candidate content scan evidence.
The scanner does not search sibling trees, sibling drives, or unrelated
folders, and it reads only the candidate the user confirms.

### Read-only scan

Run staged detectors with progress and current-path evidence. The scan reads a
parent-level launcher descriptor only when the user confirmed it in the
pre-scan choice. Cancellation stops reads and discards unapproved profile
state. The targeted inventory covers descriptors, approved setup instructions,
skills, subagents, Codex/MCP configuration, managed setup state, and bounded
Git evidence. It does not open or count ordinary gameplay, localisation,
media, generated documentation corpora, or unrelated root data dumps.

When the bounded scan finds a valid managed lock, it reports the stored states
of `workflow.3d` and `workflow.super_events` in its concise optional-workflow
summary. It does not infer selection from a loose skill directory or expose a
credential value. A valid `not_selected` Super Events state remains a remembered
decline until Update or Repair changes it.

### Review sequence

1. identity and descriptors
2. documentation and project rules
3. skills and helpers
4. subagents
5. Codex and MCP
6. Git
7. managed setup state
8. conflicts and platform limits

Namespace, prefix, naming, localisation, and folder-profile proposals belong
to the separate selected-provider review. They are not deterministic scan
findings and do not justify reading unrelated mod content.

Every deterministic finding shows value, confidence, evidence path, line or file set, impact, and recommendation. Required provider suggestions show the approved input manifest, selected provider/model/profile, linked deterministic evidence, and separate confidence. The user can accept, edit, reject, or defer non-required proposals. Every field required for planning must be confirmed.

### Existing AGENTS.md

When present, compare previous installed base when available, local content, and incoming template. Preserve project restrictions. Highlight stale absolute paths, foreign project names, missing skill references, and security-sensitive changes.

### Existing Codex and MCP

Parse TOML structurally. Show root settings, server IDs, commands, cwd, environment bindings, timeouts, sandbox, and approval policy. Never concatenate blind text fragments.

### Existing Git

Report repository root, branch, detached state, commit, clean or dirty state, staged/unstaged/untracked counts, remotes, submodules, hooks, tracked secret-like paths, and ignore files. Use bounded `--no-optional-locks` reads only; do not follow an external linked-worktree gitdir or change Git during scan.

### Conflict review and install

Resolve each path with keep, replace, merge, rename, or skip. Binary files do not offer text merge. The dry run reflects the selected outcome exactly.

## 3D selected with valid key

1. Explain provider credit use.
2. Store the key in the OS vault.
3. Validate presence and non-empty state.
4. Resolve repository 3D components.
5. Show repository-declared tools and commands.
6. Run approved preflight and health checks.
7. Record observed versions, package IDs, hashes, and results.
8. Mark ready only when selected requirements pass.

## 3D selected without a key

1. Keep the core plan valid.
2. Store no empty or placeholder secret.
3. Optionally install the workflow shell when approved.
4. Skip key-dependent actions.
5. Mark `incomplete` with a clear reason.
6. Offer Configure key in Update and Repair.

## 3D declined or unsupported

Declined produces no 3D-only operations. Unsupported platform state is shown before dry run. The app never translates commands by guesswork. Neither state blocks core AI readiness.

## Portrait workflow

Use the persisted provider state, local discovery/setup, Cloud MCP registration,
RunPod browser guidance, source/prompt archive, provider readiness, and
source-based fallback described in `docs/32_comfyui_portrait_pipeline.md`.
Portrait readiness is optional for core AI readiness but a selected provider
must not be reported as final until its own requirements pass. Settings,
Update, Repair, import, and rollback retain the same non-secret state.

## Update flow

1. Verify the selected provider configuration, or ChatGPT authentication and the Codex App Server for Codex.
2. Load the lock.
3. Verify current local hashes.
4. Resolve target revision and fetch its manifest.
5. Run a bounded read-only scan and show its approved evidence manifest.
6. Run the existing-project schema-constrained provider reanalysis and confirm its proposals.
7. Pass the fresh core-confirmed record into planning; no renderer-only record is accepted.
8. Compare base, local, and incoming.
9. Show component and dependency changes.
10. Review conflicts and confirm semantic decisions.
11. Approve dry run.
12. Execute a new transaction.
13. Refresh lock and rollback record.

Update can add `workflow.super_events` when the target verified manifest
declares it, even when the existing lock records `not_selected`. The normal
dependency, conflict, selective-download, and rollback rules still apply.

## Existing-project ChatGPT source export

1. Open **Manage an existing project** and select a project whose scan finds an
   `AGENTS.md`, flattened skill, or subagent source. A complete installation
   lock is not required.
2. Choose **Package ChatGPT project sources**.
3. Review the validated external download folder, which defaults to Downloads.
4. Keep the detected root instructions, README, flattened skills, and
   subagents selected; optionally enable root Markdown files.
5. Choose **Package sources**. Rust creates a new ZIP atomically outside the
   project and reports its path and included files. No project transaction or
   project file mutation occurs.

## Repair flow

Verify the selected provider before creating a repair plan. Classify managed files as healthy, missing, corrupted, or modified. Restore only missing or corrupted unmodified files automatically after review. Modified files enter conflict review. Re-run component health and readiness.

For `workflow.super_events`, Repair may offer the **Super Events workflow**
title again when the lock records `not_selected`, but only after reading the
manifest at the lock's immutable revision. If that revision declares the
component, Repair expands its exact locked dependency closure and installs the
skill and runtime through the normal transaction. If it does not, Repair must
explain that Update is
required and must not substitute a newer source. An already selected workflow
is shown as installed or incomplete without duplicating files.

## Removal flow

Show reverse dependencies, solely owned files, merged files, and modified files. Default to preserving merged and modified content. Update the lock and create a rollback record.

## Recovery flow

Before apply, offer resume, rollback, or discard staging. After apply starts, verify operation checkpoints and offer resume or rollback. Never delete backup material before a verified terminal state.

## Provider proposal review flow

1. Show the approved input manifest.
2. Start a read-only schema-constrained turn through the selected provider adapter.
3. Reject malformed responses.
4. Display proposals with short reasons.
5. Run deterministic validation over names, IDs, namespaces, tags, and profiles.
6. Let the user edit and confirm each required field.
7. Create the installation plan only after confirmation.
8. Preserve the draft if authentication, usage, or App Server state interrupts the turn.
