# Public-readiness transaction filesystem handoff

Status: ancestor link redirection is partially mitigated and project-root
identity is now persisted and checked; the P1 transaction race finding remains
open and blocks a public release.

## Implemented in the current candidate

- Added `src-tauri/src/safe_fs.rs` with a project-owned `RootedDir` facade.
- Unix opens components with no-follow `openat` flags and performs reads,
  writes, append, rename, delete, and directory enumeration through retained
  directory descriptors.
- Windows opens and retains each ancestor handle and rejects raw reparse-point
  attributes. Deletion and new-target rename use opened handles; replacing an
  existing file still calls `ReplaceFileW` while its retained parent chain is
  open. Read-only handles permit delete sharing; mutation roots do not.
- Routed atomic JSON writes, file hashing, backup copies, staging writes,
  executable metadata, checkpoint append/replay, managed delete, and staging
  discard through the rooted filesystem layer.
- Added a transaction source regression that rejects ambient `std::fs` reads
  and mutations from production transaction code.
- Added native Windows link/junction checks for final leaves, ancestor swaps,
  and recursive staging removal. The same tests use symlinks on Unix.
- Added platform-scoped directory identity tokens. Plans bind existing project
  roots or the existing parent of a new root; journals retain those identities
  and the identity of a created root. Recovery rejects identity drift, and
  apply rechecks the root before each operation.
- Existing-project scans record the opened root identity, and semantic review
  and plan construction reject a different directory at the same path.
- Windows `RootedDir` acquisition walks lexical components without
  canonicalizing through a possible junction and uses versioned 128-bit
  `FILE_ID_INFO` identities. Plan and journal schemas are version 1.1; legacy
  identity-less journals remain inspect-only.
- New project roots are created through the reviewed parent handle. An
  interrupted create before the root identity checkpoint is treated as
  ambiguous; rollback retains the empty root for inspection.
- Added tests that replace an existing project root and a reviewed parent at
  the same path, plus the crash-safe behavior for ambiguous new roots.
- Forward backup, apply, final verification, lock construction, and lock commit
  retain one reviewed project capability. Managed rollback file/lock operations
  and finalization's project-file and success-lock checks retain it as well.

## Verification

On 2026-10-03 the Windows all-feature suite passed 432 Rust tests with one
existing opt-in scan fixture ignored. The path tests preserve outside
sentinels during static link, ancestor junction, and staging-tree cases.
`cargo clippy` must be rerun against the final candidate.

On 2026-10-04, after the external-parent binding in item 1, the Windows all-feature suite passed 500 Rust tests with 3 opt-in fixtures ignored, `cargo clippy -p hoi4-mod-setup --all-features --all-targets -- -D warnings` passed, `cargo fmt --check` passed, and `python scripts/validate_repository.py` validated 10 integrity groups. The same day a clean clone on WSL Ubuntu 24.04's native ext4 filesystem (Rust 1.88.0, default features) passed 425 tests with 3 ignored and `cargo clippy -p hoi4-mod-setup --all-targets -- -D warnings`. macOS was not built or run.

On 2026-10-04, after the second tranche of item 1 (plan-time external-parent binding and the missing-bound-parent rule), the Windows all-feature suite passed 510 Rust tests with 3 opt-in fixtures ignored, and all-feature `cargo clippy ... -D warnings`, `cargo fmt --check`, and `python scripts/validate_repository.py` (10 integrity groups) passed. A clean clone on WSL Ubuntu 24.04 ext4 (Rust 1.88.0, default features, so the `commands.rs` wiring test is not compiled there) passed 434 tests with 3 ignored and `cargo clippy -p hoi4-mod-setup --all-targets -- -D warnings`. macOS was not built or run.

## Remaining P1 work

1. Partially closed on 2026-10-04 for transaction files (audit finding P1-A in `audit-transaction-quarantine-2026-10-03.md`); still open for the boundaries listed last.

   Now handle-based, with the external destination parent bound by identity:

   - Plan-time binding (2026-10-04, second tranche): `build_plan` and `build_maintenance_plan_blocking` in `commands.rs` call `transaction::bind_plan_external_parents` before the plan is stored for review. It opens each external destination's parent with `RootedDir::open_read`, records its identity as `PlanOperation.external_parent_identity`, and requires a fresh hash of the destination through that same handle to equal the reviewed `local_sha256`, so the bound folder is the one whose contents were reviewed. `validate_plan` rejects the field on a project operation, and the plan schema (still `1.1.0`, field optional, forbidden for `1.0.0` and for `external: false`) documents it.
   - Binding into the journal: `new_journal` copies the plan's identity into the journal operation, so no stage binds whatever occupies the path when the transaction starts. `run_transaction` calls `validate_plan_external_parent_identities` next to the project-root identity check, before any transaction storage exists: a different folder, a link, or a missing folder for a mutating operation is refused. `backup_existing` verifies the identity again and binds `external_parent_identity` itself only when the plan carried none (a parent absent at review, or a plan from before this tranche). `run_transaction` copies the identities of an interrupted journal into the replay's fresh journal and refuses one that differs from the plan's, and `resume_transaction_with_options` hashes a bound external destination through a handle with that identity before the replay writes a new journal, so a refused replay leaves the interrupted journal resumable.
   - `apply_operations`: one retained target per operation, project or external, takes the precondition hash, performs `mutate_live_leaf`, sets Unix executable state, and reads the result back. The external parent must have the bound identity; an unbound external operation is refused at apply. The old path-based `sha256_file` precondition and post-apply readback are gone.
   - `post_install_checks`: reads each destination through the retained project capability (`project_directory`, now a parameter) or the bound external parent; the bytes that `validate_managed_bytes` checks are the bytes that are hashed. Managed-delete absence and Unix executable state use the same handle.
   - `final_live_verification` and `finish_finalization`: external destinations are hashed through the bound parent instead of `regular_file_hash` by path.
   - `rollback_live_hash` and `rollback_live_executable` (and therefore `rollback_destination_is_restored`): external destinations are read through the bound parent. The main rollback step in `rollback_transaction` now opens one retained target and uses it for the live precondition, the restore or removal, the post-restore hash and executable check, and the release of its step quarantine. Quarantine settlement, the forward-quarantine branch, and `sweep_transaction_quarantines` open external parents only with the bound identity.
   - `prepare_rollback_transaction`: the child backup reads the live destination through the retained project capability or the bound external parent, and `RootedDir::copy_file_atomic_noreplace_hashed_to` hashes the bytes while copying them from one opened handle, so `before_sha256` and `backup_sha256` are the same value by construction. A project operation without a retained project capability is refused when the project root exists. The path-based `copy_atomic` helper was removed.
   - A bound parent is never recreated, and a link or non-directory at its path is identity drift. Rollback journals copy `external_parent_identity` from their parent operation, schema `1.0.0` journals carrying it are rejected by `migrate_journal`, and journal operations without it keep the earlier path-only behavior in rollback.
   - Missing bound parent (2026-10-04, second tranche): `existing_operation_target` returns `missing_bound_external_parent` (a `PathSecurity` error naming the folder and asking for it to be moved back) instead of `None` when a bound parent is missing and `operation_may_have_changed_destination` holds, that is, a leaf-changing action whose status is past `pending` and `staged`. The quarantine sweep applies the same rule, and `prepare_rollback_transaction` checks it explicitly because child rollback records stay `pending`; it runs before the batch rollback intent, so a refused rollback leaves every parent operation `verified`, no project file changed, the journal `rolling_back` with rollback allowed, and the same rollback finishes once the folder returns. Rule for "requires nothing": a skip, an external action, a legacy record without an action, and an operation still `pending` or `staged` cannot have changed anything in that folder, so a missing parent still reads as absent for them, and an unbound legacy record keeps the absent reading. An operation only marked `applying` by a batch intent counts as possibly changed, because the journal cannot prove it was not reached. A deleted launcher folder therefore blocks rollback of a changed launcher descriptor until it is restored or the journal is reviewed manually; there is no override.

   Still path-based or not identity-bound, and still release-blocking:

   - A parent that does not exist when the plan is built stays unbound in the plan; the backup stage binds the folder it first opens (the reviewed destination was absent, so only an empty or reviewed-equal folder passes the precondition). The maintenance planners and `build_plan` still take the reviewed `local_sha256` by path before `bind_plan_external_parents` re-reads it through the handle, so a swap between those two reads is caught only by the hash comparison, not by identity.
   - `build_plan` is not exercised end to end by a test because it needs a resolved source; `every_plan_builder_binds_external_destination_parents` is a source-level guard on the wiring, and the binding itself is unit-tested.
   - Within one transaction call each step reopens the external parent and rechecks identity; the handle is not retained across stages or calls. Identity binding prevents a different directory from being used, but a parent swapped away and the same directory swapped back between steps is indistinguishable from no swap, which is the intended equivalence.
   - Subdirectories below the retained project root are still resolved by name at each use, so a subdirectory replaced by a plain directory between steps is not detected by identity, only by the content hashes.
   - The pre-replay project-file precondition in `resume_transaction_with_options` still hashes project destinations by path; the replay rechecks them through the bound capability.
   - Application-data paths (journal, plan, backups, staging, checkpoint log, rollback records, and the rollback retry backup check in `prepare_rollback_transaction`) are reopened by path. This was considered for the second tranche and deliberately not started: no app-data root identity is captured anywhere today, so it needs a new journal-level identity field (with a `1.0.0` rejection in `migrate_journal` and a schema branch), capture at `run_transaction` and `prepare_rollback_transaction`, and a retained transaction-directory `RootedDir` threaded through `persist_journal`, `atomic_write_json`, `append_operation_checkpoint(s)`, `compact_operation_checkpoints`, `read_journal`, discovery (`find_incomplete_transaction`), backup and staging opens (`RootedDir::open(&roots.backup)`, `open_or_create(&roots.staging)`, `backup_existing`, the staging writer, staging discard), the rollback-record writes and their `sha256_file` checks, finalization's record check, and `expected_operation_backup` readers. Each of those currently takes a `&Path`, so the change touches roughly forty production call sites plus the resume and discovery paths, and partial threading would give a misleading guarantee.
   - Created-root cleanup, Git, external actions (launchers and post-install runners), and readiness keep their path-based boundaries and were out of scope.
   - The new tests ran on Windows and on native Linux, where the apply-time parent swap test's rename succeeds and the retained descriptor keeps the write in the bound directory. macOS has not run them.

   Tests added: `external_parent_replaced_after_backup_blocks_resume_until_the_bound_directory_returns`, `external_parent_replaced_by_a_directory_after_apply_is_refused`, `external_parent_replaced_by_a_link_after_apply_is_refused`, `external_parent_swap_during_apply_cannot_redirect_the_launcher`, and `project_root_swap_during_post_install_checks_cannot_redirect_the_reads` in `transaction.rs`; `hashed_exclusive_copy_returns_the_hash_of_the_placed_bytes` in `safe_fs.rs`; `current_journal_keeps_external_parent_identity_and_legacy_operations_omit_it` and an extended `a_legacy_journal_with_quarantine_evidence_is_rejected` in `migrations.rs`. With the identity comparison disabled, the two plain-directory drift tests fail; the link variant is caught independently by the link rejection. On Windows the apply-time swap and the project-root swap are refused by the retained delete-denying handles, which the old code also held, so those two tests are evidence of handle retention rather than of the new identity check.

   Second-tranche tests in `transaction.rs`: `plan_binding_records_the_reviewed_launcher_parent_through_one_handle`, `external_parent_swapped_after_plan_review_is_refused_before_transaction_storage`, `the_journal_starts_from_the_plan_bound_parent_before_backup`, `plan_bound_parent_missing_at_apply_time_is_refused`, `rollback_of_a_created_launcher_stops_while_its_bound_parent_is_missing`, `rollback_of_a_replaced_launcher_stops_while_its_bound_parent_is_missing`, `rollback_of_a_kept_launcher_needs_nothing_from_a_missing_bound_parent`, and `a_missing_bound_parent_is_absent_only_for_operations_that_changed_nothing`; in `commands.rs`, `every_plan_builder_binds_external_destination_parents`. Each new check was removed in turn and the targeted tests rerun: removing the preflight comparison fails the swapped-after-review and missing-at-apply tests; removing the plan-to-journal copy fails the binding and journal-start tests; removing the fail-closed rule in `existing_operation_target`, in `prepare_rollback_transaction`, or in the quarantine sweep fails, respectively, the status-rule test, both rollback-stops tests, and the status-rule test's sweep assertions. With only the preflight check removed, the backup stage still refuses the swapped folder because the journal carries the plan identity; the preflight test detects the removal by asserting that no transaction storage was written.
2. Implemented on Windows for forward apply, managed delete, managed rollback restore and removal, the success-lock commit, and the rollback lock restore; see `public-readiness-transaction-quarantine.md`. The displaced leaf moves to a journaled same-directory quarantine through the retained parent handle, its bytes are verified before an exclusive placement, and every crash point recovers without losing user bytes. The Linux routes (`renameat2` and the `linkat` fallback) pass natively on WSL ext4; the macOS `renameatx_np` route is not yet compiled or run, so this item stays open until the macOS part of item 4 passes.
3. The successful local path-swap tests prove ancestor containment for selected
   cases; they do not cover every transaction stage or concurrent final-leaf
   edit. Add deterministic barriers for app-root, transaction, backup, staging,
   project, launcher-parent, lock, journal, apply, rollback, and staging discard
   swaps, plus crash points around sync and namespace changes.
4. The new native tests ran on Windows only. Run the same matrix on macOS and
   include the supported case-sensitive and case-insensitive filesystem routes
   before closing the finding.

`safe_join`, canonicalization, `symlink_metadata`, and `path_has_link_component`
remain useful for lexical checks and diagnostics. They do not authorize a
mutation. Do not describe the transaction boundary as race-proof until the
remaining root-identity, same-leaf, recovery, and cross-platform gates pass.
