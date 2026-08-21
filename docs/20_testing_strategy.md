# Testing strategy

## AI provider integration matrix

Use a protocol fixture that emulates App Server JSONL without real account credentials in ordinary CI. Cover process absence, incompatible versions, initialize ordering, an existing ChatGPT session, browser login, exact per-`loginId` `account/login/cancel` behavior, isolated concurrent cancellation, device-code fallback, logout, account updates, usage limits, App Server crash, turn cancellation, output schema acceptance and rejection, unexpected fields, deterministic rejection of bad identifiers, token and account-data absence, and recovery while signed out. Add fake Anthropic, OpenAI-compatible, and loopback adapters for configured, missing-key, malformed-response, endpoint, redirect, bounded-response, provider-switch, and no-secret-persistence cases.

Run a controlled manual release test against a real ChatGPT account. Never place real credentials in CI.

## Unit tests

- path normalization and containment
- Windows and macOS native Documents resolution, including redirected
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
- flattened Chat-source mapping and size/collision policy
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
- manifest and tree mismatch
- changed default branch
- partial download and resume
- checksum failure
- cache corruption
- Windows Credential Manager mock
- macOS Keychain mock
- Git fixtures with ambient configuration isolation, hostile local config
  rejection before spawn, and direct bounded submodule discovery
- MCP test server
- launcher descriptor outside project
- bounded parent-level launcher descriptor discovery, explicit pre-scan
  confirmation/decline/cancel, candidate mismatch, and no read of an
  unconfirmed candidate
- provider API envelopes and bounded response bodies
- flattened skill names, required project documents, and subagent names
- Super Events manifest selection, exact `.agents/skills/hoi4-super-events/`
  destination, `core.skills` dependency, no-credential plan, and deselected
  exclusion from the core skills tree

## Transaction fault injection

Fail at every stage and operation boundary through process kill, disk full, permission denied, file lock, network loss, checksum mismatch, command timeout, health failure, and cancellation. Verify recovery and final hashes. Rollback tests must also assert a separate child journal, parent linkage, inverse backup of live post-transaction bytes, durable parent/child rollback records, safe reuse of the child identity on retry, and inverse rollback refusal after a later user file or lock edit.

For an absent new-project root, inject failure before apply, at leaf creation,
after managed files are applied, and during rollback. Assert that exactly one
reviewed leaf is created only at apply, that a pre-apply race stops safely, and
that rollback removes the leaf only when empty while preserving unknown content
and its parent.

## End-to-end cases

### New Windows project

Resolve redirected Documents, auto-fill the project root and launcher descriptor
from the project ID, preserve an explicit override, create the reviewed leaf at
apply, generate descriptors, install core, initialize Git, run readiness, and
open the project.

### Existing Windows project

Scan local AGENTS and Codex files, review, merge, install, update, repair, and roll back.

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
the skill tree and Super Events-specific `AGENTS.md` guidance. Test Update adding
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

### Provider selection and Chat sources

Run each provider profile through new and existing-project planning with a fake
adapter. Change provider and model after a proposal is returned and verify the
old record cannot enter a plan. For Codex, select the flatten checkbox and
verify skill renames, subagents, adapted AGENTS, README, conflicts,
flat-conflict preservation across a later source conflict, backup, interruption
recovery, and rollback. For non-Codex profiles, verify
the checkbox and Open in Codex action are absent.

## Golden files

Descriptors, provider-adapted AGENTS and README, flattened Chat sources, TOML merge, scan, plan, lock, readiness, journal, and conflict preview. Golden updates require review.

ChatGPT source-export tests cover eligibility from scan-detected AGENTS, skills,
or subagents without an installation lock, the Downloads default,
required/default selection, optional root Markdown selection, stale selection
rejection, UTF-8 and secret-shaped content rejection, links/traversal, archive
collisions and size limits, no-overwrite, atomic output, and unchanged project
contents.

## UI tests

Keyboard traversal, focus order, screen reader labels, contrast, scaling, long paths, long translations, error states, reduced motion, and visual regression for all 17 screen states. Add density assertions for the seven-phase rail, one primary task per screen, maximum visible content regions, collapsed secondary details, absence of permanent keyboard hints, and no repeated explanatory copy.

## Security tests

Traversal, symlink and junction race, case collision, reserved device name,
command injection, environment injection, secret in stderr, crash redaction,
archive limits, compromised manifest, and rejection of dynamic/non-HTTPS
external browser URLs. Persist prefixed, quoted-field, and unquoted-assignment
credential shapes in a multibyte transaction failure and prove that journal
storage, current and legacy journal reads, Recovery Details, and direct
transaction and rollback errors contain only 2 KiB UTF-8-safe redacted text and
never the raw value.

## Performance

Test targeted setup inventories containing 500, 20,000, and 150,000 approved
agentic files, plus mods with very large gameplay, media, wiki, and generated
data trees that must be pruned. Measure first finding, total scan, memory, UI
responsiveness, and cancellation latency. Large-mod evidence must prove that
out-of-scope content is neither opened nor counted and that detector text stays
within its retained-content bound. An isolated fixture keeps its complete
path/size/modified-time signature unchanged; an approved live fixture also
snapshots app-owned metadata and reports unrelated concurrent changes instead
of attributing them to the scanner. Windows installer lifecycle evidence must
start without current-user or matching legacy machine-wide product
registration, fail closed on registry inspection errors, and end with no
E2E-owned install path in either product registry key.

Add a Tauri command responsiveness regression test with blocking fake
filesystem, network, Git, and provider waits. Hold each representative wait
open and assert that a desktop event-loop probe remains schedulable and that
cancellation is observable. Every command must use async/thread-pool dispatch;
an eventual correct result does not excuse event-loop blocking.

## Compatibility

Test supported Windows and macOS versions, x64 and Apple Silicon where applicable, case-sensitive and case-insensitive volumes, local and cloud-synced folders.

## Release gates

- schema examples validate
- no critical security issue
- transaction fault suite passes
- core new and existing flows pass on both platforms
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
- updater metadata contains all three supported targets, exact release URLs,
  and non-empty signatures for the final platform packages
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

Create clean temporary projects on both platforms and verify the internal descriptor, external launcher descriptor, thumbnail decode, picture reference, folder profile, backup, rollback, repair, and modified-thumbnail preservation. Prove that rendering uses confirmed provider proposals and deterministic validators.
On Windows, the full transaction test must render the launcher path without the
internal `\\?\` verbatim prefix, validate it against the canonical root used for
filesystem operations, still reject a different complete path, and resume a
verified pre-apply validation checkpoint through final readiness and the
success lock.
