---
name: hoi4-mod-setup-testing
description: Use for test architecture, fixtures, fake adapters, property tests, fuzzing, transaction fault injection, security tests, platform end-to-end tests, performance tests, or release gates.
---

# Testing and completion evidence

## Required sources

Read:

- `AGENTS.md`
- `docs/20_testing_strategy.md`
- `docs/22_acceptance_criteria.md`
- owning product skill
- relevant schemas and examples

## Test layers

Use the smallest useful layer and keep high-risk behavior covered at more than one layer.

- unit tests for deterministic domain functions
- property tests for invariants
- fuzzing for parsers and hostile input; the checked-in cargo-fuzz targets cover
  manifests, relative paths, Codex analysis payloads, descriptors/thumbnail
  PNGs, and structured TOML merges
- integration tests for adapters and cross-module behavior
- transaction fault injection
- security tests
- UI component and accessibility tests
- Windows and macOS end-to-end tests
- performance tests
- release artifact verification

## Required fakes

Provide deterministic fakes for:

- filesystem
- GitHub and HTTP source
- clock
- random and operation IDs
- credential store
- external process runner
- Codex App Server protocol and streamed notifications
- Git
- MCP health server
- platform paths
- disk and permission failures

Tests must not require a real Meshy key or paid provider call.

## Property examples

- normalized destinations remain inside the approved root
- apply followed by rollback restores original hashes
- verified operations are idempotent or reject an invalid replay
- managed removal never deletes unowned content
- secret-like values never survive serialization
- one plan revision produces one file set
- scan produces no project mutation
- no launcher artifact is generated before confirmed Codex proposals
- App Server account data and tokens never survive serialization
- an approved process receives only `MESHY_API_KEY`, while known secret values are absent from both output streams and serialized artifacts
- an MCP health probe accepts only the manifest-declared Windows wrapper,
  completes initialize and read-only `tools/list` validation within its bound,
  never invokes a tool, and leaves no child process running
- update planning rejects a missing fresh Codex reanalysis record, while repair
  remains able to use the validated locked analysis; the reanalysis evidence
  scan remains read-only and its approved references are bound to the core
  session before transmission

## Fault injection

For every transaction operation, support controlled failure before and after the live mutation boundary. Verify journal state, recovery options, destination hashes, and absence of false success.

## UI tests

Test all 17 required screen states and seven phases. Include density assertions, keyboard traversal, scaling, reduced motion, long values, errors, conflict comparison, staged scanner progress, correlated event filtering, indeterminate progress semantics, and cancellation evidence messaging.

## Platform matrix

At minimum:

- Windows x64
- macOS Apple Silicon
- macOS Intel only while supported
- case-insensitive filesystem
- case-sensitive macOS volume fixture
- local and cloud-synced path fixtures

Unsupported external workflows must be tested as honest non-blocking states.

## Release gates

A release is blocked by:

- failing schema examples
- unresolved critical security finding
- failing transaction fault suite
- core new or existing flow failure on either supported platform
- credential leakage
- fake optional success
- inaccessible core UI
- unsigned or unverified stable artifacts when signing is required
- missing license for public release
- failing ChatGPT authentication or Codex analysis contract tests
- launcher scaffold failure on either supported platform

On Windows, activate the MSVC environment with `vcvars64.bat` before
all-feature Rust gates when using PowerShell. Compile fuzz targets with
`cargo check --manifest-path fuzz/Cargo.toml --bins`; run the bounded targets
from `fuzz/README.md` when parser behavior changes; package and smoke-test
through `pnpm release:build`, `pnpm release:verify`, and `pnpm desktop:e2e`.
The native smoke harness uses `taskkill.exe` with an argument array on Windows
so the Tauri process tree cannot survive the test timeout; keep cleanup bounded
and verify that no test process remains after a failed run.

## Update this skill when

Update this skill when test commands, fixture layout, fake interfaces, property invariants, fault injection, platform matrix, performance thresholds, accessibility checks, or release gates change.
