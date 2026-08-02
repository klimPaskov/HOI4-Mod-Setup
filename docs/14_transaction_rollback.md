# Transaction and rollback design

## Metadata layout

```text
<project>/.hoi4-mod-setup/
  install.lock.json
  state.json
  transactions/<id>/
    journal.json
    plan.json
    readiness-report.json
    rollback-record.json
  backups/<id>/
    install.lock.json.bak
```

Large caches and backups live outside the project:

```text
Windows: %LOCALAPPDATA%/HOI4 Mod Setup/
macOS:   ~/Library/Application Support/HOI4 Mod Setup/
```

Large transactions append bounded per-operation records to one checkpoint log during backup, staging, apply, and rollback instead of rewriting the complete operation array for every file. Before apply or rollback changes any live file, durable intent records are written in groups of at most 64 operations. Completed-file records then provide per-file recovery and progress evidence. The log is compacted into an atomic full-journal snapshot every 1,024 completed operations and at stage boundaries; journal reads replay only newer records and reject links, wrong bindings, oversized records, and oversized logs. A crash inside a group leaves durable intents that recovery resolves against verified backups and observed live hashes.

## Twelve stages

### 1. Preflight

Validate root, permissions, process locks, incomplete journals, platform, confirmed selected-provider analysis metadata, selections, provider connectivity/configuration, and flatten preferences. Bind the canonical project root and transaction UUID before any mutation. The setup does not block on a disk-space estimate; filesystem errors remain explicit transaction failures with recovery evidence.

### 2. Source resolution

Resolve exact commit and validate manifest compatibility.

### 3. Selective download

Fetch only selected files and metadata into immutable cache. Record one
operation-bound ledger entry for each remote file with its component, source
path, destination, exact revision, manifest hash, file hash, size, ownership,
and platform.

### 4. Checksum verification

Verify every file before staging.

The backup and staging directories do not exist before their corresponding
stages. Source or checksum failure therefore occurs before any predecessor-lock
or project-file backup is written.

## New-project root lifecycle

When a new project root is absent, the plan records
`project_root_mode: create_leaf`, the canonical existing parent, and exactly
one reviewed leaf name. Preflight verifies that the parent is contained and
stable and that the leaf is absent. Planning, download, backup, and staging
create no project-root directory. Apply creates that one leaf exactly once,
after dry-run approval and staging validation; it does not recursively create
unreviewed ancestors.

If the reviewed parent or leaf changes before apply, or the leaf already
exists, the transaction stops for revalidation rather than adopting or
overwriting it. The journal records whether the leaf was created by this
transaction and checkpoints its create/cleanup state.

Rollback removes the created leaf only when it is still the transaction's
reviewed leaf, all removable managed content has been verified, and the leaf is
empty. Unknown, newly added, modified, or otherwise unmanaged content keeps
the leaf and its content in place; rollback records `retained_user_content`
instead of recursively deleting it or its parent. The external launcher
descriptor remains an independently reviewed, backed-up operation.

### 5. Dry-run review

Show exact file, external, Git, conflict, and rollback actions. Require approval.

### 6. Backup

Copy every path that may be replaced, merged, removed, or have metadata changed. Record hash and metadata. If a prior installation lock exists, copy and hash it outside the project before apply.

### 7. Staging

Build the complete target outside live paths. Generate both descriptors, the thumbnail, profile folders, provider-adapted AGENTS/README files, selected optional workflow trees such as `workflow.super_events`, optional flattened Chat sources, and merge results from confirmed values here.

### 8. Validation

Run parsers, schemas, containment, wiki coverage and the exact locked wiki
snapshot/media/provenance/license evidence, hashes, provider output, flatten
collision/secret/size checks, and component validators against staging. Invalid
descriptor, TOML, JSON, AGENTS, PNG, flattened source, or wiki output cannot
reach the live project.

### 9. Apply

Use same-volume atomic replacement where possible. Apply in deterministic order and checkpoint every operation.

### 10. Post-install checks

Hash live files, parse final configuration, run approved health checks, and verify Git actions.

### 11. Readiness report

Generate checks and core gate. A blocking check fails the transaction before stage 12 and before a success lock is written; optional incomplete or unsupported routes remain visible without blocking core setup.

### 12. Rollback record

Record restoration steps and retention. The current runner writes this record
from the completed journal after final readiness. If a failure occurs after
apply, rollback uses the journal's verified backups and predecessor-lock copy;
it does not claim success until the reversal is persisted.

## Journal durability

Before and after each stage and operation boundary, write a new journal, fsync
the file, atomically replace the old journal, and fsync its directory where
supported. The journal includes action, source path/size, reviewed resolution,
source/result/backup hashes, separate staged/live hashes,
ownership/rollback metadata, and the project-root binding. Source resolution,
selective download, and checksum verification are completed by the core plan
builder before the reviewed transaction is accepted; their exact revision and
hash evidence is carried into the journal plan. Never mark complete before
destination, readiness, and hash evidence are durable. For any reviewed
external wrapper action, persist the manifest-declared executable, interpreter,
and runtime identity evidence in the plan and journal; missing identity keeps
the action `planned_unavailable` and is never permission to run a same-named
PATH command.

## Apply order

1. directories
2. new independent files
3. managed leaves
4. structured merged files
5. descriptors
6. project state and lock last
7. Git after file validation

The lock is a completion artifact. A partial apply cannot look successful because the lock is written only after final verification.

The Rust core retains the reviewed plan and prepared bytes in its bounded session. Starting installation sends only that plan's ID and the reviewed project root; it does not send the full plan or confirmed AI-analysis record back from the renderer. This prevents a second serialization boundary from invalidating or altering an already approved plan.

## Rollback

Reverse operations: remove created files, restore backups, restore metadata,
reverse structured contributions, restore the external descriptor, restore the
verified predecessor lock (or remove only this transaction's lock), and
reverse new Git initialization when safe. Explicit skip, external, and
rollback-none operations are durable no-ops; legacy operations without this
metadata are also treated conservatively and are never deleted automatically.
Preserve-mode remote additions are removed only when their recorded URL is
unchanged; an already-matching preserve-mode remote is left in place and a
different URL is rejected. Rollback and staging discard recheck the
journal-root binding internally. Staging and backup roots are reparse-point
checked before and during file operations. A first-install skip of a modified
managed file records the incoming hash as its managed baseline and preserves
the local content during later removal. The implementation persists the Git
cleanup boundary and uses `rolling_back` plus `rollback_applying` checkpoints
so a retry verifies an already-restored operation before continuing; a
mismatched live state still requires manual review.

A completed rollback child journal is also a guarded inverse point for the
managed file and lock state that existed before rollback. The Ready screen
offers this action only for that completed child; the same root and recorded
live-state hashes are checked before any file apply, and later edits fail
closed. Git initialization, remote changes, and other external side effects
are not recreated by this inverse action.

## Interrupted states

Before apply, resume, rollback, or discard staging after revalidation. During apply, compare each operation's expected before and after hashes. A prior maintenance lock may remain during resume only when its hash matches the journaled predecessor. Unknown state blocks resume or requires manual review. After apply but before readiness, run post-checks and finish or roll back. The final lock write uses a `finalizing` journal state. The journal records the exact pretty-JSON SHA-256 of the success lock before the rollback record is committed; if the process stops after the lock and rollback record are durable, resume verifies the exact lock bytes, rollback record, live operation results, and stage checkpoint before completing only the journal. It never replays file operations. A crash during file rollback leaves `rolling_back` and a per-operation `rollback_applying` checkpoint for a safe retry; inverse backups are validated independently from already-restored live bytes, while a mismatched live state still requires manual review. Resume requires the predecessor lock to exist with the recorded hash (or to remain absent when no predecessor was recorded), and rollback refuses later lock edits before touching project files.

## Idempotency

Each operation has a stable ID and expected states. Repeating a verified operation is a no-op.

## Retention

Keep at least three rollback points by default. Never delete the only backup for an incomplete or unknown transaction.

## Cloud-synced folders

Detect common OneDrive and iCloud paths. Warn about synchronization ordering. Perform local atomic operations and recheck hashes after a stabilization interval.

## Fault tests

Crash and fail at every stage and operation, including disk full, permission loss, antivirus lock, network loss, checksum mismatch, user edits during dry run or staging, external health failure, and rollback after Git initialization. The checked-in fault suite covers stage and apply-operation injection, subprocess termination during finalization and rollback backup creation, inverse rollback refusal after a user file or lock edit, plus targeted skipped-file, ownership, remote-approval, reanalysis binding, exact success-lock, predecessor-lock, and separate rollback-transaction backup regressions. Native disk-full, antivirus, network-loss, timeout, cancellation, and journal-write-failure adapters remain release-gate work and must not be represented as passing until exercised on Windows and macOS.
