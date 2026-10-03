# Public-readiness destination quarantine handoff

Status: handoff P1 item 2 from `public-readiness-transaction-handles.md` is implemented and passes on Windows; it stays open until the Linux and macOS matrix runs natively.

## Files changed

- `src-tauri/src/transaction.rs`: quarantine engine, forward apply, managed rollback, lock commit and restore, recovery settlement, fault options, and tests.
- `src-tauri/src/safe_fs.rs`: exclusive same-directory rename, exclusive atomic copy and write, hash-checked removal, and an identity-verified fallback for the Windows directory flush.
- `src-tauri/src/models.rs`: optional `quarantine_leaf` and `quarantine_sha256` on `JournalOperation`.
- `docs/schemas/transaction-journal.schema.json` and `docs/examples/transaction-journal.example.json`: the new optional operation fields.
- `docs/14_transaction_rollback.md`: new "Destination quarantine" section, apply step, fault coverage, and Windows flush wording.
- `.agents/skills/hoi4-mod-setup-transactions/SKILL.md`: quarantine rules and fault expectations.
- `docs/development/handoffs/public-readiness-transaction-handles.md`: item 2 status.

## Design

`mutate_live_leaf` in `transaction.rs` is the single route for changing a live leaf.

1. An absent precondition places the new bytes with a synced temporary file and an exclusive rename. A file that appears in the window is kept and the operation fails with `local precondition changed during apply`.
2. An existing precondition first persists a synced operation checkpoint with `quarantine_leaf` and no `quarantine_sha256`.
3. The destination moves to `.hoi4ms-quarantine-<transaction id>-<operation id>.tmp` in the same directory through the retained parent handle (`RootedDir::rename_file_noreplace`), and the directory is synced.
4. The moved bytes are hashed. A mismatch is journaled, the bytes move back with an exclusive rename, and the operation fails as a changed-local-file conflict. If a new file took the name, both files are kept and the quarantine stays recorded.
5. A match is journaled as `quarantine_sha256`. New bytes are placed with `copy_file_atomic_noreplace_to` or `write_atomic_noreplace`; a managed delete places nothing.
6. After the operation result checkpoint is synced, `release_quarantine` removes the quarantine through `remove_file_if_hash`. On Windows the hashed handle denies write and delete sharing and performs the delete. On Unix the leaf is reopened and its identity compared before `unlinkat`.

Exclusive rename uses `FILE_RENAME_INFO` with `ReplaceIfExists = 0` on Windows, `renameat2(RENAME_NOREPLACE)` on Linux, and `renameatx_np(RENAME_EXCL)` on macOS. A Unix filesystem without an exclusive rename falls back to exclusive `linkat` plus `unlinkat`.

Recovery:

- Forward file crash points recover through rollback, because resume stays refused once project apply has started. Rollback moves a surviving forward quarantine back instead of copying the backup. When the destination holds the installed bytes, rollback first moves them into its own quarantine on the child operation and releases that quarantine after both rollback records are durable.
- Rollback treats preserved local bytes (`quarantine_sha256` different from the predecessor, quarantine absent, destination equal to it) as already restored.
- Rollback never deletes a quarantine whose hash differs from both the precondition and the backup; a quarantine beside unexpected destination bytes keeps both files and fails for manual review.
- A rollback retry settles the child step quarantine first: verified beside the completed destination means release, beside an absent destination means move back and retry. A child operation that reached its quarantine intent is not re-captured by the inverse-backup step.
- Lock quarantines use derived names (`install-lock-commit`, `install-lock-restore`) and expected hashes from `previous_lock_sha256` and `result_lock_sha256`. `finish_finalization` and `rollback_transaction` settle them before checking the lock. Finalization resume releases a verified commit quarantine beside the exact committed lock; beside an absent lock it restores the predecessor and leaves rollback as the route.
- The success-lock commit now refuses to write when the live lock differs from the journaled predecessor.

## Journal fields

- `quarantine_leaf`: optional string, pattern `^\.hoi4ms-quarantine-[0-9a-f]{32}-[A-Za-z0-9_-]{1,64}\.tmp$`. Recovery accepts only the name derived from the journal's transaction ID and the operation ID.
- `quarantine_sha256`: optional SHA-256 of the quarantined bytes; requires `quarantine_leaf` through `dependentRequired`.
- Both are omitted from serialization when absent, so existing journals and snapshots are unchanged. Schema `1.0.0` journals must not contain them; the journal schema version stays `1.1.0`.

## Call sites

Converted:

- Forward apply replace, create, and managed delete for project and external destinations (`apply_operations`). External parents are retained through `live_target`.
- Rollback backup restore and created-file removal for project and external destinations (`rollback_transaction`).
- Rollback restore of a surviving forward quarantine (new path in `rollback_transaction`).
- Success-lock commit (`commit_success_lock`, previously a replacing `write_atomic`).
- Rollback lock restore and lock removal (`restore_previous_lock`).
- Backup capture into application data (`copy_backup_from_root`, `capture_rollback_lock_backup`) now uses an exclusive copy so an existing backup name is never replaced.

Not converted:

- `prepare_rollback_transaction` still copies the live destination into the child backup through the path-based `copy_atomic`. It writes only a new application-data backup leaf checked absent just before, never a project destination.
- `capture_previous_lock` writes the predecessor lock bytes into application data with `write_atomic`; it does not mutate a project file.
- Journal, plan, readiness, rollback-record, and checkpoint writes in application data use their existing atomic writers; they are transaction-owned files, not user destinations.
- The lock quarantine name and hashes are derived rather than journaled because `TransactionJournal` (outside the allowed `JournalOperation` scope) has no lock-quarantine field.

## Related fixes found during the work

- The Windows directory flush added before this task failed on every write in this environment: `ReOpenFile` returns `ERROR_ACCESS_DENIED` for a write reopen of the retained directory handle even though a path open for write succeeds. `sync_windows_directory` keeps `ReOpenFile` first and, only on that error, opens the retained path for write and flushes after confirming the same 128-bit file identity. Without this, the transaction and `safe_fs` suites failed before any quarantine code ran.
- The rollback check that refuses an unverified destination whose bytes match neither the predecessor nor the planned result was unreachable, because the batch rollback intent rewrites the status to `rollback_applying` before the check reads `applying`. Rollback could therefore delete a user file created in the apply window. The check now also accepts `rollback_applying`, and `file_created_in_the_apply_window_is_never_clobbered` covers it.

## Tests added

All are in `src-tauri/src/transaction.rs`:

- `concurrent_edit_after_precondition_is_preserved_as_a_conflict`: a barrier edit between the precondition and the rename is preserved byte for byte, apply fails as a conflict, and rollback keeps the edit.
- `file_created_in_the_apply_window_is_never_clobbered`: a file created at an absent reviewed destination survives apply and rollback.
- `file_created_after_quarantine_keeps_both_user_files`: a file created after verification is kept, the original stays in its recorded quarantine, and rollback refuses with both files intact.
- `crash_at_each_quarantine_boundary_rolls_back_to_the_original_bytes`: before rename, after rename, after verification, and after the result checkpoint before release; resume is refused and rollback restores the original bytes with no quarantine left.
- `managed_delete_preserves_a_concurrent_edit`: a concurrent edit survives a managed delete, and a delete interrupted after verification is restored by rollback.
- `rollback_quarantine_interruptions_are_settled_on_retry`: rollback faults before rename, after rename, after verification, after placement, and before release all converge on retry.
- `crash_before_lock_quarantine_release_is_finished_by_resume`: finalization resume removes the predecessor-lock quarantine and keeps the new lock and file bytes.
- `crash_inside_lock_commit_restores_the_predecessor_for_rollback`: an interrupted lock commit restores the predecessor on resume and rolls back cleanly.
- `rollback_lock_restore_interruptions_are_settled_on_retry`: rollback lock restore faults after rename and before release converge on retry.
- `journaled_quarantine_names_are_bound_to_their_operation`: journaled names must equal the derived name, and unsafe operation IDs are hashed.

The existing stage and operation fault matrix, `fail_after_live_mutation`, and every earlier precondition and checkpoint test still pass unchanged.

## Commands and results

- `cargo test -p hoi4-mod-setup --all-features --lib transaction`: 82 passed.
- `cargo test -p hoi4-mod-setup --all-features --lib safe_fs`: 8 passed.
- `cargo test -p hoi4-mod-setup --all-features -- --test-threads=2` on the final source: 465 passed, 0 failed, 3 ignored (opt-in fixtures), in 2,128 seconds on Windows.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all`: applied.
- `python scripts/validate_repository.py` (the `pnpm validate` script; `pnpm` is not on this shell's PATH): "Validated 10 repository integrity groups."
- A scratch crate containing the Unix `rename_file_noreplace_impl` type-checked for `aarch64-apple-darwin` against `libc 0.2.189`. The full crate cannot cross-check for macOS here because `objc2-exception-helper` needs a C compiler, and no Linux target is installed.

## Remaining risks

- macOS and Linux were not built or tested natively. The `renameat2`, `renameatx_np`, `linkat` fallback, and Unix `remove_file_if_hash_impl` routes have only the scratch macOS type-check above.
- On Unix, a process that already held the destination open for writing can still write to the quarantined inode after verification. Release rehashes and rechecks identity immediately before `unlinkat`, which narrows but does not close that window. Windows denies write sharing during the hash-and-delete.
- When rollback restores a forward quarantine beside an absent destination and is interrupted before its checkpoint, the retry re-captures the restored bytes as the child's inverse backup. The inverse rollback then treats them as pre-rollback bytes; no bytes are lost because the inverse takes its own backup first.
- A conflict that keeps both files blocks rollback for manual review instead of rolling back the remaining operations.
- The Windows flush fallback reopens the directory by its retained path. Ancestor handles deny delete sharing and the identity is compared, but this is not a handle-only flush. Directory-entry durability across sudden power loss is still unproven.
- `docs/13_security_model.md`, `.agents/skills/hoi4-mod-setup-security/SKILL.md`, and `docs/development/handoffs/session-continuation-2026-10-03.md` still describe displaced bytes as unquarantined. They were outside this task's file scope and need an owner update.
- Items 1, 3, and 4 of the transaction-handles handoff remain open.
