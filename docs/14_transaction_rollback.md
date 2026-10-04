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

## Rooted filesystem operations

`src-tauri/src/safe_fs.rs::RootedDir` provides no-follow primitives for many
transaction reads and mutations. Unix child operations use `openat`; Windows
root acquisition walks lexical components from the volume or share root,
rejects reparse points at each opened component, and uses a 128-bit file ID.
The transaction orchestration still reopens roots through path-based wrappers
between many operations. The source regression catches selected direct
`std::fs` calls, but is not a complete proof of the transaction boundary.

Existing-project scans capture the root identity, and semantic review and plan
construction require the same directory. Plans and journals persist that
identity; a create-leaf plan binds its existing parent and later records the
created directory. Recovery rejects drift and root creation uses the reviewed
parent handle. Schema 1.0 journals without identity evidence remain
inspect-only. A root left before its identity checkpoint is retained with its
observed identity and stays unowned. Inverse rollback stops for inspection if
the recreated directory does not match the journal.

Forward backup, apply, final verification, lock construction, and lock commit
retain the reviewed project capability. Managed rollback file and lock changes
and finalization checks retain it as well. Post-install checks read project files through the same retained capability, and the bytes they validate are the bytes they hash.

An external destination's parent directory, such as the launcher `mod` folder, is bound when the plan is built: `bind_plan_external_parents` opens the parent through one handle, records its identity as the plan operation's `external_parent_identity`, and requires a fresh hash of the destination through that same handle to equal the reviewed `local_sha256`, so the bound folder is the reviewed one.
The journal operation starts from the plan's identity, and `run_transaction` refuses a different folder, or a missing folder for a mutating operation, before it writes any transaction storage; the backup stage verifies the same identity again.
A plan whose parent did not exist at review, and a plan from before plan-time binding, carry no identity; for those the backup stage binds the parent the first time it opens it, as before.
A resumed replay carries the identity forward and refuses an interrupted journal that bound a different folder than the plan, and the pre-replay check refuses a different directory before a new journal is written, so the interrupted journal stays resumable once the bound directory returns.
Apply takes the precondition hash, performs the quarantine and placement, and reads the result back through one retained parent handle whose identity must match; an external operation without a bound identity is refused at apply.
Post-install checks, final verification, finalization resume, rollback, the rollback child backup, quarantine settlement, and the quarantine sweep reopen the parent only when it still has the bound identity, and a bound parent is never recreated.
A link or other non-directory at the path of a bound parent is identity drift.
A missing bound parent is handled by whether the operation may have changed its destination: an operation with a leaf-changing action whose status has moved past `pending` and `staged` (including one only marked `applying` by a batch intent) could have its destination or a quarantine of the user's bytes inside the moved folder, so rollback, finalization, and the quarantine sweep stop with a `PathSecurity` error that names the folder and asks for it to be moved back.
Rollback stops in the child-backup capture before it writes any rollback intent or changes a project file, the parent journal stays `rolling_back` with rollback allowed, and the same rollback finishes once the folder is back.
A skip, an external action, a legacy record without an action, and an operation still `pending` or `staged` changed nothing in that folder, so their rollback genuinely requires nothing and a missing parent still reads as absent; an unbound legacy record keeps the absent reading too.
A bound parent is never recreated, so a deleted launcher folder blocks rollback of a changed launcher descriptor until it is restored or the journal is reviewed manually.
The rollback child backup is copied and hashed in one pass from one opened handle, so its `before_sha256` and `backup_sha256` describe the same bytes.
Journal operations from before the binding keep the earlier path-only behavior in rollback.

Created-root cleanup, application-data, Git, external actions, and readiness are not yet bound to retained capabilities through their complete lifetimes, and the external parent handle is reopened and rechecked at each step rather than retained across stages.
Those cross-stage races remain release blockers. A regular destination that changes
after its precondition hash is no longer replaced or deleted blindly; the
destination quarantine below keeps those bytes and records them in the journal.

On Windows, `RootedDir::sync_directory` flushes the retained directory through a write handle obtained with `ReOpenFile`.
When Windows denies that reopen, the directory is opened by its retained path, whose ancestors cannot be renamed while their delete-denying handles are held, and the flush proceeds only when the opened object has the same 128-bit file identity.
Only `ERROR_INVALID_FUNCTION` and `ERROR_NOT_SUPPORTED` from the flush itself are accepted as a filesystem without directory flush; every other failure is reported.
Directory-entry durability across a sudden power loss is still not proven by a native power-loss test.

## Destination quarantine

Every forward apply, managed rollback step, success-lock commit, and rollback lock restore that changes an existing regular file uses the same sequence in `mutate_live_leaf`.

1. The precondition hash is taken through the retained parent handle.
2. For a journaled operation, a synced operation checkpoint records `quarantine_leaf`, the deterministic same-directory name `.hoi4ms-quarantine-<transaction id>-<operation id>.tmp`, with no `quarantine_sha256`.
3. The destination leaf is moved to that name with an exclusive same-directory rename through the retained parent handle, and the directory is synced. No cross-volume copy happens.
4. The moved bytes are hashed. When they differ from the precondition, the observed hash is journaled and the file is moved back with an exclusive rename. If a new file took the destination name in the meantime, both files are kept, the quarantine stays recorded, and the operation fails for manual review.
5. When the bytes match, the observed hash is journaled as `quarantine_sha256`, and the new bytes are placed with a synced temporary file plus an exclusive rename. A file created at the destination in the window is never replaced; the operation fails and the verified original stays in its quarantine. A managed delete places nothing.
6. Forward apply reads the destination back and requires the staged hash (or, for a delete, an absent destination) before it records the operation as `verified`. Any other bytes were written after placement: the operation keeps its `applying` intent without `after_sha256`, the verified original stays in its quarantine, and apply fails as a changed-local-file conflict. Rollback then refuses with both files kept instead of treating the edit as installed bytes.
7. After the operation result checkpoint is synced, the quarantine is removed only while it still holds the verified bytes. On Windows the hashed handle denies write and delete sharing and performs the delete; on Unix the leaf identity is rechecked before `unlinkat`.

An ordinary error after the rename and before placement completes, such as a hash read failure, a failed journal write, or a failed placement copy, triggers a best-effort exclusive rename of the quarantined bytes back to an absent destination before the error is returned. The journal intent stays recorded, so rollback still applies when that move back is impossible. The same applies when the rename succeeded but its directory sync or binding check failed. Quarantine fault hooks model a process stop and deliberately skip this move back. Errors after placement are left to rollback.

On Windows the delete uses `FileDispositionInfoEx` with POSIX semantics. When the volume rejects that class with `ERROR_INVALID_PARAMETER`, `ERROR_NOT_SUPPORTED`, or `ERROR_INVALID_FUNCTION`, as FAT, exFAT, and many SMB servers do, the classic `FileDispositionInfo` disposition is set on the same hashed handle instead. Only read sharing was granted, so no writer or deleter can open the file before the handle closes and the entry is removed; another reader that already holds the file open can keep the name in a delete-pending state until it closes. There is no preflight capability probe: the fallback was chosen so those volumes keep working. A unit test forces the classic route on NTFS; it has not run on a real FAT, exFAT, or SMB volume.

A destination that was absent at review is created with an exclusive rename, so a file that appears in the window is kept and the operation fails.

The exclusive rename uses `renameat2(RENAME_NOREPLACE)` on Linux, `renameatx_np(RENAME_EXCL)` on macOS, and a `FILE_RENAME_INFO` rename with `ReplaceIfExists = 0` on Windows. A Unix filesystem without an exclusive rename falls back to exclusive `linkat` followed by `unlinkat`.

Recovery handles every crash point:

| Stop point | Live state | Recovery |
| --- | --- | --- |
| Before the rename | Destination unchanged, quarantine absent | Rollback sees the predecessor bytes as already restored. |
| After the rename, before verification | Destination absent, quarantine unverified | Rollback moves the quarantine back without replacing anything. |
| After verification, before placement | Destination absent, quarantine verified | Rollback moves the quarantine back. |
| Changed bytes journaled, before the move back | Destination absent, quarantine holds the changed bytes | Rollback moves the changed bytes back and does not restore the backup over them. |
| After placement, before the result checkpoint | New bytes, verified quarantine | Rollback moves the installed bytes into its own quarantine, moves the original bytes back, and releases its quarantine after both rollback records are synced. |
| After the result checkpoint, before release | New bytes, verified quarantine | Same as the previous row. |
| Edited after placement | Local bytes at the destination, verified quarantine, no `after_sha256` | Apply fails as a conflict and rollback refuses with both files kept. |
| Changed bytes moved back | Local bytes at the destination, quarantine absent, `quarantine_sha256` differs from `backup_sha256` | Rollback treats the local bytes as the restored state and does not replace them. |
| Changed bytes and a new destination file | Both files present | Rollback refuses and keeps both files. |

Once project apply has started, resume remains refused and rollback is the recovery route for file operations.
Rollback restores the quarantined bytes rather than the backup copy because they are the newest bytes the user saw at that path.
It probes the derived quarantine name of each operation even when the journal does not record it, and records the hash of the bytes it moved back in `quarantine_sha256`.
Beside installed bytes it moves a forward quarantine back only when the quarantine holds exactly the journaled `quarantine_sha256`; anything else keeps both files for manual review.
When the destination already holds the restored bytes, a surviving forward quarantine is released only if it holds those same bytes; otherwise rollback stops with both files kept.
It never deletes a quarantine whose hash differs from both the reviewed precondition and the backup.

After the last operation of a forward apply, during finalization resume, and after the last rollback step, a bounded sweep lists the destination directories of the transaction's operations for leftover `.hoi4ms-quarantine-<transaction id>-*` files, including the step quarantines of the rollback transaction.
Each file is matched to its operation by derived name and settled by hash: bytes equal to the destination are released; during rollback, precondition or backup bytes beside an absent destination move back; beside a verified result, precondition bytes are released; planned result bytes are released.
Bytes that match none of these, and names that match no operation, are kept and the step fails for manual review.
Lock quarantines are settled separately, and the sweep is bounded to 4,096 directories and 1,000,000 listed entries.
An unverified forward operation is recognized by its missing result evidence, so rollback refuses a destination whose bytes match neither the predecessor nor the planned result instead of removing or replacing it.

Rollback steps journal their own quarantine on the child rollback operation, named with the child transaction ID.
A retry first settles that quarantine: a verified quarantine beside the completed destination is released, and a quarantine beside an absent destination moves back so the step starts again.
A child operation that already reached its quarantine intent keeps its compacted inverse-backup evidence instead of re-capturing a quarantined destination.

The lock quarantine names are derived from the transaction ID with the `install-lock-commit` and `install-lock-restore` suffixes, and the expected displaced and placed hashes come from `previous_lock_sha256` and `result_lock_sha256`.
The success-lock commit also refuses to write when the live lock no longer matches the journaled predecessor.
Resume of a `finalizing` journal releases a verified commit quarantine beside the exact committed lock; beside an absent lock it moves the predecessor back and leaves rollback as the recovery route.
Rollback settles both lock quarantines before it validates the lock precondition.

A populated operation record looks like this:

```json
{
  "id": "op-001",
  "status": "verified",
  "before_sha256": "4444444444444444444444444444444444444444444444444444444444444444",
  "after_sha256": "6666666666666666666666666666666666666666666666666666666666666666",
  "quarantine_leaf": ".hoi4ms-quarantine-960ccbb7c36a41b09d760105bcd83b05-op-001.tmp",
  "quarantine_sha256": "4444444444444444444444444444444444444444444444444444444444444444"
}
```

Both fields are optional and are omitted, never written as `null`, when absent. Journals written before quarantine support remain readable, and schema version `1.0.0` journals must not contain them.

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

If inverse rollback is interrupted after a new directory is created but before
its identity is journaled, recovery stops for inspection instead of adopting
the directory based only on its empty state.

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

Open each backup source once without following links, hash and copy those same
opened bytes, create the backup through the retained backup directory, and
sync it before checkpointing. Reopening a path after hashing is not sufficient
for source identity.

### 7. Staging

Build the complete target outside live paths. Generate both descriptors, the thumbnail, profile folders, provider-adapted AGENTS/README files, selected optional workflow trees such as `workflow.super_events`, optional flattened Chat sources, and merge results from confirmed values here.

Selected profile folders are staged and validated as directories, not marker
files. The plan carries normalized relative directory paths. Apply journals
which paths were absent before creating them, and rollback removes those paths
in deepest-first order only while they remain empty. `.gitkeep` is never
generated or installed.

Selected Codex subagent TOML remains bound to its verified source bytes, but
staging deterministically adds the required `fork_context=false` spawn rule to
developer instructions when the verified file does not already state it. A
definition that explicitly requires inherited context is rejected. The plan
keeps the verified source hash separate from the adapted installed hash.

### 8. Validation

Run parsers, schemas, containment, wiki coverage and the exact locked wiki
snapshot/media/provenance/license evidence, hashes, provider output, flatten
collision/secret/size checks, and component validators against staging. Invalid
descriptor, TOML, JSON, AGENTS, PNG, flattened source, or wiki output cannot
reach the live project.

Launcher validation compares the descriptor's complete `path=` value with the
same canonical user-facing project path that the renderer writes. On Windows,
the core keeps the `\\?\` verbatim prefix for filesystem operations but removes
that internal prefix before the exact case-insensitive launcher comparison;
UNC identity remains preserved. macOS keeps a literal backslash as part of a
path component rather than converting it into a directory separator, and the
renderer canonicalizes system aliases such as `/var` before writing the path so
validation observes the same `/private/var` root.

### 9. Apply

Apply in deterministic order and checkpoint every operation. An existing destination is replaced or deleted only through the destination quarantine; a new destination is created with an exclusive rename. An operation is recorded as `verified` only when the destination reads back as the staged bytes, or as absent for a delete.

### 10. Post-install checks

Hash live files, parse final configuration, run approved health checks, and
verify Git actions. A selected Windows 3D workflow then runs only its reviewed
manifest action: the core rechecks the installed bootstrap hash/size, fixed
arguments, declared network/writes/privilege/rollback evidence, Python identity,
and OS-vault Meshy reference before spawn. The bounded sanitized outcome updates
an effective plan used for readiness and the final lock. Only `ready`,
`incomplete`, or `unsupported_platform` are accepted; the original reviewed
plan remains authoritative for final managed-file verification.

Persist `post-install-actions-intent` before invoking the action and bounded
redacted evidence after it. A missing optional credential or tool returns an
honest non-blocking `incomplete` state. A changed action/script or internal
runner error fails after apply and therefore requires rollback or manual review;
it is never silently replayed. Pre-apply resume uses the same production runner.

### 11. Readiness report

Generate checks and core gate. A blocking check fails the transaction before stage 12 and before a success lock is written; optional incomplete or unsupported routes remain visible without blocking core setup.

Blocking errors include the check's diagnostic message. For MCP bootstrap
failure, that message comes from the already journaled, bounded and redacted
process result, including exit/timeout information. Recovery therefore retains
the cause without rerunning the external action or bypassing package integrity.

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

Operation checkpoints are appended to `operation-checkpoints.jsonl` beside the journal. Each record carries a monotonic `sequence`, and each journal snapshot records the `checkpoint_sequence` it already includes. Replay applies only records with a higher sequence, so a wall-clock step backwards cannot hide a durable intent. Checkpoint records use format `1.1.0`; replay still reads `1.0.0` records, and a journal and log written entirely before sequences existed fall back to the earlier timestamp comparison.

Managed rollback restores project files and predecessor lock state. It does not
claim to uninstall source-declared external bootstrap state such as user-level
`uv`, downloaded runtime caches, dependencies, or Blender extensions; the dry
run and journal retain that explicit boundary.

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

Journal failure messages are diagnostic evidence, not trusted output. Redact
credential-shaped values, including quoted secret fields and unquoted secret
assignments, and bound the message to 2 KiB on a UTF-8 boundary before every
journal write.
Sanitize again after migration and checkpoint replay so a legacy journal cannot
expose a previously stored value through Recovery; the interface applies the
same bounded redaction to Recovery Details and direct transaction or rollback
command errors as a final display boundary.

## Interrupted states

Before apply, resume, rollback, or discard staging after revalidation. During apply, compare each operation's expected before and after hashes. A prior maintenance lock may remain during resume only when its hash matches the journaled predecessor. Unknown state blocks resume or requires manual review. After apply but before readiness, run post-checks and finish or roll back. The final lock write uses a `finalizing` journal state. The journal records the exact pretty-JSON SHA-256 of the success lock before the rollback record is committed; if the process stops after the lock and rollback record are durable, resume verifies the exact lock bytes, rollback record, live operation results, and stage checkpoint before completing only the journal. It never replays file operations. A crash during file rollback leaves `rolling_back` and a per-operation `rollback_applying` checkpoint for a safe retry; inverse backups are validated independently from already-restored live bytes, while a mismatched live state still requires manual review. Resume requires the predecessor lock to exist with the recorded hash (or to remain absent when no predecessor was recorded), and rollback refuses later lock edits before touching project files.

## Idempotency

Each operation has a stable ID and expected states. Repeating a verified operation is a no-op.

## Retention

Keep at least three rollback points by default. Never delete the only backup for an incomplete or unknown transaction.

## Cloud-synced folders

Detect common OneDrive and iCloud paths. Warn about synchronization ordering. Perform local atomic operations and recheck hashes after a stabilization interval.

## Fault tests

Crash and fail at every stage and operation, including disk full, permission loss, antivirus lock, network loss, checksum mismatch, user edits during dry run or staging, external health failure, and rollback after Git initialization. The checked-in fault suite covers stage and apply-operation injection, subprocess termination during finalization and rollback backup creation, inverse rollback refusal after a user file or lock edit, and targeted skipped-file, ownership, remote-approval, reanalysis binding, exact success-lock, predecessor-lock, and separate rollback-transaction backup regressions.

Destination-quarantine coverage, all modelled as returned errors rather than process aborts:

- Forward replace: before the rename, after the rename, after a changed-bytes record before the move back, after verification, after placement before the result checkpoint, and after the result checkpoint before release; deterministic barriers for an edit after the precondition, a file created after verification, and an edit after placement; injected hash-read and journal-write errors after the rename.
- Forward create: a stop after placement and a file created in the apply window.
- Managed delete: a concurrent edit after the precondition and a stop after verification only.
- Rollback backup restore, rollback restore of a forward quarantine, and rollback removal of a created file: before the rename, after the rename, after verification, after placement, and before release, each followed by a retry.
- Success-lock commit: before the rename, after the rename, after verification, and before release.
- Rollback lock restore with a predecessor and lock removal after a first install: before the rename, after the rename, after verification, and before release.
- Recovery: a quarantine the journal does not name, an operation whose intents were never replayed, unknown bytes under an owned or unowned quarantine name, a forward quarantine beside already-restored bytes, checkpoint replay ordered by sequence, and the classic delete disposition forced on NTFS.

Not covered: process-abort variants of the quarantine boundaries, external-destination quarantines, the remaining managed-delete boundaries, a concurrent edit during a rollback step, real FAT, exFAT, or SMB volumes, and every Unix route. Native disk-full, antivirus, network-loss, timeout, cancellation, and journal-write-failure adapters remain release-gate work and must not be represented as passing until exercised on Windows and macOS.
