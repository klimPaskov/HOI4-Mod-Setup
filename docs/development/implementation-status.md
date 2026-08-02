# Implementation status

This document is the concise developer status. User guidance stays in the root
`README.md`.

## Implemented

- Tauri desktop shell with a Rust core and React TypeScript seven-phase wizard.
- New-mod generation and existing-mod read-only scanning with editable,
  evidence-backed findings.
- Codex App Server login/logout and schema-constrained analysis, plus explicit
  hosted and local provider profiles.
- Versioned remote manifest resolution, selective downloads, immutable source
  revision binding, verified cache reads, and offline wiki installation.
- Twelve-stage transactions with dry run, delayed backup/staging creation,
  operation-bound download evidence, interruption recovery, rollback, repair,
  reinstall, and managed removal.
- Windows Credential Manager and macOS Keychain boundaries for provider and
  Meshy credentials.
- Git initialize/preserve/skip, guarded online push, and separately approved
  public GitHub-repository creation.
- Optional Codex-only flattened ChatGPT project sources without offline-wiki
  files, shown with filenames and sizes before installation.
- Rust unit/property tests, fuzz targets, transaction fault injection,
  frontend/accessibility tests, workflow authority checks, and Windows/macOS
  native build and launch-smoke workflows.
- User-facing release curation that publishes only the Windows installer and
  two macOS disk images.
- Automatic signed application-update checks and an in-app update action.
- Sanitized development-only documentation fixtures for consistent public
  screenshots of the current wizard and maintenance flow.

## Current optional workflow and responsiveness behavior

- `workflow.super_events` is a provider-neutral optional workflow resolved from
  the verified manifest and selectively installed at
  `.agents/skills/hoi4-super-events/`. Its no-credential, non-blocking state,
  lock/scan memory, ordered Optional workflows question, unselected AGENTS
  guidance rule, and Update/Repair source boundary are documented across the
  product surfaces.
- Every Tauri desktop command is required to dispatch filesystem, network, Git,
  and provider waits asynchronously through a thread pool, with a regression
  test for event-loop responsiveness.

The Rust and React regression suites cover fresh selection, later maintenance,
remembered state, selected and unselected AGENTS adaptation, readiness, and
desktop command dispatch away from the UI event loop.

## Current source and release boundary

The Agentic HOI4 Modding default branch, conditional Super Events guidance,
and current published manifest are at commit
`bdef773758125b367fff3ab40d85433675d511cb`. The manifest declares selected-file
evidence generated from immutable Git blobs at revision
`8c619f7dfee59f50096573e4ed1ea200e06632be`.

The release route produces a ChaosX Authenticode-signed Windows installer and
ad-hoc signed macOS disk images when official credentials are absent. The same
workflow uses Azure Artifact Signing and Apple Developer ID/notarization when
those credentials are configured. The app does not invent a macOS route for
the source repository's Windows-oriented 3D or MCP steps. A missing Meshy key
remains optional and non-blocking.

LoRA and ComfyUI are not setup state. A successful Ready screen links to the
separate portrait workflow repository.
