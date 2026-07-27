# Testing strategy

## Codex integration test matrix

Use a protocol fixture that emulates App Server JSONL without real account credentials in ordinary CI. Cover process absence, incompatible versions, initialize ordering, an existing ChatGPT session, browser login, cancellation, device-code fallback, logout, account updates, usage limits, App Server crash, turn cancellation, output schema acceptance and rejection, unexpected fields, deterministic rejection of bad identifiers, token and account-data absence, and recovery while signed out.

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

## Property tests

- normalized destinations never escape root
- apply then rollback restores hashes
- verified operations are idempotent
- removal never deletes unowned files
- secret-like values never survive serialization

## Fuzzing

Descriptor syntax, thumbnail PNG decoding, TOML parsing and structured merge,
manifest JSON, Codex analysis JSON, relative paths, Markdown link extraction,
localisation keys, encodings, huge files, and deep trees. The checked-in
`fuzz/` package provides bounded `descriptor` and `toml_merge` targets in
addition to the manifest, path, and Codex targets.

## Integration tests

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

## Transaction fault injection

Fail at every stage and operation boundary through process kill, disk full, permission denied, file lock, network loss, checksum mismatch, command timeout, health failure, and cancellation. Verify recovery and final hashes.

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

## Golden files

Descriptors, adapted AGENTS, TOML merge, scan, plan, lock, readiness, journal, and conflict preview. Golden updates require review.

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

Create clean temporary projects on both platforms and verify the internal descriptor, external launcher descriptor, thumbnail decode, picture reference, folder profile, backup, rollback, repair, and modified-thumbnail preservation. Prove that rendering uses confirmed Codex proposals and deterministic validators.
