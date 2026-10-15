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
- existing-import readiness fixtures with a kept Workshop-style descriptor,
  repeated replace paths, omitted/custom picture, and no external launcher;
  verify success-lock reevaluation, strict new-project checks, predecessor
  preserved-local consistency, all-stage faults and rollback-only failure
- dynamic manifest tests for changed skill/subagent trees, generic compatible
  IDs, newly published default-profile additions during Update, preservation of
  earlier optional choices, generic optional workflow UI, and the single
  canonical unversioned manifest filename
- reviewed post-install action tests for exact 3D arguments/evidence, optional
  missing-key/tool outcomes, successful readiness/lock persistence, failure
  recovery, and pre-apply resume using the production runner
- provider-account adapter tests for pinned package preparation, isolation,
  capability checks, cancellation, logout, and honest unsupported platforms
- security tests
- UI component and accessibility tests
- Windows, macOS, and Linux end-to-end tests
- performance tests
- release artifact verification

The stable Rust command is `pnpm test:rust`, which runs the workspace
all-feature suite through `scripts/run_rust_tests.mjs`. It defaults to four
Rust test threads because native tests start child processes; override
`HOI4_TEST_RUST_THREADS` only with an integer from 1 through 32. Keep this
bounded command as the normal native test entry point instead of asking
contributors to call an internal Cargo invocation directly.

## Required fakes

Provide deterministic fakes for:

- filesystem
- GitHub and HTTP source
- Git ref-advertisement source fallback and bounded source-resolution cache
- clock
- random and operation IDs
- credential store
- external process runner
- Codex App Server protocol and streamed notifications
- provider adapters, endpoint validation, bounded responses, and model profiles
- pinned Gemini/Grok account runtimes, trusted runtime identity, isolated homes,
  and provider CLI process boundaries
- Git
- MCP health server
- platform paths
- disk and permission failures

Tests must not require a real Meshy key, ChatGPT login, provider-account
credential, or paid provider call. A separately authorized account smoke is
manual evidence only and never an ordinary CI prerequisite.
External-launcher fixtures use canonical owned temporary parents, formatted
through `user_facing_path`, so macOS's `/var` alias does not weaken or trip the
production no-symlink checks. Linux validates native application and package behavior; test its XDG,
Secret Service, path, process, and default-profile policies explicitly alongside
Windows/macOS policies.
Tests that mutate process-global command state must take the shared test-state
guard so the default parallel Rust test runner cannot cross-contaminate evidence,
session, cancellation, credential-health, or analysis assertions.

Native acceptance chooses an apply control from the actual reviewed screen,
not only the operation type: conflict-free maintenance shares the dry-run
screen and Start installation button. Maintenance completion goes directly to
Ready without the first-install Continue step. Require the real completed
journal, readiness report, and original/repaired file hashes for both paths.

Candidate native acceptance resolves the public default branch at run start,
tests Latest setup, same-revision Repair, an explicit commit pin followed by
Latest, and Kaiserreich Update. Verify each resulting source/analysis binding
and MCP version against the exact public manifest. These same-package policy
transitions are not evidence of a live version-to-version MCP upgrade; synthetic
incoming-manifest and lock regressions cover changing package identities.

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
- coding-environment validation covers Claude Code's fresh default primary,
  exactly one primary, primary/additional filtering, every supported primary
  and multi-environment combination, and persisted selection migration; a
  missing primary in legacy state remains the Codex compatibility default
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
- the shared Claude catalog has 14 built-in entries with Haiku 5.5/medium as
  default and legacy Haiku 4.5 as one-effort; bounded exact unknown Claude IDs
  remain usable without assuming or forwarding an effort
- native Anthropic/Gemini/Grok schema payloads contain no tools or credential
  values; runtime Draft 2020-12 validation precedes typed deserialization,
  core assigns analysis UUIDs, empty evidence refs are valid for inference,
  unknown refs/registry IDs reject, and only serialization failures receive one
  corrective turn
- experimental account routes never read or persist provider tokens, use
  private empty workspaces with disabled tools/MCP/hooks/plugins, retain only
  approved-input provider sessions outside the project, and report macOS
  unsupported until its runtime/terminal path is verified
- MCP, 3D, `codex.config`, ChatGPT flattening, and Open in Codex remain available
  independently of a non-Codex setup assistant
- image-provider selection synchronizes only manifest-declared `imagegen.*`
  route components, removes stale route IDs on switch/manual selection, and
  leaves Rust dependency expansion authoritative; Live Tools selection keeps its
  `workflow.live_testing` dependency and provider pairing
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

## Integration tests

For a published Windows installer, use the remote-only `Released app E2E`
workflow and `pnpm test:released-e2e <scenario>`. The immutable artifact and mod
pins live in `tests/remote/released-app-pins.json`; read them instead of copying
a temporary release version into a skill. `scripts/run_released_e2e.mjs` drives
the unmodified WebView2 UI through Playwright CDP with real Rust IPC and real
transactions. Only the supported loopback planning endpoint is a deterministic
fixture; do not equate it with hosted authentication or model-quality evidence.
The new, Kaiserreich and interrupted-import scenarios verify original-file
hashes, persisted readiness, repair and rollback. Read
`docs/development/released-app-e2e.md` for the complete boundary. The runtime
guard refuses local/self-hosted execution. Keep all new harness checks under
`pnpm test:released-harness` and run them in CI when the desktop must remain free.

Elevated hosted Windows runners require an executable-specific WebView2 HKLM
override; refuse existing values and remove only exact owned values on shutdown.
Use setup-python's absolute interpreter with `-I -S`, an owned working directory
and a minimal environment for archive validation. Authenticate CRLF archive text
against its LF Git blob without changing the extracted bytes. Recompute approved
analysis-input hashes and sanitize every result field before artifact upload.
Candidate acceptance reuses the same driver only after native CI verification;
the same-workflow artifact must match all recorded hashes and exact build SHA,
version, product, architecture and bundle. Label this separately from signed
published-binary evidence. Historical released-app pins run manually and retain
known failures; never weaken those assertions to approve a candidate fix.
The hosted MCP diagnostic matrix compares clean supported Node/npm installs;
`HOI4_MCP_DIAGNOSTICS=1` retains public package hashes only. Never replace a
reviewed tree hash with a diagnostic result merely to make a test pass.

Use fake source clients for the rate-limit-only GitHub REST 403/429 fallback:
accept only the fixed repository's bounded `info/refs?service=git-upload-pack`
advertisement, bind `HEAD`/default branch and peeled release tags, and reject
unrelated 403s, other repositories/services, redirects, object downloads, and
checkout behavior. Cover the request-keyed resolution cache, 15-minute maximum
TTL, lower manifest TTL, exact revision/branch/manifest evidence, and
same-revision manifest/blob selection.

Exercise public selection from each selected component's `expected_files`
inventory without a recursive tree prerequisite. Reject missing, duplicate,
outside-component, size, SHA-256, undeclared-tree, and unresolved Git LFS
pointer evidence before staging. Treat image-generation provider/model
catalogs as manifest data: newly declared entries must load without app enum or
per-model schema changes, while selected route component closure remains Rust-
validated.

The provider matrix must cover native Anthropic/Gemini/Grok schema payloads,
runtime Draft 2020-12 validation, core-assigned analysis UUIDs, empty evidence
references for inference, unknown evidence/registry IDs, and one corrective
serialization turn only. Cover immediate findings-review pending state,
automatic image route-component synchronization, stale route removal, and the
manifest-declared Live Tools dependency/provider pairing in component tests.

For unpublished package validation, set `HOI4_MOD_SETUP_SOURCE_REPOSITORY` to
an absolute committed Agentic HOI4 Modding checkout, then run `pnpm test:offline`.
The script preserves installed Cargo/Rustup toolchain homes while isolating
application HOME/USERPROFILE/APPDATA/LOCALAPPDATA and caches, runs opt-in tests
serially with Cargo offline, and scans a synthetic 50,000-gameplay-file mod.
Preflight the public source history and fail fast before constructing the large
fixture. A reused fixture must be explicitly supplied and carry this runner's
synthetic marker. Profile folders do not isolate Credential Manager/Keychain:
the lifecycle fixture uses the test-only thread-local Meshy-vault override and
strips credential/proxy/Git/Codex-home environment influence. Keep verified
incoming bytes and their adapted result hashes for healthy Skip operations;
modified/merged skips need explicit Keep conflicts. Upgrade rollback must
assert that every newly created destination is absent, not only that old files
and the previous lock were restored.
Public 0.3.5 compatibility uses the unchanged, hash-bound manifest in
`docs/fixtures/`; never regenerate that historical fixture. The ignored
`offline_lifecycle::offline_all_component_install_import_repair_remove_and_rollback`
acceptance test in `src-tauri/src/commands_lifecycle_tests.rs` then exercises
planning, adaptation, staging, apply, readiness, repair, removal, and rollback
without real provider or external-installer calls. The ignored
`local_source_serves_every_declared_file` test uses a temporary
`HttpSourceClient::with_cache_root` so source verification never depends on an
ambient application cache. These are opt-in checks and should be reported as
such when they are not run.

## Fault injection

For every transaction operation, support controlled failure before and after the live mutation boundary. Verify journal state, recovery options, destination hashes, and absence of false success.

When transaction filesystem code changes, run the `safe_fs` link/reparse tests,
the transaction fault matrix, and `transaction_mutations_do_not_use_ambient_filesystem_apis`.
Add native Windows junction and macOS symlink swap barriers for every affected
ancestor and operation boundary. These tests currently establish ancestor-link
containment for selected paths; final-leaf races and root identity across
recovery remain separate required cases.

## UI tests

Run `pnpm test:e2e:install` once to prepare Playwright Chromium, then
`pnpm test:e2e` for the real headless-browser suite in `tests/browser/`.
Run `pnpm docs:screenshots` to refresh all 15 public captures through the same
headless suite; review the images and let Pages consume those exact files.
`HOI4_TEST_BROWSER_PATH` can select an existing local browser executable.
Keep the documentation-fixture screen loop, runtime-error and overflow checks,
keyboard environment selection, signed-out management, and recovery interaction
coverage. Browser fixtures do not prove native IPC, provider authentication,
or transaction success; report those layers separately. Keep Vitest discovery
limited to `src/**/*.test.{ts,tsx}` so it does not run Playwright tests.

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
messaging. Assert Claude's built-in catalog label/default and exact model-ID
field, the experimental account controls without credential fields, immediate
`Reviewing with <provider>…` pending feedback, automatic image-provider route
selection/stale-route removal, and the restored HOI4 Live Tools row/provider
state. Assert the **3D models workflow** title and the immediately following
**Super Events workflow** order. Recovery coverage must render a
validation-stage checkpoint with its sanitized Details disclosure and only the
core-permitted Continue/Discard actions before apply; feed prefixed,
quoted-field, and unquoted-assignment secret shapes plus multibyte overflow
through that path and prove the raw values are absent and the result remains
within 2 KiB. Direct transaction and rollback command errors use quoted-field
fixtures too. Never show Undo as an unavailable card.

Keep a desktop responsiveness regression test that verifies every Tauri command
uses `#[tauri::command(async)]` and long-running helpers use the Tauri blocking
pool so blocking Rust core work cannot run on the UI event loop; the source
regressions are `every_desktop_command_uses_the_async_dispatcher` and
`blocking_command_work_uses_the_async_runtime_blocking_pool`.

Use `pnpm test:mcp-live` for an opt-in Windows check of the exact MCP package in
the bundled source manifest. It isolates npm configuration, installs into a
disposable prefix, verifies registry and full-tree evidence, then performs only
`initialize` and `tools/list`. It never calls a tool and needs no provider
credential. The ordinary fake MCP health-server tests remain the CI gate.

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

Repository validation selects the supported remote manifest schema from the
canonical fixture's schema version. Keep legacy v3 and provider-contract v4
selection and unsupported-version rejection covered by `pnpm test:manifest`.
Browser tests import scenario names from `src/documentation-scenarios.ts`;
keep that module independent of bundled JSON and browser-only fixture code.

A release is blocked by:

- failing schema examples
- unresolved critical security finding
- failing transaction fault suite
- core new or existing flow failure on any supported platform
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
- launcher scaffold failure on any supported platform

On Windows, activate the MSVC environment with `vcvars64.bat` before
all-feature Rust gates when using PowerShell. Compile fuzz targets with
`cargo check --manifest-path fuzz/Cargo.toml --bins`; run the bounded targets
from `fuzz/README.md` when parser behavior changes. Use the repository-owned
scripts for validation and release gates: `pnpm validate`,
`pnpm test:workflows`, `pnpm test:a11y`, `pnpm test:e2e`,
`pnpm release:build`, `pnpm release:verify`, `pnpm desktop:e2e`, and
`pnpm installer:e2e`; use `pnpm test:release-assets` for the release curation
fixture. The curation test verifies that the platform dependency inventories
are unioned into internal `release-artifacts/` evidence, while preview
curation writes the corresponding union under `preview-artifacts/`; workflows
upload that evidence separately from public publication assets.
Windows/macOS signing verifies changed package bytes; Linux updater signing
authenticates both final package files. Every signing job regenerates its
complete internal artifact manifest, and curation cryptographically verifies
final updater bytes against the embedded public key. Use
`pnpm release:prepare` only for final publication-asset curation.
The native smoke harness resolves `taskkill.exe` through the native Windows
system-directory alias, never `PATH`, and uses an argument array plus a bounded
exit wait so the Tauri process tree cannot survive the test timeout. macOS DMG
tools use their fixed `/usr/bin` paths. Keep cleanup bounded and verify that no
test process remains after a failed run.
The installer harness operates only inside a fresh runner-temporary directory:
it silently installs and uninstalls the NSIS package on Windows, mounts,
copies, launches, removes, and detaches the DMG application on macOS, and
extracts, launches, and removes AppImage and Debian payloads on Linux.
On Windows it must refuse a pre-existing current-user product registration,
fail closed when registry inspection cannot be completed, refuse a matching
legacy per-machine installation, remove only registry values whose install
paths exactly match the current E2E root, and assert that neither product
registry key retains a test-owned path. Cleanup runs after success and ordinary
failure; never delete or replace unrelated installation metadata.

The browser harness builds development-only synthetic routes into the ignored
`artifacts/browser-fixture` directory, then serves that fixture on loopback
port 1421. It never reuses a running user app or enables fixtures in the normal
production output. Build readiness is awaited before browser tests start, so
Vite cold module compilation is not charged to the first wizard assertion.


## Linux package and protection tests

Linux x64 packages use the Ubuntu 22.04 native matrix and `appimage,deb` bundle
selection. Use `pnpm test:release-assets` for ELF/AppImage/Debian identity and
failure checks, startup/termination behavior, complete Linux updater targets,
missing package/signature rejection, changed signed bytes, and exact public
asset/dependency-evidence separation. CI also runs
`python -m unittest scripts/test_linux_release_gates.py` to reject missing
native gates and unsuccessful or foreign-app main-protection evidence.

Linux core-only fuzz jobs install `pkg-config` and `libdbus-1-dev` before
compilation because the native Secret Service adapter is part of the core.
Native Linux CI, preview, and release run the isolated credential smoke through
`env -u HOI4_TEST_IMAGE_VAULT_ADAPTER bash scripts/test_linux_credentials.sh`.
Require the exact ignored test to exist and execute; a zero-test success is not
evidence. The harness uses temporary HOME/XDG, D-Bus, and Secret Service state,
and a synthetic value. Workflow regressions reject absent native prerequisites,
wrong install ordering, missing vault gates, and external source-adapter
inheritance. Local source-Python crosschecks remain explicitly opt-in.

Native Linux smokes run under Xvfb and D-Bus with temporary XDG application data
and supervised process-group cleanup. Local smokes extract both packages;
only disposable CI sets `HOI4_MOD_SETUP_DEDICATED_LINUX=1` for actual `dpkg`
installation/purge, existing-registration refusal, database fail-closed
preflight, and final absence proof. Never enable this flag on a shared host.

Stable Linux signing uses the protected Tauri updater key for both AppImage
and Debian files, with no invented OS-signing route. Require cryptographic
verification against the committed public key. Metadata contains
`linux-x86_64-appimage`, `linux-x86_64-deb`, and the AppImage
`linux-x86_64` fallback; test that Debian gets its own package and signature.
Native packaging does not prove real launcher/game loading, completed account
login, a desktop Secret Service round trip, or replacement/restart/retry.

## Update this skill when

Update this skill when test commands, fixture layout, fake interfaces, property invariants, fault injection, platform matrix, performance thresholds, accessibility checks, or release gates change.

Provider tests cover Windows, macOS, and Linux planning, selected-client registration, explicit local and remote Clausewitz capability, provider replacement and removal, preserved custom routes, rollback, and incomplete optional runtime outcomes. Browser coverage uses synthetic app states and blank credential fields; it does not establish real provider or game behavior.
