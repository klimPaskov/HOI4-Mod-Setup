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
- descriptor regression tests must use identities with a script prefix and primary namespace and prove that neither unsupported key is emitted into the internal or launcher descriptor
- fuzzing for parsers and hostile input; the checked-in cargo-fuzz targets cover
  manifests, relative paths, provider analysis payloads, descriptors/thumbnail
  PNGs, structured TOML merges, and flattened ChatGPT-source mappings
- integration tests for adapters and cross-module behavior
- transaction fault injection
- dynamic manifest tests for changed skill/subagent trees, generic compatible
  IDs, newly published default-profile additions during Update, preservation of
  earlier optional choices, generic optional workflow UI, and the single
  canonical unversioned manifest filename
- reviewed post-install action tests for exact 3D arguments/evidence, optional
  missing-key/tool outcomes, successful readiness/lock persistence, failure
  recovery, and pre-apply resume using the production runner
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
- provider adapters, endpoint validation, bounded responses, and model profiles
- Git
- MCP health server
- platform paths
- disk and permission failures

Tests must not require a real Meshy key, ChatGPT login, or paid provider call.
Tests that mutate process-global command state must take the shared test-state
guard so the default parallel Rust test runner cannot cross-contaminate evidence,
session, cancellation, credential-health, or analysis assertions.

## Property examples

- normalized destinations remain inside the approved root
- selected starter directories are created without `.gitkeep` files, and
  rollback removes empty transaction-created directories while preserving
  later user content
- apply followed by rollback restores original hashes
- per-operation checkpoint size stays bounded as the plan grows, replay restores the latest durable operation state, and compaction preserves that state in the full journal
- batched apply and rollback intents cover every possible live mutation before it starts, while interrupted groups reconcile safely from backups and observed hashes
- verified operations are idempotent or reject an invalid replay
- managed removal never deletes unowned content
- secret-like values never survive serialization
- one plan revision produces one file set
- coding-environment validation covers Codex's default primary, exactly one
  primary, primary/additional filtering, every supported primary and
  multi-environment combination, and persisted selection migration
- native package fixtures parse Codex, Claude Code, Cursor, Qoder, and OpenCode
  settings/MCP files and canonical agent projections; drift, missing files, and
  malformed runtime configuration fail readiness
- existing-project detection plus update, repair, reinstall, rollback, and
  removal preserve modified/unmanaged client files and remove only unchanged
  managed files when an environment is deselected
- selected manifest components and dependencies are the only downloaded files;
  an unselected `workflow.super_events` tree cannot enter the ledger or stage
- manifest-declared `.gitkeep` markers never enter the selected file set
- scan produces no project mutation
- no launcher artifact is generated before confirmed selected-provider proposals
- standard Documents/mod resolution and launcher discovery reject links,
  malformed candidates, collisions, and ambiguous sibling registrations
- launcher scaffold coverage asserts that Windows launcher descriptors use the
  canonical user-facing project path without an internal `\\?\` prefix,
  reject a different complete path in the full transaction before live apply,
  and that a validation-stage interruption
  resumes through final readiness and the success lock; macOS coverage keeps a
  literal backslash in a filename component while matching the canonical root
  when a temporary directory uses the `/var` system alias
- the UI and Rust plan builder both block missing, unauthenticated, or
  usage-limited provider capability while signed-out recovery remains usable
- reviewed external-link commands accept only the fixed URLs and use the
  platform system-browser bridge
- provider/model/reasoning-effort/profile stays bound from the start gate through analysis, plan,
  lock, and readiness as setup-analysis provenance, while generated guidance,
  development components, Open in Codex, and flatten export remain independent
- live model catalogs are bounded, credential-safe, and drive only supported
  reasoning options; unavailable catalogs retain an editable verified fallback
- MCP, 3D, `codex.config`, ChatGPT flattening, and Open in Codex remain available
  independently of a non-Codex setup assistant
- direct Open in Codex rejects a missing installed Codex configuration, uses
  fresh local readiness, and remains available after setup-provider sign-out
- the actual `.hoi4-mod-setup/state.json` path is schema-validated and carries
  provider, model, reasoning effort, and optimization-profile provenance
- provider-neutral `workflow.super_events` selection is represented by its
  parent manifest/component ID, expands to the complete hidden runtime
  dependency closure, namespace-adapts only verified text files, leaves
  binary DDS/PSD bytes unchanged, is isolated from unselected downloads, adds
  no unselected AGENTS guidance, and remains non-blocking in readiness
- every selected Codex subagent either already declares the bounded spawn rule
  or is deterministically adapted to contain `fork_context=false`; an explicit
  true declaration is rejected, and the adapted TOML remains parseable
- adapted AGENTS output contains no template-only Placeholder Guide text,
  preserves the first real instruction section across LF and CRLF templates,
  and supplies those same cleaned bytes to flattened Chat sources
- flattening rejects output collisions, traversal, links, secret-shaped paths,
  and secret-shaped content while preserving the source transaction boundary;
  large selected wiki trees and other ineligible component files neither enter
  the flat output nor consume its file and size limits
- existing-project ChatGPT export discovers only root AGENTS/README, direct
  skill SKILL.md files, direct TOML subagents, and immediate root Markdown;
  it is eligible without an installation lock when an AGENTS, skill, or
  subagent source exists. Required/default selection, optional Markdown,
  Downloads defaults, stale selections, archive limits/collisions, atomic
  no-overwrite output, no-secret behavior, and unchanged project contents are
  tested
- App Server account data and tokens never survive serialization
- login cancellation targets one validated App Server `loginId`, calls the
  managed cancel method, and cannot clear or cancel another active attempt
- remote manifest parsing applies the authoritative Draft 2020-12 schema before
  typed deserialization and rejects unknown nested policy fields
- component recommendation payloads accept only the checked-in
  `codex-analysis.schema.json` shape and deterministic registry IDs, including
  `workflow.super_events`
- read-only Git inspection rejects hostile local configuration before spawn,
  suppresses ambient Git behavior, and never recurses into submodules
- Portrait workflow tests cover Cloud, Local, RunPod, and Disabled selection,
  non-secret persistence, disabled marker/file cleanup, Cloud MCP registration,
  bounded local discovery, the two authorized workflow hashes and adaptive
  crop-node hashes, RunPod guidance, source/prompt pairing, fallback status,
  readiness separation, the 16 GB VRAM/25 GB storage recommendation, and
  no-spend/no-secret behavior. Use fakes and fixtures; do not start paid
  provider resources.
- an approved process receives only `MESHY_API_KEY`, while known secret values are absent from both output streams and serialized artifacts
- an MCP health probe accepts only the manifest-declared Windows wrapper,
  completes initialize and read-only `tools/list` validation within its bound,
  never invokes a tool, and leaves no child process running
- update planning rejects a missing fresh selected-provider reanalysis record, while repair
  remains able to use the validated locked analysis; the reanalysis evidence
  scan remains read-only and its approved references are bound to the core
  session before transmission
- source-selector changes invalidate confirmed analysis and return new and
  existing projects to their respective review screens; maintenance analysis
  derives pinned source details from the installed lock, and legacy analysis
  binding copies only valid same-lock source provenance while rejecting absent
  or malformed evidence
- Repair uses only the immutable locked source and rejects a component absent
  from it; Update resolves the newer source required to add that component
- the managed lock, completed scan context, and readiness result remain the
  remembered state across review/maintenance until an explicit refresh or
  transaction changes them
- an available signed app update starts once on startup, a duplicate startup
  check cannot start a second install, and a failed install preserves setup and
  exposes a retry

## Fault injection

For every transaction operation, support controlled failure before and after the live mutation boundary. Verify journal state, recovery options, destination hashes, and absence of false success.

## UI tests

Public README captures use the sanitized, development-only scenarios in
`src/documentation-fixtures.ts`. Tests prove known routes return synthetic
state without account identity or secret values, unknown routes are ignored,
and production builds cannot activate the fixture. Capture at 1280 by 960 from
the top of the page so comparison evidence is consistent.

Test all 18 required screen states and seven phases. The dedicated Coding
Environments screen is part of that required set; assert its bounded option
layout and visible focus-within styling so the accessibility gate cannot omit
it. Include density assertions, keyboard traversal, scaling, reduced motion,
long values, errors, conflict comparison, staged scanner progress, correlated
event filtering, indeterminate progress semantics, and cancellation evidence
messaging. Assert the **3D models workflow** title and the immediately following
**Super Events workflow** order. Recovery coverage must render a
validation-stage checkpoint with its sanitized Details disclosure and only the
core-permitted Continue/Discard actions before apply; feed prefixed,
quoted-field, and unquoted-assignment secret shapes plus multibyte overflow
through that path and prove the raw values are absent and the result remains
within 2 KiB. Direct transaction and rollback command errors use quoted-field
fixtures too. Never show Undo as an unavailable card.

Keep a desktop responsiveness regression test that verifies every Tauri command
uses `#[tauri::command(async)]` so blocking Rust core work cannot run on the UI
event loop; the current source-level test is
`every_desktop_command_uses_the_async_dispatcher`.

## Platform matrix

At minimum:

- Windows x64
- macOS Apple Silicon
- macOS Intel only while supported
- case-insensitive filesystem
- case-sensitive macOS volume fixture
- local and cloud-synced path fixtures

The CI dependency install uses pnpm 11's strict build policy. Keep
`pnpm-workspace.yaml` explicit (`allowBuilds.esbuild=true`) and add a reviewed
allow-list entry whenever a dependency's install script is intentionally
required; never replace it with an allow-all setting.

Unsupported external workflows must be tested as honest non-blocking states.

Large scanner regressions prove gameplay/media/generated corpora are pruned
before inventory and exercise bounded targeted detector text separately.
Exercise the 150,000-targeted-file, 200,000-directory,
64-level, 64-MiB aggregate path, 50,000-entry/8-MiB directory-sort, 256-MiB
detector-byte, and ten-minute boundaries with small override fixtures where
practical. A real Git inspection fixture must remain complete while its project
root and `.git` directory are retained by identity-checked handles; linked,
replaced, or executable local configuration must return partial/unsafe evidence
without starting a helper process.

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
- updater metadata missing a platform, using a wrong release URL, carrying an
  empty signature, or referencing bytes changed after updater signing
- checkout-free publication jobs that run `gh release` without an explicit
  `--repo "$GITHUB_REPOSITORY"` on every read and write
- failing ChatGPT authentication, provider adapter, or common analysis contract tests
- launcher scaffold failure on either supported platform

On Windows, activate the MSVC environment with `vcvars64.bat` before
all-feature Rust gates when using PowerShell. Compile fuzz targets with
`cargo check --manifest-path fuzz/Cargo.toml --bins`; run the bounded targets
from `fuzz/README.md` when parser behavior changes. Use the repository-owned
scripts for validation and release gates: `pnpm validate`,
`pnpm test:workflows`, `pnpm test:a11y`, `pnpm test:e2e`,
`pnpm release:build`, `pnpm release:verify`, `pnpm desktop:e2e`, and
`pnpm installer:e2e`.
The platform signing jobs verify changed package bytes and regenerate their
complete internal artifact manifest before curation. Use
`pnpm release:prepare` only for final publication-asset curation.
The native smoke harness resolves `taskkill.exe` through the native Windows
system-directory alias, never `PATH`, and uses an argument array plus a bounded
exit wait so the Tauri process tree cannot survive the test timeout. macOS DMG
tools use their fixed `/usr/bin` paths. Keep cleanup bounded and verify that no
test process remains after a failed run.
The installer harness operates only inside a fresh runner-temporary directory:
it silently installs and uninstalls the NSIS package on Windows, and mounts,
copies, launches, removes, and detaches the DMG application on macOS.
On Windows it must refuse a pre-existing current-user product registration,
fail closed when registry inspection cannot be completed, refuse a matching
legacy per-machine installation, remove only registry values whose install
paths exactly match the current E2E root, and assert that neither product
registry key retains a test-owned path. Cleanup runs after success and ordinary
failure; never delete or replace unrelated installation metadata.

## Update this skill when

Update this skill when test commands, fixture layout, fake interfaces, property invariants, fault injection, platform matrix, performance thresholds, accessibility checks, or release gates change.
