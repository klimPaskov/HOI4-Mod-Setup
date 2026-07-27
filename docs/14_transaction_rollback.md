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

## Twelve stages

### 1. Preflight

Validate root, disk, permissions, process locks, incomplete journal, platform, confirmed Codex analysis metadata, selections, and connectivity. Bind the canonical project root and transaction UUID before any mutation.

### 2. Source resolution

Resolve exact commit and validate manifest compatibility.

### 3. Selective download

Fetch only selected files and metadata into immutable cache.

### 4. Checksum verification

Verify every file before staging.

### 5. Dry-run review

Show exact file, external, Git, conflict, and rollback actions. Require approval.

### 6. Backup

Copy every path that may be replaced, merged, removed, or have metadata changed. Record hash and metadata. If a prior installation lock exists, copy and hash it outside the project before apply.

### 7. Staging

Build the complete target outside live paths. Generate both descriptors, the thumbnail, profile folders, and merge results from confirmed values here.

### 8. Validation

Run parsers, schemas, containment, wiki coverage, hashes, and component validators against staging. Invalid descriptor, TOML, JSON, AGENTS, PNG, or wiki output cannot reach the live project.

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
supported. The journal includes action, source/result/backup hashes,
ownership/rollback metadata, and the project-root binding. Source resolution,
selective download, and checksum verification are completed by the core plan
builder before the reviewed transaction is accepted; their exact revision and
hash evidence is carried into the journal plan. Never mark complete before
destination, readiness, and hash evidence are durable.

## Apply order

1. directories
2. new independent files
3. managed leaves
4. structured merged files
5. descriptors
6. project state and lock last
7. Git after file validation

The lock is a completion artifact. A partial apply cannot look successful because the lock is written only after final verification.

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

## Interrupted states

Before apply, resume, rollback, or discard staging after revalidation. During apply, compare each operation's expected before and after hashes. A prior maintenance lock may remain during resume only when its hash matches the journaled predecessor. Unknown state blocks resume or requires manual review. After apply but before readiness, run post-checks and finish or roll back. The final lock write uses a `finalizing` journal state: if the process stops after the lock and rollback record are durable, resume verifies those artifacts and completes only the journal. It never replays file operations. A crash during file rollback leaves `rolling_back` and a per-operation `rollback_applying` checkpoint for a safe retry; a mismatched live state still requires manual review.

## Idempotency

Each operation has a stable ID and expected states. Repeating a verified operation is a no-op.

## Retention

Keep at least three rollback points by default. Never delete the only backup for an incomplete or unknown transaction.

## Cloud-synced folders

Detect common OneDrive and iCloud paths. Warn about synchronization ordering. Perform local atomic operations and recheck hashes after a stabilization interval.

## Fault tests

Crash and fail at every stage and operation, including disk full, permission loss, antivirus lock, network loss, checksum mismatch, user edits during dry run or staging, external health failure, and rollback after Git initialization. The checked-in fault suite covers stage and apply-operation injection, subprocess termination during finalization and rollback backup creation, plus targeted skipped-file, ownership, remote-approval, reanalysis binding, and separate rollback-transaction backup regressions. Per-operation rollback resume and power-loss behavior outside the tested finalization windows remain release work.
