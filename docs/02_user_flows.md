# Complete user flows

## Common entry

1. Start the app.
2. Check for an incomplete local transaction.
3. Open recovery first when a journal is incomplete.
4. Otherwise show Welcome.
5. Show the provider and model selection before semantic planning.
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

Preview and validate `descriptor.mod`, `<project_id>.mod`, and `thumbnail.png`. Check duplicate keys, quoting, supported-version syntax, project path, destination, descriptor consistency, picture reference, PNG decoding, dimensions, and replacement policy. The script prefix and primary namespace remain project conventions and do not appear as descriptor keys.

### Folder profile

Propose a minimal editable structure. A total conversion can include bookmarks, map, and history. A focused event mod can start with `events/`, `localisation/english/`, `common/`, `interface/`, `gfx/`, `docs/`, and tests. The transaction creates the selected directories directly and does not add `.gitkeep` marker files. Missing unselected folders are not defects.

### Source and components

Choose latest, pinned commit, or pinned release. Select components and review automatically selected dependencies.

### Optional workflows

On one Optional workflows screen, ask these questions in order:

1. **Do you want to set up the 3D models workflow?**
2. **Do you want to set up the Super Events workflow?**

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
Keep LoRA and ComfyUI out of setup and readiness; the only portrait behavior
remains the fixed Ready link described below.

### MCP, credentials, and Git

Review server requirements, credential storage, external commands, Git mode, ignore rules, branch, optional commit, and optional remote.

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
candidate and its normalized `path=` match, then requires an explicit visible
choice: confirm the candidate, scan without an external descriptor, or cancel.
The scanner does not search sibling trees, sibling drives, or unrelated
folders, and it reads no unconfirmed external descriptor.

### Read-only scan

Run staged detectors with progress and current-path evidence. The scan reads a
parent-level launcher descriptor only when the user confirmed it in the
pre-scan choice. Cancellation stops reads and discards unapproved profile
state.

When the bounded scan finds a valid managed lock, it reports the stored states
of `workflow.3d` and `workflow.super_events` in its concise optional-workflow
summary. It does not infer selection from a loose skill directory or expose a
credential value. A valid `not_selected` Super Events state remains a remembered
decline until Update or Repair changes it.

### Review sequence

1. identity and descriptors
2. folder structure
3. IDs, namespaces, and naming
4. localisation conventions
5. documentation and project rules
6. skills and helpers
7. subagents
8. Codex and MCP
9. Git
10. conflicts and platform limits

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

## Portrait workflow handoff

After successful setup, show one concise Ready-screen link to
`https://github.com/klimPaskov/comfyui-hoi4-portraits`. It is a fixed HTTPS
link opened through the typed system-browser action, not a wizard step,
component, preference, transaction action, maintenance option, or readiness
result.

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

## Repair flow

Verify the selected provider before creating a repair plan. Classify managed files as healthy, missing, corrupted, or modified. Restore only missing or corrupted unmodified files automatically after review. Modified files enter conflict review. Re-run component health and readiness.

For `workflow.super_events`, Repair may offer the exact Super Events question
again when the lock records `not_selected`, but only after reading the
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
