# Testing strategy

## AI provider integration matrix

Use a protocol fixture that emulates App Server JSONL without real account credentials in ordinary CI. Cover process absence, incompatible versions, initialize ordering, an existing ChatGPT session, browser login, cancellation, device-code fallback, logout, account updates, usage limits, App Server crash, turn cancellation, output schema acceptance and rejection, unexpected fields, deterministic rejection of bad identifiers, token and account-data absence, and recovery while signed out. Add fake Anthropic, OpenAI-compatible, and loopback adapters for configured, missing-key, malformed-response, endpoint, redirect, bounded-response, provider-switch, and no-secret-persistence cases.

Run a controlled manual release test against a real ChatGPT account. Never place real credentials in CI.

## Unit tests

- path normalization and containment
- project ID validation
- descriptors
- manifest schema
- dependency graph and cycles
- platform resolution
- hashing
- file classification
- three-way and TOML merge
- secret redaction
- readiness aggregation
- journal transitions
- provider profile and model binding
- flattened Chat-source mapping and size/collision policy

## Property tests

- normalized destinations never escape root
- apply then rollback restores hashes
- verified operations are idempotent
- removal never deletes unowned files
- secret-like values never survive serialization
- provider credential references remain keyed and never cross provider switches
- flattening is deterministic and rejects traversal, links, collisions, secret-shaped content, and oversized files

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
- Git fixtures
- MCP test server
- launcher descriptor outside project
- provider API envelopes and bounded response bodies
- flattened source file names and user-selected extra paths

## Transaction fault injection

Fail at every stage and operation boundary through process kill, disk full, permission denied, file lock, network loss, checksum mismatch, command timeout, health failure, and cancellation. Verify recovery and final hashes. Rollback tests must also assert a separate child journal, parent linkage, inverse backup of live post-transaction bytes, durable parent/child rollback records, safe reuse of the child identity on retry, and inverse rollback refusal after a later user file or lock edit.

## End-to-end cases

### New Windows project

Create, generate descriptors, install core, initialize Git, run readiness, and open project.

### Existing Windows project

Scan local AGENTS and Codex files, review, merge, install, update, repair, and roll back.

### New macOS project

Install platform-neutral core, report unsupported current Windows-only MCP and 3D routes, and keep core usable.

### Missing Meshy key

Select 3D, omit key, complete core, verify incomplete optional status, configure later, and rerun health.

### LoRA placeholder

Record interest and assert zero forbidden operations.

### Interrupted install

Terminate during staging and apply. Verify resume and rollback.

### Provider selection and Chat sources

Run each provider profile through new and existing-project planning with a fake
adapter. Change provider and model after a proposal is returned and verify the
old record cannot enter a plan. For Codex, select the flatten checkbox and
verify skill renames, subagents, adapted AGENTS, README, extras, conflicts,
flat-conflict preservation across a later source conflict, backup, interruption
recovery, and rollback. For non-Codex profiles, verify
the checkbox and Open in Codex action are absent.

## Golden files

Descriptors, provider-adapted AGENTS and README, flattened Chat sources, TOML merge, scan, plan, lock, readiness, journal, and conflict preview. Golden updates require review.

## UI tests

Keyboard traversal, focus order, screen reader labels, contrast, scaling, long paths, long translations, error states, reduced motion, and visual regression for all 17 screens. Add density assertions for the seven-phase rail, one primary task per screen, maximum visible content regions, collapsed secondary details, absence of permanent keyboard hints, and no repeated explanatory copy.

## Security tests

Traversal, symlink and junction race, case collision, reserved device name, command injection, environment injection, secret in stderr, crash redaction, archive limits, and compromised manifest.

## Performance

Test 500, 20,000, and 150,000 file projects plus a large wiki media set. Measure first finding, total scan, memory, UI responsiveness, hash throughput, and cancellation latency.

## Compatibility

Test supported Windows and macOS versions, x64 and Apple Silicon where applicable, case-sensitive and case-insensitive volumes, local and cloud-synced folders.

## Release gates

- schema examples validate
- no critical security issue
- transaction fault suite passes
- core new and existing flows pass on both platforms
- 3D states are honest
- LoRA placeholder creates zero forbidden actions
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
- release builds use the exact tag commit
- public release is blocked when `LICENSE` is missing
- committed-secret pattern checks pass

## Skill drift tests

For broad changes, compare touched product surfaces with the skill ownership table. Pull request review should fail when a workflow changed and the owning skill remains stale without an explicit reason.

## Launcher-ready scaffold tests

Create clean temporary projects on both platforms and verify the internal descriptor, external launcher descriptor, thumbnail decode, picture reference, folder profile, backup, rollback, repair, and modified-thumbnail preservation. Prove that rendering uses confirmed provider proposals and deterministic validators.
