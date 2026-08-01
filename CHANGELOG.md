# Changelog

HOI4 Mod Setup follows semantic versioning.

## Unreleased

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
