# Testing strategy

Use the repository script `pnpm test:rust` for the normal Rust workspace test
gate. It uses four test threads by default; `HOI4_TEST_RUST_THREADS` may set an
integer from 1 through 32 for a local run. The script rejects values outside
that range before invoking Cargo.

External-launcher fixtures use canonical temporary parents and user-facing
Windows paths. A macOS `/var` alias must not bypass production link rejection.
Linux x64 is a native application and release test target. Exercise its XDG
application-data, path containment, process, Secret Service, and launcher policies
explicitly alongside Windows/macOS policies. Bundled source JSON keeps exact upstream Git bytes through `-text`
attributes; compare its committed SHA-256 with the source audit on every host.

## AI provider integration matrix

Use a protocol fixture that emulates App Server JSONL without real account credentials in ordinary CI. Cover process absence, incompatible versions, initialize ordering, an existing ChatGPT session, browser login, exact per-`loginId` `account/login/cancel` behavior, isolated concurrent cancellation, device-code fallback, logout, account updates, usage limits, App Server crash, turn cancellation, output schema acceptance and rejection, proposal key/value type binding, unexpected fields, deterministic rejection of bad identifiers, token and account-data absence, and recovery while signed out. Add fake Anthropic, OpenAI-compatible, and loopback adapters for configured, missing-key, malformed-response, endpoint, redirect, bounded-response, provider-switch, and no-secret-persistence cases.

`pnpm test:codex-live` can verify a signed local App Server initialize/account
read plus browser/device login start-and-cancel using a temporary Codex home.
It does not complete browser authentication and never prints account or login
details; run the clean browser/device completion gate only with a developer
present. Never place real credentials in CI.

Claude model tests must validate the shared 14-entry catalog, Haiku 5.5/medium
as the default, legacy Haiku 4.5 as a one-effort choice, and bounded exact
model IDs entered under Advanced without requiring an app release. Built-in
choices must remain available when no live catalog exists.

Gemini and Grok account-adapter tests use fake terminal/process boundaries and
never request a user's credential. Verify explicit-sign-in package download,
SRI/archive/member digests, trusted Node verification for Gemini, isolated
provider homes, empty workspaces, disabled tools/MCP/hooks/plugins, provider-
owned session persistence boundaries, logout, cancellation, timeout, redacted
errors, and unsupported macOS handling. A separately authorized manual smoke
may use a real account; ordinary CI must not.

## Unit tests

- path normalization and containment
- Windows, macOS, and Linux native Documents resolution, including redirected
  Documents, missing locations, and explicit path overrides
- project ID validation
- descriptors
- authoritative Draft 2020-12 manifest schema before typed deserialization,
  including unknown nested-field rejection and the closed executable,
  interpreter, and runtime SHA-256/size identity parameter set
- dependency graph and cycles
- source-owned manifest generation over declared component trees, including a
  changed skill, new skill, new subagent, and unchanged-file deterministic
  output; compatible new default-profile components are adopted on Update
  without restoring previously declined optional choices
- platform resolution
- documentation fixtures preserve manifest-declared Windows-only MCP and 3D
  routes instead of presenting them as all-platform
- hashing
- file classification
- bounded project profiling: separate metadata counts and samples, content
  categories, path-hash sampling, four-per-category fairness, 256-sample and
  8 MiB aggregate caps, 32 KiB prefix-only hashes, metadata-only binaries, and
  explicit sampled/truncated coverage
- profile findings for observed namespaces and identifier prefixes, actual
  localisation language headers/BOM/versioned quoted-key style, and redacted
  documentation headings/short overviews without project-wide convention
  inference
- three-way and TOML merge
- generic manifest-declared optional workflows render/select without an app
  enum, while provider/platform/dependency gates remain enforced
- selected 3D configuration contains exact Meshy and bounded Blender routes;
  the reviewed bootstrap action carries fixed arguments, network/write/
  privilege/rollback evidence, and transaction tests prove success, optional
  incomplete outcomes, failure recovery, and production resume wiring
- secret redaction
- readiness aggregation
- journal transitions
- provider profile and model binding
- model-catalog failure and empty-result fallback for every provider profile,
  including editable Local and Custom model controls
- native structured-output payloads for Anthropic, Gemini, and Grok, including
  `output_config.format`, `response_format.json_schema`, and Responses
  `text.format`; the payloads must contain no tools or credential values
- runtime schema validation after native output, core-assigned analysis UUIDs,
  empty evidence references for brief-only proposals, unknown-reference
  rejection, and the distinction between corrective serialization errors and
  private/truncated/refused/transport output that must not be retried
- setup-provider isolation from generated guidance, development-client component
  selection, flattened ChatGPT packaging, and opening the selected client
- coding-environment selection uses the manifest default (currently Claude Code)
  for new installs, validates exactly one primary, filters the primary from
  additional choices, and resolves every primary/additional combination against
  the published manifest
- complete native package parsing for every registry client, the registered
  MCP route in each project MCP format, the legacy OpenCode structural check,
  and format 6 availability gating; canonical Claude-file-to-projection
  synchronization and drift checks
- reverse merge of the shared MCP route in every format on removal, deselection,
  and Update, creation evidence, component retirement, and a route-removal
  fault matrix with rollback to the original bytes
- hosted candidate acceptance (`clients`): update a Latest install to the
  pinned format 6 source, add every client while merging a pre-existing Kiro MCP
  file, deselect clients while keeping a modified agent and a user MCP server,
  assert that the unchanged Kiro MCP file loses only the shared route, repair a
  deleted projection, and refuse an older format that lacks the recorded clients
- existing-project environment detection, preservation of a recorded Codex or
  other primary, compatibility migration when no primary is recorded,
  managed-file preservation, deselection removal rules, and persisted selections
  across update, repair, reinstall, rollback, and removal
- flattened Chat-source mapping and size/collision policy
- automatic image-provider route-component synchronization, stale image-route
  removal, and Rust dependency-closure authority
- manifest-declared HOI4 Live Tools row, its live-testing dependency/provider
  state, experimental Windows-only status, and honest unsupported-platform UI
- read-only scan exclusion of virtual environments, dependency trees, editor
  metadata, caches, generated artifacts, app-managed `.tools/`, `.tmp/`, and
  offline-wiki pages while retaining valid HOI4 files

## Property tests

- normalized destinations never escape root
- apply then rollback restores hashes
- verified operations are idempotent
- removal never deletes unowned files
- secret-like values never survive serialization
- provider credential references remain keyed and never cross provider switches
- flattening is deterministic and rejects links, collisions, secret-shaped content, and oversized files
- selecting `workflow.super_events` downloads only its verified manifest tree,
  keeps the provider-neutral component credential-free, records its state in the
  lock and scan summary, and adds no Super Events guidance when unselected

## Fuzzing

Descriptor syntax, thumbnail PNG decoding, TOML parsing and structured merge,
manifest JSON, provider/Codex analysis JSON, relative paths, flattened
Chat-source mappings, Markdown link extraction, localisation keys, encodings,
huge files, and deep trees. The checked-in `fuzz/` package provides bounded
`descriptor`, `toml_merge`, and `flatten` targets in addition to the manifest,
path, and Codex targets.

## Integration tests

- Command tests that mutate process-global session, evidence, cancellation,
  health, or analysis stores must use a shared test-state guard so default
  parallel runs remain deterministic.

- fake GitHub latest and pinned server
- unauthenticated GitHub REST rate-limit-only 403/429 fallback to the fixed
  repository's read-only Git ref advertisement; reject unrelated 403s,
  redirects, other repositories, object downloads, and checkout behavior
- manifest and tree mismatch
- changed default branch
- partial download and resume
- checksum failure
- cache corruption
- source-resolution memory-cache expiry, lower manifest TTL, request-key
  isolation, and same-revision manifest/raw-file evidence
- public file selection from manifest `expected_files`, including missing,
  duplicate, outside-component, size, SHA-256, and undeclared-tree cases
- Windows Credential Manager mock
- macOS Keychain mock
- Linux Secret Service mock and missing/locked/unavailable service failures
- Git fixtures with ambient configuration isolation, hostile local config
  rejection before spawn, and direct bounded submodule discovery
- MCP test server
- opt-in `pnpm test:mcp-live` on Windows: install the exact manifest-pinned
  package into a disposable prefix, validate integrity and full-tree evidence,
  then require the initialize handshake and all required `tools/list` names;
  never invoke a tool or use a provider credential
- launcher descriptor outside project
- bounded parent-level launcher descriptor discovery, explicit pre-scan
  confirmation/decline/cancel, candidate mismatch, and no read of an
  unconfirmed candidate
- project-profile synthetic fixtures for read-only behavior, cancellation,
  linked entries, hidden/cache/tooling/wiki/secret-like exclusions, very large
  files, deep trees, per-directory limits, profile truncation, and no script,
  Git, link-following, or content-filter execution
- a native remote Kaiserreich fixture proving useful bounded gameplay,
  localisation, identifier, and documentation observations without turning the
  existing setup scan or Git pathspec into a whole-repository review
- scan-to-provider evidence approval with the core-owned normalized model
  excerpt, absolute-launcher-path redaction, stable excerpt hashes despite UI
  ordering, no raw gameplay input, and cancellation/root-binding invalidation
- provider API envelopes and bounded response bodies
- flattened canonical skill names, required project documents, and subagent names
- Super Events manifest selection, exact `.claude/skills/hoi4-super-events/`
  destination, `core.skills` dependency, no-credential plan, and deselected
  exclusion from the core skills tree and from canonical instruction guidance

## Transaction fault injection

Fail at every stage and operation boundary through process kill, disk full, permission denied, file lock, network loss, checksum mismatch, command timeout, health failure, and cancellation. Verify recovery and final hashes. Rollback tests must also assert a separate child journal, parent linkage, inverse backup of live post-transaction bytes, durable parent/child rollback records, safe reuse of the child identity on retry, and inverse rollback refusal after a later user file or lock edit.

For an absent new-project root, inject failure before apply, at leaf creation,
after managed files are applied, and during rollback. Assert that exactly one
reviewed leaf is created only at apply, that a pre-apply race stops safely, and
that rollback removes the leaf only when empty while preserving unknown content
and its parent.

Transaction filesystem regressions must run natively on Windows, macOS, and Linux.
Cover link and reparse swaps at every ancestor, root/transaction/backup/staging
replacement, journal and checkpoint updates, apply and rollback, external
launcher parents, executable metadata, and recursive staging discard. Use
deterministic barriers after root/parent acquisition and immediately before
namespace changes. Assert outside sentinels remain byte-for-byte unchanged,
root identity drift cannot produce a success lock, and displaced user bytes
remain recoverable. The source regression catches selected direct ambient
filesystem calls; it does not prove helper wrappers use the same verified root
handle. Current `safe_fs` unit tests cover static link leaves, linked-root
acquisition, ancestor swaps, and recursive staging links on Windows and Unix;
they do not cover concurrent final-leaf changes or macOS native execution.

## End-to-end cases

### New Windows project

Resolve redirected Documents, auto-fill the project root and launcher descriptor
from the project ID, preserve an explicit override, create the reviewed leaf at
apply, generate descriptors, install core, initialize Git, run readiness, and
open the project.

### Existing Windows project

Scan local CLAUDE/AGENTS instructions, canonical or legacy skills and subagents,
and native client files; run the bounded project profile; review, merge, install,
update, repair, and roll back. Assert that profile counts/samples remain
separate from setup-file inventory and that sampled coverage is not presented as
exhaustive gameplay validation.

### Coding-environment matrix

Run fresh Windows setups with each of the five supported primary environments,
plus at least one multi-environment setup. Repeat the Core and Core plus 3D
profile cases, then exercise existing-project migration, update, repair,
reinstall, rollback, and removal. Assert that every native package is complete,
that modified files are preserved, and that only unchanged managed files are
removed when a client is deselected.

Verify that the packaged Windows native executable launches as a GUI-subsystem
application without opening a console window, including when started from its
installed shortcut rather than an active terminal.

### New macOS project

Install platform-neutral core, report unsupported current Windows-only MCP and 3D routes, and keep core usable.

### Missing Meshy key

Select 3D, omit key, complete core, verify incomplete optional status, configure later, and rerun health.

### Super Events workflow

On the Optional workflows screen, verify the title order **3D models workflow**,
**Super Events workflow**, then **ComfyUI portrait production**. Test selected installation from one
verified manifest revision, the managed skill destination, readiness and lock
state, and the read-only scan summary. Test a declined install for absence of
the skill tree and Super Events-specific `CLAUDE.md`/`AGENTS.md` guidance. Test Update adding
the component from a target manifest and Repair adding it only when the locked
manifest at the same immutable revision declares it; otherwise Repair must
direct the user to Update.

### ComfyUI HOI4 portrait workflow

Verify non-sourced fictional/impossible portrait routing uses native ImageGen
and never selects a ComfyUI text-to-image graph. Verify generic Disabled output contains no portrait provider components,
marker sections, Cloud MCP entry, or ComfyUI-specific guidance. Verify Cloud,
Local, and RunPod selections persist through creation, import, settings, Update,
and Repair, while provider credentials remain absent from project state, plans,
locks, logs, and previews. Verify Cloud deferred authorization/subscription,
local bounded discovery/loopback rejection/hardware rejection/current workflow
workflow and adaptive-crop node hashes, RunPod canonical commands and browser guidance, source/prompt basename
pairing, exact prompt prefix and person-only validation, output sizes, DDS
conversion, source-based fallback, final versus pending status, and durable
source archive preservation. Use fakes and fixtures only; do not spend Cloud
credits or start paid RunPod resources.
Also verify that expanding the portrait row shows 16 GB VRAM and 25 GB storage
as recommendations while core readiness remains non-blocking.

### Interrupted install

Terminate during staging and apply. Verify resume and rollback.

### Setup-assistant selection and Chat sources

Run each provider profile through new and existing-project planning with a fake
adapter. Change provider and model after a proposal is returned and verify the
old record cannot enter a plan. Verify that switching setup assistants leaves
development-client components and the flatten checkbox unchanged, generated
canonical instructions, AGENTS/README content contains no setup-provider
attribution, and a DeepSeek setup can still install Codex configuration and
open the selected client. Select the flatten checkbox and
verify skill renames, subagents, adapted AGENTS, README, conflicts,
flat-conflict preservation across a later source conflict, backup, interruption
recovery, and rollback.

The same E2E family must cover the visible experimental account-CLI state
without asking for credentials, native structured-output failure messaging,
Claude's exact model-ID field, automatic image-provider component selection,
and restored HOI4 Live Tools selection plus its live-testing provider state.

## Golden files

Descriptors, provider-adapted CLAUDE/AGENTS and README, flattened Chat sources,
TOML merge, setup scan plus project-profile findings, plan, lock, readiness,
journal, and conflict preview. Golden updates require review.

ChatGPT source-export tests cover eligibility from scan-detected AGENTS, skills,
or subagents without an installation lock, the Downloads default,
required/default selection, optional root Markdown selection, stale selection
rejection, UTF-8 and secret-shaped content rejection, links/traversal, archive
collisions and size limits, no-overwrite, atomic output, and unchanged project
contents.

## UI tests

`pnpm test:e2e` runs Playwright against the development server using disposable
browser contexts and sanitized documentation fixtures. Install its browser
with `pnpm test:e2e:install`; CI performs this before the browser gate. The
suite checks rendered screens, runtime errors, horizontal overflow at desktop
and narrow widths, keyboard coding-environment selection, signed-out project
management, and recovery details. It does not claim native IPC or a complete
installation from a screenshot fixture. Native lifecycle tests remain required.

Keyboard traversal, focus order, screen reader labels, contrast, scaling, long paths, long translations, error states, reduced motion, and visual regression for all 17 screen states. Add density assertions for the seven-phase rail, one primary task per screen, maximum visible content regions, collapsed secondary details, absence of permanent keyboard hints, and no repeated explanatory copy.

## Security tests

Traversal, symlink and junction race, case collision, reserved device name,
command injection, environment injection, secret in stderr, crash redaction,
archive limits, compromised manifest, and rejection of dynamic/non-HTTPS
external browser URLs. Persist prefixed, quoted-field, and unquoted-assignment
credential shapes in a multibyte transaction failure and prove that journal
storage, current and legacy journal reads, Recovery Details, and direct
transaction and rollback errors contain only 2 KiB UTF-8-safe redacted text and
never the raw value. Profile samples must additionally prove that token-shaped
and machine-local values are redacted from documentation overviews and
model-visible excerpts, while prefix hashes remain hashes of only the sampled
bytes.

## Web edition

- Unit tests in `src/web/*.test.ts(x)` drive the page against a fake core
  client: new and existing mods, per-conflict decisions, core errors, the
  assistant's header handling, one corrective turn, and no key in core calls.
- `crates/hoi4-setup-web` tests verify downloads, export from the candidate
  manifest fixture, and assistant replies (retry, truncation, unreadable).
- The CI `Web edition` job builds the page, runs `scripts/check_web_build.mjs`
  (CSP, SRI, connect hosts, size budgets), and runs `pnpm test:web` in
  Chromium, Firefox, and WebKit. GitHub's REST API is answered locally; source
  files come from `raw.githubusercontent.com` at the pinned fixture commit and
  pass the core's hash check. Chromium also downloads the ZIP and scans a
  golden mod folder.

## Performance

`crates/hoi4-setup-core/tests/performance_budgets.rs` enforces time budgets
for profiling 120, 6,000, and 40,000-file synthetic mods and for writing and
reading a 6,000-entry ZIP, so the shared core cannot regress unnoticed on
either host. `scripts/check_web_build.mjs` bounds the WebAssembly core and the
page's scripts.

Test targeted setup inventories containing 500, 20,000, and 150,000 approved
agentic files separately from project-profile fixtures approaching the 500,000
file and 200,000 directory bounds. Include mods with very large gameplay,
localisation, media, wiki, and generated-data trees. Measure first finding,
total scan, profile memory, UI responsiveness, and cancellation latency.
Large-mod evidence must prove that out-of-scope setup entries are filtered
before targeted sort/count/path budgets, profile exclusions are not followed,
profile samples stay within the prefix and aggregate bounds, and detector text
stays within its retained-content bound. An isolated fixture keeps its complete
path/size/modified-time signature unchanged; an approved live fixture also
snapshots app-owned metadata and reports unrelated concurrent changes instead
of attributing them to the scanner. Windows installer lifecycle evidence must
start without current-user or matching legacy machine-wide product
registration, fail closed on registry inspection errors, and end with no
E2E-owned install path in either product registry key.

Exercise retained project and launcher-parent directory enumeration against
symlink/junction replacement and swap-back, aggregate sensitive-path reporting
without filename disclosure, case-colliding setup paths, forged launcher
approvals, and Git dirtiness both inside and outside literal targeted Agentic
pathspecs. Cover modified, staged and unstaged deleted, wildcard-shaped, and
command-line-batched setup paths, path-inventory and process-output truncation,
Unicode case collisions, Unix literal backslashes, CRLF skill frontmatter,
Windows drive/UNC absolute paths, and equality between rendered evidence bytes
and approved evidence hashes. Cancellation tests must interrupt launcher-parent
enumeration and a running reviewed Git/process child within the documented
latency bound. A provider schema rejection must
leave the approved scan reusable; malformed transport JSON must force a fresh
initialized App Server without exposing raw protocol text.

Add a Tauri command responsiveness regression test with blocking fake
filesystem, network, Git, and provider waits. Hold each representative wait
open and assert that a desktop event-loop probe remains schedulable and that
cancellation is observable. Every command must use async/thread-pool dispatch;
an eventual correct result does not excuse event-loop blocking.

## Linux native package acceptance

CI, development preview, and stable release build Linux x64 on Ubuntu 22.04
with `HOI4_MOD_SETUP_BUNDLE=appimage,deb`. Package checks validate native and
packaged ELF64 architecture, the AppImage type-2 marker, a root-contained
AppRun/application payload, and Debian package name, version, and architecture.
Both public Linux packages receive separate updater signatures; installer-specific
metadata prevents a Debian install from selecting AppImage bytes.

`pnpm test:release-assets` exercises both Linux public links, the complete updater
set, dependency-evidence unions, and rejection of missing platforms/packages,
missing Debian signatures, foreign packages, changed signed bytes, and invented
Linux OS-signing evidence. `python -m unittest scripts/test_linux_release_gates.py`
proves that missing native gates and skipped/failed/cancelled or foreign-app CI
checks cannot satisfy protection. `pnpm test:workflows` requires the dedicated
Linux job and final embedded-key verifier in the release pipeline.

The native runner uses Xvfb and D-Bus with a temporary XDG profile, bounded
process-group termination, and no real provider or game call. AppImage extraction,
launch, and removal work without a FUSE mount. Local Debian smokes also extract,
launch, and remove without altering the host package database. Dedicated CI
sets `HOI4_MOD_SETUP_DEDICATED_LINUX=1` to install and purge the verified Debian
package with `dpkg`, refuse pre-existing product registrations, fail closed on
unreadable package state, and assert clean removal. Native package tests do not
replace real game/launcher, Secret Service round-trip, real-account login, or
signed replacement/restart/retry acceptance.

Linux native CI, preview, and stable release also run
`scripts/test_linux_credentials.sh` with the optional source-Python adapter
unset. The runner selects and verifies the exact ignored native Rust test,
creates a temporary HOME/XDG and D-Bus/Secret Service session, exercises
save/read/delete with a synthetic value, and proves the serialized reference
does not contain it. No real desktop vault, account, or external source checkout
is required. Linux core-only fuzz compilation installs `pkg-config` and
`libdbus-1-dev` before its Cargo check; workflow mutation tests reject missing
prerequisites, incorrect ordering, missing vault gates, and external adapter
inheritance in CI.

## Compatibility

Test supported Windows, macOS, and Linux versions, x64 and Apple Silicon where applicable, case-sensitive and case-insensitive volumes, local and cloud-synced folders.

## Release gates

- schema examples validate
- no critical security issue
- transaction fault suite passes
- core new and existing flows pass on every supported platform
- 3D states are honest
- Super Events selection, integrity, maintenance, and non-blocking readiness are
  honest
- portrait provider state, disabled cleanup, local discovery, Cloud MCP
  registration, RunPod guidance, fallback, and no-secret persistence pass
- accessibility audit passes
- artifacts are signed or checksummed

## Open-source repository tests

Validate the public repository layer:

- root README contains user information rather than contributor build commands
- GitHub issue forms and workflows parse as YAML
- subagent TOML files parse and include bounded instructions
- skill files have valid frontmatter and update triggers
- CODEOWNERS covers governance, AGENTS, skills, subagents, schemas, security, and release paths
- Dependabot monitors npm, Cargo, and GitHub Actions
- default GitHub Actions permissions remain read-only outside the release job
- fork pull requests receive no release credentials
- checkout-free publication jobs pass `--repo "$GITHUB_REPOSITORY"` to every
  `gh release` read and write
- release builds use the exact tag commit
- public release is blocked when `LICENSE` is missing
- committed-secret pattern checks pass
- updater metadata contains Windows x64, both macOS architectures,
  `linux-x86_64-appimage`, `linux-x86_64-deb`, and the AppImage
  `linux-x86_64` fallback, exact release URLs, and signatures cryptographically
  verified against the committed key for the final packages
- startup update tests show an available version and begin one automatic signed
  installation, while failure leaves the app usable and exposes retry
- a stage-8 validation interruption shows the exact stage/checkpoint and
  sanitized failure details, offers only Continue and Discard before apply,
  and never renders Undo as an unavailable card; the fixture includes
  prefixed, quoted-field, and unquoted-assignment credential shapes plus
  multibyte overflow and proves the raw values are absent from Details

## Skill drift tests

For broad changes, compare touched product surfaces with the skill ownership table. Pull request review should fail when a workflow changed and the owning skill remains stale without an explicit reason.

## Launcher-ready scaffold tests

Create clean temporary projects on every supported platform and verify the internal descriptor, external launcher descriptor, thumbnail decode, picture reference, folder profile, backup, rollback, repair, and modified-thumbnail preservation. Prove that rendering uses confirmed provider proposals and deterministic validators.
On Windows, the full transaction test must render the launcher path without the
internal `\\?\` verbatim prefix, validate it against the canonical root used for
filesystem operations, still reject a different complete path, and resume a
verified pre-apply validation checkpoint through final readiness and the
success lock.

The browser harness builds development-only synthetic routes into the ignored
`artifacts/browser-fixture` directory, then serves that fixture on loopback
port 1421. It never reuses a running user app or enables fixtures in the normal
production output. Build readiness is awaited before browser tests start, so
Vite cold module compilation is not charged to the first wizard assertion.
