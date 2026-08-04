# Changelog

HOI4 Mod Setup follows semantic versioning.

## Unreleased

No unreleased changes.

## 0.2.3 - 2026-08-04

- Update the ComfyUI portrait integration to the current upstream 2.4.1 workflow package, including adaptive source framing, eight pinned model files, current workflow hashes, and the RunPod slim ComfyUI path.
- Show the optional portrait workflow's minimum requirement of 16 GB VRAM and 25 GB storage, and refresh the related user guidance, screenshots, and release notes.

## 0.2.2 - 2026-08-04

- Show live percentage and estimated time while preparing the installation review.
- Open maintenance plans in visible review, fix the Codex desktop opener, simplify Ready-screen optional states, and link the product repository and ChatGPT handoff correctly.

- Keep script prefixes and primary namespaces out of generated HOI4 descriptor files while retaining them for project guidance and later maintenance.
- Limit descriptor tags to official HOI4 categories and present them as editable choices.

### Fixed

- Made installation use the core-owned reviewed plan so provider analysis records are not serialized through the interface
- Show only recovery actions that can actually be used at the current transaction checkpoint
- Install the complete optional Super Events guidance and research package at its intended project paths
- Keep plan preparation visibly active and show journal-backed percentages and current files during installation
- Remove decorative information letters from status labels
- Replace quadratic full-journal rewrites with a bounded durable checkpoint log for much faster large installs
- Batch crash-safe install and rollback intents, accelerate rollback backup/apply, and show live recovery progress
- Prevent valid installations from falling into recovery because selected subagents omitted the required bounded-context spawn rule
- Align plan preparation with the review, contain recovery progress text, open integration requirements by default, and make the file-plan disclosure show real files
- Show percentage and estimated time remaining while generated mod details are prepared from the description
- Remove the source template's placeholder guide from installed and flattened project instructions
- Create starter folders directly without installing `.gitkeep` marker files
- Keep centered setup panels consistently wide, separate Meshy actions from the key field, and make Configure later advance without touching a stored key
- Keep file counts with installation progress and prevent optional 3D checks from freezing or crashing the app
- Remove operating-system details and internal planning notes from normal project guidance

### Changed

- Keep the standard skills and subagents free of Super Events guidance unless the workflow is selected
- Show an elapsed-rate estimate of the remaining installation time once enough progress is available

## 0.2.1 - 2026-08-01

### Fixed

- Restored Codex planning with the current Codex App Server read-only request format
- Made stalled Codex checks return to a retryable state instead of waiting for minutes

### Changed

- Claude, Kimi, GLM, and DeepSeek now fill their connection details automatically and ask only for an API key in the normal setup path

## 0.2.0 - 2026-08-01

### Added

- Signed in-app updates with a quiet launch check and user-approved install
- Optional Super Events workflow and repair support for previously prepared mods
- Codex-only flattened ChatGPT project sources in the Components step

### Changed

- Simplified user-facing text, component review, and application artwork
- Consolidated routine dependency updates into one pull request per ecosystem

## 0.1.1 - 2026-07-29

### Added

- Windows and macOS Tauri desktop wizard for new and existing HOI4 mods
- Codex App Server sign-in plus explicit Claude, Kimi, GLM, DeepSeek, local,
  and custom provider profiles
- Automatic editable identity, descriptor, namespace, tag, and folder values
  from a mod name and natural-language description
- Selective source-manifest installation, offline wiki, repair, reinstall,
  rollback, managed removal, and interrupted-transaction recovery
- Git initialize/preserve/skip, separately approved online push and public
  GitHub creation, and optional Codex-only flattened ChatGPT sources
- Windows Credential Manager and macOS Keychain integration
- Windows and macOS native build, package, launch-smoke, and release workflows

### Changed

- The interface now uses one clean app surface, larger controls, plain titles,
  and simpler user-facing language
- The portrait setup placeholder was removed; successful setup links to the
  separate ComfyUI HOI4 Portraits project
- GitHub Releases publish three clearly named installer files

### Security

- Existing projects are scanned read-only before planning
- Downloads and transactions are bound to reviewed source and operation
  evidence before backup or apply
- Codex, Git, source-cache, process, credential, and rollback boundaries fail
  closed on unreviewed or changed state
