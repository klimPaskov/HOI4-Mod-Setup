# Complete user flows

## Common entry

1. Start the app.
2. Check for an incomplete local transaction.
3. Open recovery first when a journal is incomplete.
4. Otherwise show Welcome.
5. Start the local Codex App Server and read account state.
6. Require ChatGPT sign-in before Create, Import, Update, or Repair planning.
7. Display current remote source status without downloading selected components yet.
8. Choose new or existing project.

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

## New mod flow

### Natural-language description

The user describes the mod. The app verifies ChatGPT authentication and sends the approved brief and wizard constraints to a schema-constrained Codex turn. Codex proposes the normalized description, display name, project ID, script prefix, namespace, tags, folder profile, likely systems, 3D relevance, and component selection. Deterministic validators check every field. The user edits and confirms the proposals before rendering.

### Identity and paths

Confirm display name, stable project ID, project folder, version, supported game version, tags, and launcher descriptor destination. The project ID uses a lowercase stable slug and remains independent of future display-name changes.

### Descriptors

Preview and validate `descriptor.mod`, `<project_id>.mod`, and `thumbnail.png`. Check duplicate keys, quoting, supported-version syntax, project path, destination, descriptor consistency, picture reference, PNG decoding, dimensions, and replacement policy.

### Folder profile

Propose a minimal editable structure. A total conversion can include bookmarks, map, and history. A focused event mod can start with `events/`, `localisation/english/`, `common/`, `interface/`, `gfx/`, `docs/`, and tests. The transaction creates the selected directories. Missing unselected folders are not defects.

### Source and components

Choose latest, pinned commit, or pinned release. Select components and review automatically selected dependencies.

### Optional workflows

Ask both exact questions. A missing 3D key can be deferred. LoRA and ComfyUI interest can be recorded.

### MCP, credentials, and Git

Review server requirements, credential storage, external commands, Git mode, ignore rules, branch, optional commit, and optional remote.

### Dry run and apply

Show every create, merge, replace, rename, skip, external command, Git action, source revision, hash, disk estimate, and rollback rule. Next stays disabled while a blocking conflict is unresolved.

Run the 12-stage transaction. Verify the launcher descriptor resolves to the project, both descriptors agree, the thumbnail decodes, and the selected scaffold exists. Then show readiness and Open in Codex.

## Existing project flow

### Root selection

The user selects one explicit root. The scanner does not search sibling drives or unrelated folders.

### Read-only scan

Run staged detectors with progress and current-path evidence. Cancellation stops reads and discards unapproved profile state.

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

Every deterministic finding shows value, confidence, evidence path, line or file set, impact, and recommendation. Required Codex suggestions show the approved input manifest, model metadata when reported by App Server, linked deterministic evidence, and separate confidence. The user can accept, edit, reject, or defer non-required proposals. Every field required for planning must be confirmed.

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

Declined produces no 3D-only operations. Unsupported platform state is shown before dry run. The app never translates commands by guesswork. Neither state blocks core Codex readiness.

## LoRA and ComfyUI placeholder

1. Show the exact question.
2. Explain setup is unavailable.
3. Save interest when selected.
4. Generate zero ComfyUI, model, Python, GPU, or driver operations.
5. Show `planned_unavailable` in readiness.

## Update flow

1. Verify ChatGPT authentication and the Codex App Server.
2. Load the lock.
3. Verify current local hashes.
4. Resolve target revision and fetch its manifest.
5. Run a bounded read-only scan and show its approved evidence manifest.
6. Run the existing-project schema-constrained Codex reanalysis and confirm its proposals.
7. Pass the fresh core-confirmed record into planning; no renderer-only record is accepted.
8. Compare base, local, and incoming.
9. Show component and dependency changes.
10. Review conflicts and confirm semantic decisions.
11. Approve dry run.
12. Execute a new transaction.
13. Refresh lock and rollback record.

## Repair flow

Verify ChatGPT authentication before creating a repair plan. Classify managed files as healthy, missing, corrupted, or modified. Restore only missing or corrupted unmodified files automatically after review. Modified files enter conflict review. Re-run component health and readiness.

## Removal flow

Show reverse dependencies, solely owned files, merged files, and modified files. Default to preserving merged and modified content. Update the lock and create a rollback record.

## Recovery flow

Before apply, offer resume, rollback, or discard staging. After apply starts, verify operation checkpoints and offer resume or rollback. Never delete backup material before a verified terminal state.

## Codex proposal review flow

1. Show the approved input manifest.
2. Start a read-only schema-constrained Codex turn.
3. Reject malformed responses.
4. Display proposals with short reasons.
5. Run deterministic validation over names, IDs, namespaces, tags, and profiles.
6. Let the user edit and confirm each required field.
7. Create the installation plan only after confirmation.
8. Preserve the draft if authentication, usage, or App Server state interrupts the turn.
