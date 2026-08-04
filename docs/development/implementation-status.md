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
- Automatic signed startup update checks that download, verify, replace, and
  restart into the latest version, with retry after failure.
- Existing-project ChatGPT source packaging with a Downloads default, required
  flattened-source selection, optional root Markdown, safe atomic ZIP output,
  and no project mutation.
- Sanitized development-only documentation fixtures for consistent public
  screenshots of the current wizard and maintenance flow.

## Current optional workflow and responsiveness behavior

- `workflow.super_events` is a provider-neutral optional workflow resolved from
  the verified manifest and selectively installed at
  `.agents/skills/hoi4-super-events/`. Its no-credential, non-blocking state,
  lock/scan memory, ordered optional workflow titles, unselected AGENTS
  guidance rule, and Update/Repair source boundary are documented across the
  product surfaces.
- Every Tauri desktop command is required to dispatch filesystem, network, Git,
  and provider waits asynchronously through a thread pool, with a regression
  test for event-loop responsiveness.

The Rust and React regression suites cover fresh selection, later maintenance,
remembered state, selected and unselected AGENTS adaptation, readiness, and
desktop command dispatch away from the UI event loop.

## Current source and release boundary

The Agentic HOI4 Modding default branch, provider-specific portrait workflows,
one-skill Super Events workflow, and current published manifest are at commit
`3f0c61d9891344197cab38ab5e760821134f6daf`. The 24-component manifest declares
the selected-file records generated from immutable Git blobs at revision
`8f01184fbf6c4ebce6d850e17d752925b76d6182`.

The release route produces a ChaosX Authenticode-signed Windows installer and
ad-hoc signed macOS disk images when official credentials are absent. The same
workflow uses Azure Artifact Signing and Apple Developer ID/notarization when
those credentials are configured. The app does not invent a macOS route for
the source repository's Windows-oriented 3D or MCP steps. A missing Meshy key
remains optional and non-blocking.

The ComfyUI HOI4 portrait workflow is integrated as an optional
Cloud, Local, RunPod, or Disabled project capability. Provider selection and
the exact upstream revision `b47222a77f2f6454704530865aa1441fad48bdd3` are
persisted without secrets; Cloud MCP registration, bounded local discovery and
workflow installation, RunPod guidance, source-based fallback, and disabled
cleanup are covered by the portrait pipeline contract in
`docs/32_comfyui_portrait_pipeline.md`.

The application version is declared in `package.json`, Tauri configuration, and
the Rust package metadata. The release workflow builds the current Windows x64
installer and macOS arm64/x64 disk images from the exact annotated tag, then
publishes updater metadata and both macOS updater archives after verification.
