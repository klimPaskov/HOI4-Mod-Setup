# Audit: destination quarantine transaction recovery (2026-10-03)

Auditor role: `hoi4setup_transaction_recovery_auditor`, read-only.
Scope: `src-tauri/src/transaction.rs`, `src-tauri/src/safe_fs.rs`, `src-tauri/src/models.rs` (`JournalOperation`), `docs/schemas/transaction-journal.schema.json`, `docs/examples/transaction-journal.example.json`.
Source state: commit `2543803` on the working tree of 2026-10-03; the quarantine work is committed there and the in-scope files have no uncommitted edits.
Inputs read: AGENTS.md section 7, the transactions skill, `docs/14_transaction_rollback.md`, the journal schema and example, `public-readiness-transaction-handles.md`, and `public-readiness-transaction-quarantine.md`.

## Verdict

No P0 or P1 defect was found in the new quarantine engine on Windows.
Every crash point I traced for forward apply, managed delete, rollback restore and removal, the rollback restore of a forward quarantine, the success-lock commit, and the rollback lock restore either preserves the user's bytes at the destination, keeps them in a derived quarantine that recovery finds, or refuses with both files kept.
The remaining risks are P2 and P3: recovery relies only on the journal to discover a quarantine, the forward apply does not compare the placed bytes with the staged hash before it releases the quarantine, the POSIX delete has no fallback for non-NTFS volumes, the Unix routes have never run, and the fault suite and documentation claim more boundary coverage than the tests exercise.
P1 items 1, 3, and 4 of the handles handoff remain open and still block a public release.

## Commands run

- `cargo test -p hoi4-mod-setup --all-features --lib quarantine` with `CARGO_TARGET_DIR=target/auditor-tests`: 5 passed (`journaled_quarantine_names_are_bound_to_their_operation`, `file_created_after_quarantine_keeps_both_user_files`, `crash_before_lock_quarantine_release_is_finished_by_resume`, `crash_at_each_quarantine_boundary_rolls_back_to_the_original_bytes`, `rollback_quarantine_interruptions_are_settled_on_retry`).
- `cargo test ... --lib -- concurrent_edit_after_precondition file_created_in_the_apply_window file_created_after_quarantine managed_delete_preserves crash_inside_lock_commit rollback_lock_restore_interruptions live_mutation safe_fs`: 14 passed, including all 8 `safe_fs` tests.
- No flaky timing was observed; the full suite was not rerun.

## Stage coverage

The table traces each interruption point against the code path that recovers it.
"Tested" means a test injects a fault at that point and then asserts recovery.

| Flow | Interruption point | Disk and journal state | Recovery path | Tested |
| --- | --- | --- | --- | --- |
| Forward replace or delete | Before the intent checkpoint | Destination unchanged; batch intent `applying` | `rollback_destination_is_restored` sees the backup bytes | Yes (stage and operation matrix) |
| Forward replace or delete | After the intent, before rename | Destination unchanged; `quarantine_leaf` set, no hash | Same as above; a user edit is refused as "uncertain live state" | Yes (`BeforeRename`) |
| Forward replace or delete | After rename, before hash | Destination absent; unverified quarantine | Rollback forward-quarantine branch moves it back with no replacement (`transaction.rs:6034-6109`) | Yes (`AfterRename`) |
| Forward replace or delete | Hash mismatch recorded, before move-back | Destination absent; quarantine holds the changed bytes | The same forward-quarantine branch moves the changed bytes back | No |
| Forward replace or delete | After verification, before placement | Destination absent; verified quarantine | Forward-quarantine branch | Yes (`AfterVerification`, and a delete in `managed_delete_preserves_a_concurrent_edit`) |
| Forward replace | After placement, before the result checkpoint | New bytes; verified quarantine; `after_sha256` absent | Forward-quarantine branch, with `installed` matched through `expected_sha256` (`transaction.rs:6055`) | No: `fail_after_live_mutation` is tested only for a create (`transaction.rs:10964`) |
| Forward replace or delete | After the result checkpoint, before release | New bytes or absent; verified quarantine; `verified` | Forward-quarantine branch, with `installed` matched through `after_sha256` | Yes (`BeforeRelease`) |
| Forward create | After placement, before the checkpoint | New bytes; `applying` | The generic path accepts `expected_sha256` and removes the file through a child quarantine | Yes (`fail_after_live_mutation`) |
| Forward create | A user file appears in the window | User file; `applying` | Refused as "uncertain live state" | Yes |
| Rollback backup restore | Child intent / after rename / after verification / after placement / before release | See `settle_rollback_step_quarantine` | Settle, then retry | Yes for `RestoreBackup` only (`rollback_quarantine_interruptions_are_settled_on_retry`) |
| Rollback created-file removal (`LiveChange::Delete`) | Each child boundary | Child quarantine beside an absent destination | Settle releases a verified quarantine when the completed state is `None` | No |
| Rollback restore of a forward quarantine (`LiveChange::MoveFrom`) | Child boundaries, after placement | Child quarantine holds the installed bytes; forward quarantine moved or present | Settle includes the parent `quarantine_sha256` in the completed states | No: this branch runs in tests only without a fault |
| Lock commit | Before rename | Lock equals the predecessor | Resume refuses on the checksum, then rollback | No |
| Lock commit | After rename / after verification | Lock absent; commit quarantine | `settle_journal_lock_quarantines` moves it back | Yes |
| Lock commit | After placement, before release | New lock; commit quarantine | Settle releases it beside the exact result lock | Yes |
| Rollback lock restore (predecessor exists) | After rename, before release | | Settle moves back or releases | Yes (`AfterRename`, `BeforeRelease`) |
| Rollback lock restore | Before rename, after verification | | Settle | No |
| Rollback lock removal (no predecessor) | Every boundary | Restore quarantine beside an absent lock | Settle releases, because the completed state is `None` | No |
| Finalize (resume of `finalizing`) | Commit quarantine present | | `finish_finalization` settles before it reads the lock | Yes |
| Any forward point | A real process abort rather than a returned error | | Same paths | No: quarantine boundaries are modelled only as returned errors, and the outer error handler then persists the in-memory journal (`transaction.rs:1722-1768`) |

## Journal state findings

- The quarantine intent, the changed-bytes record, and the verified record are each written through `persist_operation_checkpoint_batch` (`sync_data`) before the next namespace change; every namespace change is followed by a directory sync, so the forward ordering matches the design.
- The forward result checkpoint is synced before `release_quarantine` (`transaction.rs:3426-3434`), and the rollback parent `rolled_back` record is synced before a child quarantine is released when one is held (`transaction.rs:6312-6316`).
- The child rollback `rolled_back` record is written through `persist_rollback_checkpoint`, which uses the unsynced `append_operation_checkpoint` (`transaction.rs:5581-5605`), yet the comment at `transaction.rs:6325` says both records are durable. Recovery stays correct because the synced parent record alone lets the retry treat the destination as restored, but the comment overstates durability (P3).
- Checkpoint replay keeps or skips entries by comparing wall-clock RFC 3339 strings (`transaction.rs:2062`, `if checkpoint.updated_at <= snapshot_updated_at`); quarantine discovery now depends on that comparison (P2-1).
- `OperationCheckpoint` records keep `schema_version: "1.0.0"` (`transaction.rs:1941`) while carrying the new operation fields, and no checkpoint schema documents that.
- Schema: `quarantine_leaf` and `quarantine_sha256` are optional, the `1.0.0` branch forbids both (`schema.json:534-547`), and `quarantine_sha256` depends on `quarantine_leaf` through `dependentRequired`. Serialization omits both when they are `None` (`models.rs:1062-1069`). Journal schema `1.1.0` has not shipped (tag `v0.3.5` still writes `1.0.0`), so no downgrade compatibility is at stake.
- Recovery accepts only the derived name: `journaled_quarantine_leaf` (`transaction.rs:3511`) compares the journaled leaf with `quarantine_leaf_name(transaction_id, operation.id)`, hashes unsafe operation IDs, and checks the hash format.

## Apply findings

- `mutate_live_leaf` (`transaction.rs:3702-3786`) places new leaves only with `rename_file_noreplace`, `write_atomic_noreplace`, or `copy_file_atomic_noreplace_to`; no project destination still goes through the replacing `write_atomic` or `ReplaceFileW`, which remains only in application-data writers (`safe_fs.rs:1601-1648`).
- The Windows no-replace rename sets `FILE_RENAME_INFO.ReplaceIfExists = 0` on a handle opened with `FILE_FLAG_OPEN_REPARSE_POINT` and checked as a non-reparse regular file (`safe_fs.rs:1485-1505`, `1652-1711`). The target is passed as a full path with `RootDirectory = null`, so it is path-based; the retained parent and its ancestors deny delete sharing, which keeps that path stable.
- Windows `remove_file_if_hash_impl` hashes and deletes through one handle that shares only `FILE_SHARE_READ` (`safe_fs.rs:1529-1572`), so the hashed bytes cannot change before deletion.
- The forward apply records the post-placement hash as `after_sha256` and releases the quarantine without comparing that hash with the staged hash (P2-2).
- A failure after the rename succeeded is reported as "disappeared and nothing was changed" when the destination is absent (P3-1), and any I/O or journal error after the rename leaves the destination absent until rollback (P2-4).

## Rollback findings

- The forward-quarantine branch (`transaction.rs:6034-6109`) runs only after the early `rollback_destination_is_restored` return (`transaction.rs:6008`), so a forward quarantine beside a destination that already holds the backup bytes is never inspected and is left behind while rollback reports success (P2-1).
- The changed-bytes rule in `rollback_destination_is_restored` (`transaction.rs:5012-5036`) compares `quarantine_sha256` with `backup_sha256`, while the schema and model describe the comparison as against `before_sha256`. The forward-quarantine branch also overwrites the forward operation's `quarantine_sha256` with the hash rollback moved back (`transaction.rs:6065`). Behavior is correct, but the field's documented meaning drifts (P3-3).
- `settle_interrupted_quarantine` (`transaction.rs:3809-3836`) releases only when the held hash equals the journaled verified hash and the destination is in a completed state; otherwise it moves the quarantine back into an absent destination or keeps both files. I found no path where it deletes bytes that differ from both the precondition and the backup.
- `prepare_rollback_transaction` still captures the child backup by path (`symlink_metadata`, `sha256_file`, `copy_atomic` at `transaction.rs:5510-5550`). The hash and the copy are separate reads, so `before_sha256` and `backup_sha256` can disagree under a concurrent edit; later checks catch the disagreement, so no bytes are lost (part of open P1 item 1).
- The fix that accepts `rollback_applying` for the check refusing a user file created in the apply window (`transaction.rs:6131-6170`) is reachable now, and `file_created_in_the_apply_window_is_never_clobbered` covers it.

## Findings by severity

### P0

None found.

### P1 (open, inherited, release-blocking)

P1-A. Handles-handoff items 1, 3, and 4 remain open.
External destinations still take their precondition and post-apply hashes by path (`transaction.rs:3224-3233` and `3385-3394`), and so do `rollback_live_hash` for external destinations (`transaction.rs:5094-5096`), `post_install_checks` (`read_file_path` at `transaction.rs:3967`), and `prepare_rollback_transaction`.
The quarantine closes the replace and delete race at the leaf, because the quarantined bytes are re-hashed through the retained handle; it does not close the path-swap gaps that item 1 lists.
Item 2 is implemented, but it cannot close until the Unix routes run natively (see P2-5).

### P2

P2-1. Recovery finds a quarantine only through the journal, although its name is fully derivable, and nothing sweeps for orphans.
Evidence: the forward-quarantine branch requires `journaled_quarantine_leaf(...)` to return `Some` (`transaction.rs:6034`) and runs only after the early restored check at `transaction.rs:6008`; replay skips any checkpoint whose wall-clock `updated_at` is not later than the snapshot (`transaction.rs:2062`).
Scenario A: during forward apply the system clock steps backwards (an NTP correction) after `mark_project_apply_started` persisted the journal, and the process then dies after a delete moves `descriptor.mod` into quarantine. On replay the batch intent and the quarantine intent are skipped, the operation reads as `pending`, `rollback_operation_is_actionable` skips it (`transaction.rs:5137`), and rollback reports success while the user's file exists only as a hidden `.hoi4ms-quarantine-*.tmp`. For a managed delete whose intent survived but whose quarantine intent was skipped, `interrupted_delete_completed` restores the backup over the absent destination and orphans possibly newer bytes in the quarantine.
Scenario B: the forward hash mismatches, the move-back finds a new file at the destination whose bytes equal the backup (for example a sync client re-creating it), and the early restored check returns `true`. Rollback completes and leaves the user's changed bytes in the quarantine with no report.
Scenario C (Unix `linkat` fallback): a crash between `linkat` and `unlinkat` leaves the destination and the quarantine as two links to one inode; rollback treats the destination as restored and leaves a permanent hidden hard link.
Fix: before the restored check, probe the derived name `quarantine_leaf_name(txid, op.id)` whether or not it is journaled; never mark an operation `rolled_back` while its derived forward quarantine exists unless its bytes equal the restored bytes, and then release it; finish rollback and finalization with a sweep that fails or reports any surviving derived quarantine for the transaction; replace the wall-clock replay guard with a monotonic per-journal sequence number.

P2-2. Forward apply releases the quarantine before checking that the placed bytes are the staged bytes.
Evidence: after placement, `after_hash` is read back by path relative to the root and stored as `record.after_sha256` with status `verified` (`transaction.rs:3361-3418`); the quarantine is then released at `transaction.rs:3434`, and the comparison with `result_sha256` happens only later in `post_install_checks` (`transaction.rs:3976-3990`).
Scenario: an editor or sync client writes `U` to the destination between placement and the readback. The journal records `after_sha256 = U` as verified, the original quarantine is released, and stage 9 fails. Rollback's generic path then accepts `current == after_sha256` as the installed bytes (`transaction.rs:6112-6119`), moves `U` into its child quarantine, restores the backup, and releases `U`. The user's edit survives only inside the application-data child backup and is gone from the project without any message.
Fix: in `apply_operations`, require `after_hash == staged_hash` (for a delete, require an absent destination) before recording `verified`; on mismatch, keep the status `applying` with no `after_sha256`, do not release the quarantine, and fail as a changed-local-file conflict. Recovery then takes the existing refusal paths.

P2-3. The POSIX-semantics delete has no fallback, so on non-NTFS Windows volumes every replace fails at release and rollback cannot finish.
Evidence: `remove_file_if_hash_impl` and `remove_file_impl` use only `FileDispositionInfoEx` with `FILE_DISPOSITION_FLAG_POSIX_SEMANTICS` (`safe_fs.rs:1351`, `1557`), which Windows supports only on NTFS.
Scenario: a project on an exFAT USB drive or an SMB share. The first replace verifies and then fails in `release_quarantine` after its result checkpoint, so apply aborts. Rollback places the backup and then fails releasing its own child quarantine, and every retry fails again in `settle_rollback_step_quarantine`. The bytes are safe, but the transaction can neither finish nor roll back.
Before this change only managed deletes depended on this call; now every replace does.
Fix: on `ERROR_INVALID_PARAMETER` or `ERROR_NOT_SUPPORTED`, retry on the same deny-write handle with `FileDispositionInfo` (`DeleteFile = TRUE`), and add a preflight capability probe in the destination directory before apply begins.

P2-4. An error after the rename leaves the user's file hidden until a rollback succeeds.
Evidence: `directory.hash_file(&quarantine)?` (`transaction.rs:3752`), both `journal.record(...)?` calls, and the fault hooks return without trying to move the file back.
Scenario: the disk is full, so the "verified" or "changed" checkpoint fails; or a scanner holds the quarantine open so the hash fails. The destination stays absent. Rollback must first persist its parent checkpoint (`transaction.rs:6066`), so while the disk is still full rollback also fails before it moves anything back. No bytes are lost, but the user's file appears deleted.
Fix: on any non-fault error after a successful rename, try an exclusive move-back when the destination is still absent, and keep the journal intent so that recovery still applies if the move-back also fails.

P2-5. The Unix and macOS routes have never compiled natively or run.
Evidence: `rename_file_noreplace_impl` for Unix (`safe_fs.rs:1404-1481`) and `remove_file_if_hash_impl` for Unix (`safe_fs.rs:1510-1526`) have only a scratch macOS type check.
Specific risks: on macOS FAT, exFAT, and some SMB volumes, `renameatx_np(RENAME_EXCL)` returns `ENOTSUP` and the `linkat` fallback then fails with `EPERM` or `ENOTSUP`, so every replace fails partway through apply; the same holds on Linux CIFS without hard-link support. The `linkat` and `unlinkat` fallback is not atomic, so a crash leaves two links (see P2-1, scenario C) or a leaked `.hoi4ms-<uuid>.tmp` link from `write_atomic_noreplace_from`. Unix `remove_file_if_hash_impl` compares identity and then calls `unlinkat` by name, so a rename onto the quarantine name in between unlinks a different file, and a writer holding an earlier descriptor can still write after verification (both documented).
Fix: run the native matrix (handles item 4) and add `safe_fs` unit tests that force the `linkat` fallback through an internal entry point. Add the preflight probe from P2-3 so that an unsupported filesystem fails before project apply starts rather than in the middle of it.

P2-6. The documentation claims fault coverage that the tests do not provide.
Evidence: `docs/14_transaction_rollback.md:338` says the suite covers "every destination-quarantine boundary for forward apply, managed delete, rollback steps, success-lock commit, and rollback lock restore".
Not covered: forward replace after placement and before the result checkpoint; a crash between the changed-bytes record and the move-back; every boundary of the rollback restore of a forward quarantine (`MoveFrom`); every boundary of rollback created-file removal; lock commit `BeforeRename`; rollback lock restore `BeforeRename` and `AfterVerification`; and the rollback lock removal path for a first install. See "Missing fault tests".
Fix: add the tests, or narrow the claim until they exist.

### P3

P3-1. Misleading error after a successful rename (`transaction.rs:3739-3749`).
If `rename_file_noreplace` renamed the file and its directory sync or `verify_bound_to_path` then failed (on Unix this is plausible if the user moves the mod folder), the destination is absent and the user is told that it "disappeared and nothing was changed". The journal intent still lets rollback restore it.
Fix: inside the error branch, check whether the quarantine name exists before choosing the message.

P3-2. A failed release aborts the whole apply.
A sharing violation in `release_quarantine` (any other handle open with write access or without `FILE_SHARE_DELETE`) after the result checkpoint fails the transaction and forces a full rollback of an apply that otherwise succeeded.
Consider retrying a bounded number of times, or recording the quarantine as a pending release that finalization settles.

P3-3. Schema and model wording does not match the code.
The schema and `models.rs` say that a `quarantine_sha256` different from `before_sha256` marks preserved changed bytes, but the code compares it with `backup_sha256`, and rollback overwrites the field (`transaction.rs:6065`).
`dependentRequired` checks only that the key is present, so `"quarantine_leaf": null` with a non-null `quarantine_sha256` passes the schema.
The example sets both fields to `null` (`transaction-journal.example.json:155-156`), but the serializer never writes nulls.
The `1.0.0` prohibition exists only in the schema, because `migrate_journal` does not strip or reject these fields; the impact is low because identity-less `1.0.0` journals are inspect-only.
`OperationCheckpoint` stays at version `1.0.0` while it carries the new fields.

P3-4. The comment at `transaction.rs:6325` says both rollback records are durable, but the child record is unsynced (see Journal state findings).

P3-5. A documented risk remains: when rollback restores a forward quarantine into an absent destination and stops before its checkpoint, the retry re-captures the restored bytes as the child backup.

P3-6. Production builds still contain the fault and barrier hooks (`fail_at_quarantine`, `fail_at_lock_quarantine`, `live_mutation_barrier`), because `TransactionOptions` is not gated by `cfg(test)`.
`TransactionOptions` does not implement `Deserialize`, so IPC cannot set them; this follows the existing pattern.

## Missing fault tests

1. Forward replace with `fail_after_live_mutation` on an existing destination: assert a new-bytes destination beside a verified quarantine, then rollback through the forward-quarantine branch with `installed` matched through `expected_sha256`.
2. Forward mismatch with a fault after the `quarantine-changed` record and before the move-back: rollback must move the changed bytes back and must not overwrite them with the backup.
3. Rollback of a forward quarantine (`MoveFrom`) with each `rollback_quarantine_*` fault and `rollback_after_placement`: start from a `BeforeRelease` or after-placement forward crash, then retry.
4. Rollback created-file removal (`LiveChange::Delete`, no backup) with each child boundary.
5. Rollback lock removal for a first install (no predecessor) at `AfterRename`, `AfterVerification`, and `BeforeRelease`; lock commit `BeforeRename`; lock restore `BeforeRename` and `AfterVerification`.
6. A concurrent edit during rollback, using a rollback barrier between the rollback precondition and the child rename.
7. An edit between placement and the post-placement readback; this would fail today (P2-2).
8. A forward quarantine beside a destination that equals the backup; this would leak today (P2-1, scenario B).
9. Process-abort (`maybe_abort_for_test`) variants of at least `AfterRename` and `BeforeRelease`, so that a journal persisted by the outer error handler cannot mask a missing checkpoint.
10. A journal-write failure (disk full) at the intent and verified records (P2-4).
11. `safe_fs` unit tests: `rename_file_noreplace` returns `false` and leaves the source when the target exists; `write_atomic_noreplace` does not replace; `remove_file_if_hash` keeps a mismatched file; release under a Windows sharing violation; and the forced Unix `linkat` fallback.
12. External-destination quarantine at each boundary.

## Recommended fix order

1. P2-2: compare `after_hash` with the staged hash before `verified` and before release. It is small and local, and closes the only path where the user's bytes leave the project silently.
2. P2-1: probe derived names in rollback before the restored check, add the end-of-rollback and finalization orphan sweep, and move replay to a monotonic sequence.
3. P2-3: add the `FileDispositionInfo` fallback and the preflight exclusive-rename and delete probe; the probe also serves P2-5.
4. P2-4 and P3-1: best-effort move-back after errors that follow the rename, and an accurate error message.
5. P2-6: add missing tests 1 to 5 and 7 to 11, then correct `docs/14_transaction_rollback.md:338`.
6. P2-5 and P1-A: the native Linux and macOS matrix, then handles items 1 and 3.
7. P3 cleanups: schema and model wording, the example nulls, the durability comment, and the checkpoint version note.

## Fix addendum (2026-10-03)

The fixes are uncommitted working-tree changes to `src-tauri/src/transaction.rs`, `src-tauri/src/safe_fs.rs`, `src-tauri/src/models.rs`, the journal schema and example, `docs/14_transaction_rollback.md`, the transactions skill, and `public-readiness-transaction-quarantine.md`.

### Dispositions

- P2-2 (risk 1), fixed. `apply_operations` reads the destination back and requires the staged hash, or an absent destination for a delete, before it records `verified`, `after_sha256`, or releases the quarantine. A mismatch fails as `local precondition changed during apply ... changed after the reviewed bytes were placed`, leaves the operation `applying` without `after_sha256`, and keeps the verified original in its quarantine. Rollback then refuses with both files kept. A new `LiveMutationBarrier::AfterPlacement` barrier drives the test, and a temporary revert of the check made that test fail.
- P2-1 (risk 2), fixed. `forward_quarantine_leaf` probes the derived name when the journal lacks one. The early "already restored" path calls `settle_forward_quarantine_beside_restored`, which releases a quarantine only when it holds the restored bytes and otherwise stops with both files kept (scenarios B and C). Beside installed bytes, the forward-quarantine branch moves a quarantine back only when it holds the journaled `quarantine_sha256`. `sweep_transaction_quarantines` runs after forward apply, in finalization resume, and after the last rollback step. It lists the destination directories of the transaction's operations, including the rollback transaction's step names, and settles each leftover by hash: equal-to-destination bytes and planned result bytes are released, precondition bytes move back during rollback or are released beside a verified result, and anything else, including unowned names, is kept and fails for manual review. It is bounded to 4,096 directories and 1,000,000 listed entries. Checkpoint replay uses a monotonic `sequence` on each record against a new optional journal field `checkpoint_sequence` (in the schema, forbidden in `1.0.0`), and falls back to timestamps only when both the journal and the record predate sequences (scenario A).
- P2-4 and P3-1 (risk 4), fixed for the window before placement completes. A hash error, a journal-write error, a placement error, or a rename whose sync or binding check failed now attempts an exclusive move back through `restore_quarantine_after_error` and reports the outcome in the error. Quarantine fault hooks still model a process stop and skip the move back. Errors after placement are left to rollback.
- P2-3 (risks 3 and 5, Windows part), fixed by fallback; no preflight probe was added. `set_delete_disposition` retries on the same validated handle with classic `FileDispositionInfo` only for `ERROR_INVALID_PARAMETER`, `ERROR_NOT_SUPPORTED`, or `ERROR_INVALID_FUNCTION`. The hash-then-delete guarantee holds because the hashed handle grants only read sharing until it closes; a pre-existing reader can keep the name delete-pending until it closes. This applies to `remove_file_if_hash`, `remove_file`, and directory removal.
- P2-5, open. The Unix `linkat` fallback, `renameat2`, `renameatx_np`, and Unix `remove_file_if_hash_impl` were not changed, built, or run. No Unix preflight probe was added.
- P2-6 (risk 6), fixed for the listed boundaries below. `docs/14_transaction_rollback.md` now lists exactly what is covered, says the faults are returned errors rather than process aborts, and lists what is not covered.
- P3-3, fixed. The schema, model, `docs/14`, and the quarantine handoff say `quarantine_sha256` is compared with `backup_sha256` and that rollback records the hash it moved back. A string `quarantine_sha256` now requires a string `quarantine_leaf` (an `if`/`then` beside `dependentRequired`), and the example omits the null fields. `migrate_journal` still does not strip quarantine fields from `1.0.0` journals because `migrations.rs` is outside the allowed scope.
- P3-4, fixed. When a rollback child quarantine is held, the child `rolled_back` record is now synced through `write_rollback_checkpoint`, so the comment's durability claim is true.
- Checkpoint version note, fixed. Records are written as format `1.1.0`, and replay accepts `1.0.0` and `1.1.0`.
- P3-2 (release failure aborts apply), P3-5 (retry re-captures restored bytes), and P3-6 (fault hooks in production builds) are unchanged.
- P1-A is unchanged and still release-blocking.

### Tests added

In `src-tauri/src/transaction.rs`:

- `edit_after_placement_is_kept_as_a_conflict_and_never_treated_as_installed` (missing test 7)
- `forward_replace_interrupted_after_placement_rolls_back_through_its_quarantine` (missing test 1)
- `crash_before_moving_changed_bytes_back_keeps_them_for_rollback`, using the new `QuarantineBoundary::BeforeMoveBack` (missing test 2)
- `rollback_restoring_a_forward_quarantine_settles_each_boundary_on_retry` (missing test 3)
- `rollback_removing_a_created_file_settles_each_boundary_on_retry` (missing test 4)
- `lock_commit_interrupted_before_its_rename_rolls_back_to_the_predecessor`, `rollback_lock_restore_remaining_boundaries_are_settled_on_retry`, and `rollback_lock_removal_after_a_first_install_is_settled_on_retry` (missing test 5)
- `forward_quarantine_beside_restored_bytes_is_settled_by_hash` (missing test 8)
- `errors_after_the_quarantine_rename_move_the_bytes_back`, with injected hash-read and journal-write errors (missing test 10, quarantine records only)
- `rollback_probes_the_derived_quarantine_name_when_the_journal_lacks_it`, `rollback_sweep_restores_the_quarantine_of_an_operation_left_pending`, `rollback_keeps_quarantines_it_cannot_vouch_for`, and `checkpoint_replay_orders_records_by_sequence_not_wall_clock`
- `quarantine_release_falls_back_to_the_classic_delete_disposition` (Windows)

In `src-tauri/src/safe_fs.rs` (missing test 11, except the sharing-violation release and the Unix `linkat` fallback):

- `exclusive_rename_and_write_never_replace_an_existing_leaf`
- `hash_checked_removal_keeps_a_mismatched_file`
- `posix_delete_support_errors_select_the_classic_fallback` (Windows)
- `classic_delete_fallback_keeps_the_hash_then_delete_guarantee` (Windows)

No existing hash-precondition, checkpoint, conflict, or fault test was weakened. The only edit to an existing test adds an `unreachable!` arm for the new `BeforeMoveBack` variant in `crash_at_each_quarantine_boundary_rolls_back_to_the_original_bytes`, whose matrix does not include it.

### Commands and results

All cargo commands ran from `src-tauri` with `CARGO_TARGET_DIR=C:/Users/klimp/Documents/Projects/hoi4-mod-setup/target/quarantine-fix` on Windows.

- Targeted `cargo test -p hoi4-mod-setup --all-features --lib -- quarantine checkpoint_replay rollback_ edit_after_placement forward_replace crash_ errors_after safe_fs concurrent_edit file_created managed_delete --test-threads=4`: 50 passed.
- Targeted lock tests (`lock_commit_interrupted rollback_lock_`): 4 passed.
- `cargo test -p hoi4-mod-setup --all-features -- --test-threads=4` on the final source: 492 passed, 0 failed, 3 ignored, in 268 seconds. `process::tests::reviewed_process_cancellation_interrupts_the_child_promptly` passed in this run.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all`: applied.
- `python scripts/validate_repository.py`: "Validated 10 repository integrity groups." A `jsonschema` Draft 2020-12 check also confirmed that the example validates, that a null `quarantine_leaf` with a string `quarantine_sha256` is rejected, and that `checkpoint_sequence` is rejected in a `1.0.0` journal.

### Still open

- Unix and macOS were not built or run, so every Unix route, including the `linkat` fallback and its two-link crash state, is unverified natively (P2-5, handles item 4).
- The classic delete fallback was exercised only by forcing it on NTFS; it has not run on a real FAT, exFAT, or SMB volume. No preflight capability probe exists, so a volume that rejects both dispositions still fails at the first release.
- Quarantine boundaries are modelled as returned errors; there are no process-abort variants (missing test 9).
- External-destination quarantines (missing test 12), the remaining managed-delete boundaries, a concurrent edit during a rollback step (missing test 6), a release under a Windows sharing violation, and native disk-full adapters are not tested.
- The sweep only covers directories that hold a current operation's destination, and an operation left non-actionable by a lost intent is recovered only through the sweep's hash rules.
- P1-A, P3-2, P3-5, and P3-6 remain as described above, and `migrate_journal` still does not enforce the `1.0.0` field prohibition.
