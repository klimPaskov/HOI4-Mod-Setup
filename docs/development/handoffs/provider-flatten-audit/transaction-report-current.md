# Current transaction, recovery, and rollback audit

Audit date: 2026-07-28
Snapshot: `ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70` on `codex/bootstrap-hoi4-mod-setup`, with pre-existing uncommitted changes
Method: static read-only review; no source, schema, test, or existing documentation file was changed
Scoped files: `AGENTS.md`, `.codex/agents/hoi4setup_transaction_recovery_auditor.toml`, the transaction skill, `docs/10_merge_conflict_rules.md`, `docs/14_transaction_rollback.md`, `docs/15_update_repair.md`, `docs/31_ai_provider_profiles_and_chat_sources.md`, the installation-plan/journal/lock schemas and examples, `src-tauri/src/transaction.rs`, `src-tauri/src/migrations.rs`, `src-tauri/src/models.rs`, and their inline tests.

## Verdict

**FAIL.**

The current worktree has materially stronger nominal transaction behavior than the earlier handoff:

- the exact twelve-stage order is enforced;
- source revision, manifest, selected-byte, and checksum evidence is added to the journal;
- prepared bytes are revalidated before backup;
- staged bytes are validated and rehashed immediately before apply;
- apply intent is persisted before each live mutation;
- live results are checked after apply, during post-install checks, and once more before lock construction;
- a blocking readiness result prevents stage 12 and the success lock;
- the rollback-record artifact is hashed and checked during finalization recovery;
- restored rollback destinations are rehashed before completion;
- rollback and inverse rollback use child journals and inverse backups.

The acceptance gate still fails because interruption recovery is not sound at several irreversible rollback boundaries, ordinary rollback can overwrite a later user-edited lock, finalization does not bind the exact success-lock bytes, pre-apply resume tolerates disappearance of a predecessor lock, and the required fault suite does not cover these windows or the declared real failure classes.

## Findings

### TX-CURRENT-01 — High — Rollback retry fails after an actual file or lock restore

**Fail for resumable rollback and power-loss recovery.**

Before parent restoration, `prepare_rollback_transaction` creates or reopens a child journal and backs up each live post-transaction destination. If an inverse backup already exists, it requires that backup to equal the current live hash (`src-tauri/src/transaction.rs:2643-2748`, especially `2708-2719`).

The parent then:

1. persists `rollback_applying`;
2. restores or removes the destination;
3. verifies the restored live hash;
4. persists the parent `rolled_back` checkpoint;
5. persists the child `rolled_back` checkpoint.

Evidence: `src-tauri/src/transaction.rs:2896-3005`.

A process stop after step 2 or 3 but before both checkpoints leaves:

- the child inverse backup containing the post-transaction bytes; and
- the live destination containing the restored pre-transaction bytes.

On retry, `prepare_rollback_transaction` runs before `rollback_destination_is_restored`. Its existing-backup/current-live equality check fails, so the later reconciliation branch at `src-tauri/src/transaction.rs:2876-2894` is unreachable. The same problem occurs if the predecessor lock has been restored: `capture_rollback_lock_backup` requires the existing post-transaction lock backup to equal the now-restored live predecessor lock (`src-tauri/src/transaction.rs:2603-2640`, called on every retry at `2681-2687`).

There is a second terminal gap. The parent is persisted as `rolled_back` with `rollback_allowed: false` before the child captures the resulting lock state and becomes `completed` with inverse rollback enabled (`src-tauri/src/transaction.rs:3007-3054`). A stop in that interval leaves the parent non-retryable and the child still non-retryable.

The test named `rollback_resumes_an_operation_checkpoint_after_restore` does not reproduce this state. It manually changes the parent status and live file before the first child transaction is created, so the child backup captures the already-restored bytes rather than the post-transaction bytes (`src-tauri/src/transaction.rs:4594-4633`). Cross-process rollback coverage stops only after child backup creation, before file restoration (`src-tauri/src/transaction.rs:4129-4215`).

Impact: an interrupted rollback or inverse rollback can require manual repair even though the parent and child journals contain enough hashes to distinguish the inverse backup from the restored state. The parent can also claim `rolled_back` before its child rollback record is durable.

### TX-CURRENT-02 — High — Ordinary rollback does not protect later edits to the success lock

**Fail for later-user-work protection.**

Installation journals initialize `result_lock_sha256` and `result_lock_exists` to `None` (`src-tauri/src/transaction.rs:307-380`). Successful installation finalization never fills those fields; only completed rollback child journals record their resulting lock state (`src-tauri/src/transaction.rs:3028-3054`).

Consequently, the early lock guard in `prepare_rollback_transaction` has no expected installation-lock hash for an ordinary installation or maintenance rollback (`src-tauri/src/transaction.rs:2579-2600`, `2681-2684`).

`restore_previous_lock` later treats the presence of this transaction's rollback-record path as sufficient ownership of the current lock. When that path remains present, it restores the predecessor lock or removes a first-install lock without checking whether the current lock bytes changed after installation (`src-tauri/src/transaction.rs:3082-3166`, especially `3087-3101`, `3144-3158`).

Therefore, a user or newer tool can edit a parseable success lock while retaining its rollback record, and ordinary rollback will overwrite or delete that later lock work. The check also happens after managed files have already been restored, so a changed lock without the record can be discovered only after partial rollback.

The inverse-lock regression is stronger because rollback children do record `result_lock_sha256`; it tests only inverse rollback (`src-tauri/src/transaction.rs:4499-4541`). No scoped test edits a normal installation or maintenance lock and then starts its first rollback.

### TX-CURRENT-03 — High — Finalization recovery does not bind the exact success-lock bytes or live operation results

**Fail for false-success prevention after lock commit.**

Nominal ordering is correct: readiness and final live verification precede stage 12, the journal enters `finalizing`, the rollback record is written and hashed, and only then is the lock written (`src-tauri/src/transaction.rs:736-825`).

`finish_finalization` now verifies:

- journal/root/transaction identity;
- the lock's project ID and rollback-record reference;
- the rollback-record path and checksum;
- transaction/project/plan binding between the rollback record and journal; and
- operation IDs, statuses, `after_sha256`, and `after_exists` between those two journal artifacts.

Evidence: `src-tauri/src/transaction.rs:3177-3281`.

It still does not verify the exact lock bytes:

- the parent journal has no precommitted `result_lock_sha256`;
- the current lock is merely read and migrated;
- the only lock-content checks are project ID and rollback-record reference;
- lock file entries are not compared to journal operation observations; and
- current live destinations are not rehashed during finalization recovery.

A substituted but parseable lock with the same project ID and rollback-record reference can therefore convert a stale `finalizing` journal to `completed`. A later live edit between the crash and recovery is also not reflected in the completion decision.

The finalization tests reconcile the authentic lock written by the worker or manually put a completed journal back into `finalizing`; they do not substitute the lock, alter a lock file hash/provider/source field, or edit a live operation result (`src-tauri/src/transaction.rs:4129-4197`, `4967-5005`).

### TX-CURRENT-04 — High — Pre-apply resume tolerates a missing predecessor lock

**Fail for evidence-based maintenance resume.**

Resume correctly verifies journal identity, root binding, the exact serialized plan hash, operation expectations, live preconditions for mutating operations, and staged hashes (`src-tauri/src/transaction.rs:3289-3506`).

The predecessor-lock check is one-sided:

```text
if the lock exists -> require its hash to equal previous_lock_sha256
if the lock is missing -> perform no check
```

Evidence: `src-tauri/src/transaction.rs:3368-3377`.

If an interrupted maintenance transaction recorded a predecessor lock and that lock is later deleted, resume accepts the missing lock. The replay then calls `read_existing_lock`, sees no predecessor, and can build the result as though it were a first installation, dropping prior files, components, choices, local-modification evidence, optional states, and rollback history (`src-tauri/src/transaction.rs:444-455`, `488-528`, `1991-2351`).

Resume also skips live precondition checks for `skip` and `external` operations by continuing before destination inspection (`src-tauri/src/transaction.rs:3413-3418`). The final live verification can detect a changed skipped file later, but only after the replay has mutated other destinations.

No scoped resume test deletes a recorded predecessor lock or changes a skipped destination before replay. The checked resume test is a first-install create operation (`src-tauri/src/transaction.rs:4906-4938`).

### TX-CURRENT-05 — Medium — Incoming evidence for skipped operations and prepared-file identity is incomplete

**Partial for source/download/checksum evidence.**

Stages 2–4 now carry useful evidence:

- repository, exact revision, manifest hash, and manifest origin are recorded at source resolution;
- selected prepared destinations and actual hashes are recorded at selective download; and
- the same prepared bytes are revalidated at checksum verification.

Evidence: `src-tauri/src/transaction.rs:543-599`, `974-1056`.

However:

- skipped operations are not required to have prepared incoming bytes (`src-tauri/src/transaction.rs:1043-1051`);
- if prepared bytes are supplied for a skip, their hash is not compared to the operation's source/result hash because that binding is conditional on `action != Skip` (`src-tauri/src/transaction.rs:1023-1040`);
- `PreparedFile.destination` is never compared with the operation destination;
- `source_size` is not compared with the prepared byte length; and
- source/download/checksum stage evidence contains only destination/hash strings, not per-file source path, component, size, or revision.

This matters for a first-install keep/skip. `build_lock` can use the plan's un-revalidated source/result hash as the managed installed baseline while recording the local hash as a modification (`src-tauri/src/transaction.rs:2033-2122`). That behavior is intended to protect the kept file from future removal, but the incoming baseline can be asserted without the transaction seeing those incoming bytes.

The preserved-first-install test supplies prepared incoming bytes, but it does not prove that they are required or bound for a skip (`src-tauri/src/transaction.rs:4341-4375`).

### TX-CURRENT-06 — Medium — Provider/flatten invariants and managed credential-reference clearing are not closed in the transaction core

**Partial.**

Confirmed controls:

- provider IDs must resolve to the bounded registry;
- model is bounded and non-empty;
- optimization profile must match the provider;
- non-Codex plans cannot select `codex.config`;
- `flatten_chat_sources: true` is rejected for non-Codex providers; and
- provider/model/endpoint/profile/flatten fields are copied to the lock.

Evidence: `src-tauri/src/transaction.rs:34-66`, `2329-2344`; regression at `4731-4748`.

Residual gaps:

1. A non-Codex plan may retain non-empty `flatten_additional_files` while `flatten_chat_sources` is false. The transaction writes that preference to the lock. Neither plan nor lock schema has a provider/flatten conditional (`schemas/installation-plan.schema.json:37-57`; `schemas/installation-lock.schema.json:32-52`).
2. `ai_endpoint` is an arbitrary optional string in both schemas, with no provider-specific requirement or prohibition in the scoped transaction validation (`schemas/installation-plan.schema.json:44-45`; `schemas/installation-lock.schema.json:39-40`).
3. The scoped staging validator treats flattened outputs as ordinary extension-based files. It does not independently bind `flatten_additional_files` to generated operations or enforce the documented flatten collision/link/secret/aggregate-size rules (`src-tauri/src/transaction.rs:1211-1301`). Those checks may be owned by the out-of-scope flatten planner, but the transaction runner does not revalidate them.
4. `build_lock` starts from all previous optional-workflow entries and clears a credential reference on removal only for workflow IDs present in the new plan map (`src-tauri/src/transaction.rs:2239-2271`). A schema-valid removal plan with an empty `optional_workflows` map preserves the previous Meshy project reference, contrary to the requirement that managed removal clear project references without deleting the vault item.
5. `migrate_lock` performs version normalization and plain Serde deserialization but does not validate optional-workflow credential-reference format or provider/flatten cross-field invariants (`src-tauri/src/migrations.rs:242-294`; unrestricted model field at `src-tauri/src/models.rs:773-780`).

No transaction test covers non-Codex flatten extras, endpoint/provider conditions, flatten validation during staging, or credential-reference clearing during managed removal.

The scoped transaction code does not invoke an OS-vault delete, which is correct. The separate explicit credential-removal route itself is outside this audit's named files.

### TX-CURRENT-07 — Medium — All-skip managed removal is not represented as completed removal

**Fail for an important removal edge case.**

`managed_removal_operations` classifies a missing locked file as modified and emits `Skip` (`src-tauri/src/transaction.rs:3667-3715`). If every managed target is already missing or otherwise preserved, the plan contains only skips.

`validate_plan` recognizes a non-empty all-skip/all-delete removal as a signed-out removal (`src-tauri/src/transaction.rs:75-92`). In contrast, both readiness and lock construction require at least one `DeleteManaged` operation before treating the transaction as removal (`src-tauri/src/transaction.rs:1665-1737`, `1998-2007`).

An all-skip removal therefore:

- does not receive the nonblocking removal completion report;
- does not mark components removed in the refreshed lock; and
- may run ordinary readiness against a deliberately absent installation.

When a missing predecessor file is retained by a skip, `record_local_modification` substitutes the installed hash for the absent `local_sha256`, producing a local-modification record whose `current_sha256` does not represent an existing file (`src-tauri/src/transaction.rs:2358-2373`).

The removal tests cover a plan containing a delete and preservation of an existing modified file, but not an all-missing/all-skip installation (`src-tauri/src/transaction.rs:4281-4309`, `5104-5161`).

### TX-CURRENT-08 — Medium — Journal and schema evidence remains weaker than the operation contract

**Partial.**

New journal operations record ID, destination, ownership, component, action, location scope, external flag, precondition, expected/source/result hashes, rollback instruction, backup path/hash, status, and post-state (`src-tauri/src/transaction.rs:341-367`; model at `src-tauri/src/models.rs:841-885`).

They do not record:

- source path or source size;
- local-state classification;
- resolution/conflict choice; or
- a distinct staged hash separate from the observed live result.

`stage_files` writes the staged hash into `after_sha256` before live apply (`src-tauri/src/transaction.rs:1196-1206`). If apply intent is persisted and the process stops before replacement, rollback sees an `applying` operation whose `after_sha256` already equals the expected incoming bytes even though the destination may still have its `before_sha256`. This contributes directly to the retry ambiguity described in TX-CURRENT-01.

Skip/external operations are marked `verified` without recording `after_sha256`, `after_exists`, or an operation-specific checkpoint (`src-tauri/src/transaction.rs:1321-1333`).

Schema weaknesses:

- the plan schema requires at least 12 arbitrary stage strings, not the exact list/order (`schemas/installation-plan.schema.json:245-269`); Rust validation compensates;
- the journal schema does not require `transaction_kind`, `project_root`, `plan_sha256`, predecessor/result-lock evidence, or rollback-record checksum (`schemas/transaction-journal.schema.json:7-17`, `26-50`, `85-103`, `319-350`);
- journal stage count/order is unconstrained;
- journal operations require only ID/status/destination/ownership, permit null ownership, and leave action, precondition, expected hash, rollback, backup, and observed result optional (`schemas/transaction-journal.schema.json:146-280`);
- `before_sha256`, `expected_sha256`, and `after_sha256` have no SHA-256 pattern;
- `git_initialized` is declared twice (`schemas/transaction-journal.schema.json:89-91` and `316-318`); and
- `migrate_journal` defaults only `transaction_kind` before deserialization (`src-tauri/src/migrations.rs:287-295`).

Recovery correctly refuses an empty project-root binding, and rollback treats missing action/rollback metadata as a no-op. The schemas nevertheless cannot distinguish a current resumable journal from legacy audit-only data.

### TX-CURRENT-09 — Medium — Apply ordering and rollback-child stage coverage are not enforced

**Pass for installation stage names/order; partial for mutation ordering.**

The canonical stage array is exact (`src-tauri/src/models.rs:9-22`), `validate_plan` requires exact equality (`src-tauri/src/transaction.rs:23-33`), and `run_transaction` invokes all 12 in that order (`src-tauri/src/transaction.rs:529-808`).

The documented apply order is not independently enforced. `apply_operations` iterates `plan.operations` in caller order (`src-tauri/src/transaction.rs:1312-1441`). The validator does not sort or reject descriptors, merged files, managed leaves, or external launcher descriptors that appear in a different category order.

Rollback is itself a project mutation, but `new_rollback_journal` seeds all twelve stages as pending, `prepare_rollback_transaction` marks only stage 6 (`backup`) complete, and rollback completion never updates the other stage checkpoints (`src-tauri/src/transaction.rs:2469-2565`, `2643-2748`, `2788-3074`). A completed rollback child therefore does not present a truthful ordered twelve-stage record.

All operation-order fault tests use one create operation. No test supplies intentionally misordered categories or asserts stage status on a completed rollback child.

### TX-CURRENT-10 — Medium — A concurrent edit remains possible after final live verification and before lock commit

**Residual false-success race.**

`final_live_verification` is a significant improvement: it verifies skipped, deleted, and applied destinations against plan and journal evidence (`src-tauri/src/transaction.rs:1594-1658`) and is called after readiness at `src-tauri/src/transaction.rs:768`.

The success lock is written later, after in-memory lock construction, stage-12 start, transition to `finalizing`, rollback-record write/hash, and stage-12 completion (`src-tauri/src/transaction.rs:769-815`).

A local editor or another process can still change a managed or skipped destination in that interval. The lock is built from prepared/journal evidence rather than a final reread, so the transaction can write a stale success lock. There is no project-wide mutation lock or second destination check immediately before `install.lock.json`.

No scoped test edits a destination after `final_live_verification` but before lock commit.

### TX-CURRENT-11 — Medium — Fault injection proves no lock for logical markers, not recovery correctness at every boundary

**Fail for required fault coverage.**

The runner exposes only four logical injectors: before/after stage and before/after apply operation (`src-tauri/src/transaction.rs:14-21`). The matrix iterates all 12 stage indices and before/after one create operation, asserting only that the success lock is absent (`src-tauri/src/transaction.rs:5223-5274`).

Limitations:

- `fail_after_stage` fires before the completed-stage journal is persisted (`src-tauri/src/transaction.rs:936-953`);
- skip/external operations bypass `fail_after_operation` because they continue first (`src-tauri/src/transaction.rs:1314-1333`);
- only one create action is iterated, not replace, merge, rename, generate, delete, skip, or external;
- backup, staging-file, validation, readiness/health, Git, rollback-file, predecessor-lock, child-finalization, and journal-replacement boundaries have no logical injector;
- cross-process aborts cover only after installation rollback-record write, after success-lock write, after rollback child backup, and after inverse child backup (`src-tauri/src/transaction.rs:2774-2786`, tests at `4129-4279`);
- no injected disk-full, file-lock/antivirus, permission-denial, network-loss, command-timeout, cancellation, or journal-write failure exists in the scoped suite; and
- most tests assert only lock absence, not journal durability, recovery action, backup hashes, rollback completion, and final filesystem hashes.

The design document explicitly acknowledges that per-operation rollback resume and power-loss windows remain release work (`docs/14_transaction_rollback.md:151-153`). TX-CURRENT-01 shows that this is an active correctness gap, not only missing evidence.

## Stage coverage

| Stage | Current implementation evidence | Result |
| --- | --- | --- |
| 1. Preflight | Plan/root/app-data checks, plan persistence, initial journal, predecessor-lock capture; stage marker follows | **Partial** — much preflight work and predecessor-lock backup occur before stage start |
| 2. Repository source resolution | Exact repository/revision/manifest hash/origin evidence is journaled | **Pass as reviewed-plan evidence**; real resolution remains pre-transaction by accepted design |
| 3. Selective download | Prepared operation bytes are enumerated and hashed | **Partial** — skips need no incoming bytes; source path/size/component are absent |
| 4. Checksum verification | Prepared bytes are revalidated against reviewed result/source hashes | **Pass for mutating prepared files; fail for skip baseline evidence** |
| 5. Dry-run review | Approval flags and resolved plan conflicts are validated before stage marker | **Pass for supplied plan** |
| 6. Backup | Existing live targets and predecessor lock are copied, hashed, and journaled before apply | **Pass nominally**; power-loss durability of backup copies is not fault-tested |
| 7. Staging | Bytes are written outside live destinations with per-file journal updates | **Pass** |
| 8. Validation | Staged hash plus descriptor/PNG/TOML/JSON/AGENTS/wiki checks | **Partial** — no independent schema or full flatten-invariant validation in this runner |
| 9. Apply | Live precondition/staged hash recheck, durable intent, atomic copy/delete, result hash and existence | **Pass nominally**; category ordering is caller-controlled |
| 10. Post-install checks | Live files are re-read, parsed where supported, and compared to expected and journal hashes | **Pass** |
| 11. Readiness report | Report is atomically persisted; blocking core checks prevent stage 12 and lock | **Pass** |
| 12. Rollback record | `finalizing` state, rollback record, record hash, and stage completion precede lock | **Pass nominally; fail for exact success-lock binding and finalization/live race** |

## Journal state findings

### Confirmed controls

- Every new installation journal binds the canonical project root and transaction UUID (`src-tauri/src/transaction.rs:307-381`).
- Rollback and staging discard validate root, journal filename, and transaction-directory UUID before resolving/deleting paths (`src-tauri/src/transaction.rs:384-421`, `2788-2797`, `3510-3569`).
- `read_journal` always routes through `migrations::migrate_journal` (`src-tauri/src/transaction.rs:3170-3175`).
- `persist_journal` centralizes journal updates through `atomic_write_json` (`src-tauri/src/transaction.rs:849-852`). The helper implementation is outside the parent prompt's named file scope, so this audit confirms delegation but does not independently re-audit its fsync/rename implementation.
- Stage starts/completions and apply intent/result/verification use journal persistence (`src-tauri/src/transaction.rs:900-953`, `1387-1440`).
- The predecessor lock is captured before any fault-injectable stage checkpoint (`src-tauri/src/transaction.rs:524-535`).
- Finalization has a distinct state and verifies the rollback-record checksum (`src-tauri/src/transaction.rs:776-815`, `3177-3281`).

### Residual journal gaps

- Rollback child retries conflate “inverse backup must remain unchanged” with “live destination must still equal inverse backup,” preventing post-restore reconciliation.
- Parent rollback completion is persisted before child completion/inverse availability.
- Installation journals do not record the exact result-lock hash/existence.
- Conflict choice, source path/size, and observed skip state are absent from journal operations.
- A staged hash is stored in the live-result field before apply.
- Completed rollback child stage arrays are mostly pending.
- Schema-valid journals can omit safety fields required for current recovery.
- No journal-write-failure or power-loss-after-directory-sync regression exists.

## Apply and rollback findings

### Apply findings that pass

- Prepared bytes are revalidated before backup (`src-tauri/src/transaction.rs:575-599`, `974-1056`; test `4031-4061`).
- Replace/delete targets are backed up and the backup hash is checked before apply (`src-tauri/src/transaction.rs:1058-1138`).
- Staging remains outside live destinations and rejects link components (`src-tauri/src/transaction.rs:1140-1255`).
- Live preconditions and staged expected hashes are rechecked immediately before each live mutation (`src-tauri/src/transaction.rs:1335-1386`).
- `applying` intent is persisted before copy/delete (`src-tauri/src/transaction.rs:1387-1404`).
- Destination hash/existence is observed after apply and rechecked during post-install and final-live verification (`src-tauri/src/transaction.rs:1405-1440`, `1544-1658`).
- Windows uses `ReplaceFileW` for an existing destination, and other paths use same-parent rename (`src-tauri/src/transaction.rs:1445-1542`).
- Blocking readiness and every injected pre-lock stage/operation fault prevent a success lock (`src-tauri/src/transaction.rs:756-767`; tests `4842-4904`, `5076-5102`, `5223-5274`).

### Rollback findings that pass

- A separate root-bound rollback child and inverse backup set are created before parent restoration (`src-tauri/src/transaction.rs:2469-2749`).
- Parent and child both receive `rollback_applying` checkpoints before mutation (`src-tauri/src/transaction.rs:2896-2905`).
- Existing post-transaction file edits are refused by hash (`src-tauri/src/transaction.rs:2906-2928`; inverse test `4459-4497`).
- Created-after-delete files are refused (`src-tauri/src/transaction.rs:2914-2918`).
- Explicit skip/external/rollback-none and legacy missing metadata are non-destructive no-ops (`src-tauri/src/transaction.rs:2848-2869`; test `4543-4592`).
- Backup path containment and backup hashes are checked before restoration (`src-tauri/src/transaction.rs:2930-2978`).
- Restored destinations are rehashed before `rolled_back` is persisted (`src-tauri/src/transaction.rs:2982-3005`).
- Transaction-created Git cleanup happens before file restoration (`src-tauri/src/transaction.rs:2818-2838`).
- A completed child records resulting lock state and exposes a guarded managed-file/lock inverse (`src-tauri/src/transaction.rs:3028-3061`; happy-path test `4377-4457`).

### Rollback findings that fail

- Real post-restore retry is blocked by child backup recapture (TX-CURRENT-01).
- Parent completion precedes durable child completion (TX-CURRENT-01).
- Ordinary installation/maintenance lock edits are not hash-protected (TX-CURRENT-02).
- A user deletion after a replace is not treated as later work: when `after_sha256` exists but the destination is absent, the hash mismatch branch is skipped and rollback recreates the old backup (`src-tauri/src/transaction.rs:2906-2934`). This should fail closed for replace/merge/generate-over-existing operations.
- `rollback_destination_is_restored` treats restore-backup metadata with a missing `backup_path` as restored when the destination is absent (`src-tauri/src/transaction.rs:2413-2419`). That is conservative for deletion but can falsely complete a corrupted new journal that expected an original file.
- Rollback child stage state does not reflect its actual mutation flow (TX-CURRENT-09).

## Maintenance behavior

| Mode | Scoped behavior | Audit result |
| --- | --- | --- |
| Update | Missing → create; unmodified → replace; modified → skip/review; obsolete unchanged managed/generated → delete; merged/external obsolete → preserve | **Partial** — modification protection is sound, but three-way/provider-adapted planning is caller-owned and has no scoped update test |
| Repair | Healthy managed/generated files are explicit skips; missing non-merged files recreate; modified, merged, and external files are preserved for review | **Pass for default preservation**, with no parse-invalid/provider-adapted execution test |
| Reinstall | Missing files recreate; unchanged non-merged/non-external files replace; modified, merged, and external files skip for review | **Pass for preservation**, with no complete reinstall transaction regression |
| Managed removal | Deletes only unchanged non-merged/non-external lock files; preserves modified, merged, external, and local content | **Partial** — all-skip removal is not completed honestly; credential references are cleared only for workflow IDs supplied in the plan |
| Credential removal | No implicit OS-vault deletion in the transaction module | **Pass for non-deletion**; separate explicit removal and end-to-end reference clearing are not evidenced in scope |
| Provider/flatten lock refresh | Copies selected provider/model/endpoint/profile/flatten fields from plan | **Partial** — Codex-only extras and maintenance carry-forward invariants are not enforced against the predecessor lock |

The planner helpers preserve ownership in every emitted operation (`src-tauri/src/transaction.rs:3587-3885`). Tests confirm healthy repair no-op, modified-file preservation, merged ownership persistence, and merged removal preservation (`src-tauri/src/transaction.rs:4311-4339`, `4635-4700`, `5104-5221`).

No scoped test executes a complete update, repair, reinstall, or credential-clearing removal through `run_transaction`.

## Fault scenario matrix

| Scenario | Current evidence | Result |
| --- | --- | --- |
| Before each of 12 stage markers | Synthetic matrix across all indices | **Pass for no-lock assertion** |
| After each completed stage's durable journal write | Injector fires before completion persistence | **Missing** |
| Before/after mutating apply operation | One create operation | **Partial** |
| Skip/external after-boundary | `continue` bypasses after-operation injector | **Missing** |
| Replace/merge/rename/generate/delete operation faults | No matrix cases | **Missing** |
| Process termination after rollback-record write | Cross-process abort; no lock; resume refused | **Pass** |
| Process termination after lock write | Cross-process abort; authentic artifacts reconcile | **Pass**, but substitution/live-edit cases missing |
| Process termination after rollback child backup | Cross-process abort; rollback retries | **Pass before mutation only** |
| Process termination after inverse child backup | Cross-process abort; inverse retries | **Pass before mutation only** |
| Process termination after restored file, before parent checkpoint | No abort point; retry logic is unsound | **Fail** |
| Process termination between parent and child operation checkpoints | No abort point; retry logic is unsound | **Fail** |
| Process termination after predecessor-lock restore | No abort point; child lock-backup recapture is unsound | **Fail** |
| Process termination after parent `rolled_back`, before child `completed` | No abort point; neither journal is retryable | **Fail** |
| Missing predecessor lock before pre-apply resume | No test; accepted by code | **Fail** |
| Changed skipped destination before resume | No pre-replay check/test | **Fail** |
| Later file edit before inverse | Explicit refusal test | **Pass for changed existing file** |
| Later file deletion after replace | No test; old backup is recreated | **Fail** |
| Later lock edit before inverse | Exact child result-lock check | **Pass for inverse only** |
| Later lock edit before ordinary rollback | No expected lock hash; record-bearing edit overwritten | **Fail** |
| Prepared checksum mismatch | Static regression before backup | **Pass** |
| Staged checksum/validation mismatch | Rejection code; no boundary fault fixture | **Partial** |
| Blocking readiness | Explicit no-lock regression | **Pass** |
| Disk full | No scoped fault adapter/injector | **Missing** |
| File lock / antivirus lock | No scoped fault adapter/injector | **Missing** |
| Permission denial/loss | No scoped fault adapter/injector | **Missing** |
| Network loss | Source work is pre-transaction; no scoped recovery fault | **Missing** |
| Health/command timeout | No scoped fault fixture | **Missing** |
| Cancellation | No transaction cancellation checkpoint | **Missing** |
| Journal atomic-write failure | No injector/test | **Missing** |

## Data-loss and false-success risks

1. **Rollback recovery dead-end:** a power loss after actual file or lock restoration can make retry reject the intentional restored state (TX-CURRENT-01).
2. **Premature rollback completion:** the parent can be durable as `rolled_back` before the child/inverse record is durable, with neither journal permitting retry (TX-CURRENT-01).
3. **Later lock work overwritten:** ordinary rollback can replace or delete a user-edited lock that retains the transaction rollback-record path (TX-CURRENT-02).
4. **Substituted lock accepted as success:** finalization recovery can complete from a parseable altered lock because no exact result-lock hash is committed (TX-CURRENT-03).
5. **Maintenance history loss on resume:** disappearance of the predecessor lock is not rejected and replay can rebuild as a first install (TX-CURRENT-04).
6. **False source baseline:** a skipped first-install file can be locked against incoming hash/size evidence not revalidated by the transaction (TX-CURRENT-05).
7. **Project credential reference retained:** removal can preserve a Meshy lock reference when the workflow ID is omitted from the removal plan (TX-CURRENT-06).
8. **False removal state:** an all-missing/all-skip removal is not represented as removed and may record an installed hash as the current hash of an absent file (TX-CURRENT-07).
9. **Later deletion reversed without review:** ordinary or inverse rollback can recreate a replaced file after a user deleted it (apply/rollback section).
10. **Concurrent stale lock:** a live edit after final verification but before lock write can leave a success lock whose file hash is stale (TX-CURRENT-10).

No normal approved apply path was found that silently overwrites an already modified live file: backup and immediate live-precondition checks reject that case. The highest current risks are interruption dead-ends, later lock metadata loss, false completion/evidence, and incomplete removal rather than an unchecked happy-path file replacement.

## Exact test evidence reviewed

Checked-in static evidence:

- prepared-byte revalidation before backup: `src-tauri/src/transaction.rs:4031-4061`;
- cross-process finalization and rollback/inverse backup boundaries: `src-tauri/src/transaction.rs:4063-4279`;
- non-ready managed-removal report and signed-out validation: `src-tauri/src/transaction.rs:4281-4309`;
- healthy repair no-op and first-install preserved modification: `src-tauri/src/transaction.rs:4311-4375`;
- apply → rollback child → completed inverse: `src-tauri/src/transaction.rs:4377-4457`;
- inverse later-file and result-lock edit refusal: `src-tauri/src/transaction.rs:4459-4541`;
- explicit skipped-file rollback no-op: `src-tauri/src/transaction.rs:4543-4592`;
- nominal `rollback_applying` checkpoint test: `src-tauri/src/transaction.rs:4594-4633` — does not create the pre-existing inverse backup required to reproduce TX-CURRENT-01;
- immutable merged ownership and non-removability: `src-tauri/src/transaction.rs:4635-4700`;
- missing ownership rejection and Git remote approval: `src-tauri/src/transaction.rs:4702-4729`;
- non-Codex config/flatten boolean rejection: `src-tauri/src/transaction.rs:4731-4748`;
- rollback root binding and predecessor-lock happy path: `src-tauri/src/transaction.rs:4750-4840`;
- blocking readiness and apply-operation no-lock behavior: `src-tauri/src/transaction.rs:4842-4904`;
- pre-apply resume and post-apply resume refusal: `src-tauri/src/transaction.rs:4906-4965`;
- authentic finalizing reconciliation: `src-tauri/src/transaction.rs:4967-5005`;
- staging discard and root binding: `src-tauri/src/transaction.rs:5007-5074`;
- no lock before stage-12 checkpoint: `src-tauri/src/transaction.rs:5076-5102`;
- modified repair/removal/reinstall preservation and merged removal review: `src-tauri/src/transaction.rs:5104-5221`;
- 12-stage and one-create-operation synthetic no-lock matrix: `src-tauri/src/transaction.rs:5223-5274`;
- journal migration defaults only the transaction kind: `src-tauri/src/migrations.rs:458-469`.

Tests were not executed. The user authorized writing only this report, and invoking Cargo would create or update build artifacts. All test conclusions are static review of current checked-in/uncommitted source.

## Schema and example drift

The examples do not describe data accepted by the current transaction:

- The launcher plan operation uses `location_scope: "external_launcher"` but omits `external: true` and uses a `%USERPROFILE%` placeholder instead of an explicit absolute destination (`examples/installation-plan.example.json:70-84`). `validate_plan` rejects that scope/flag combination (`src-tauri/src/transaction.rs:184-193`).
- The lock example repeats the placeholder and omits `external: true` (`examples/installation-lock.example.json:83-95`).
- The plan's `op-001` is a merge with source hash `222…` and no result hash (`examples/installation-plan.example.json:100-113`), while the same transaction journal shows it as replace with source hash `444…` and result hash `666…` (`examples/transaction-journal.example.json:81-98`). Current prepared-byte validation would require the merged deterministic result to be explicitly bound.
- The journal example backup path `backup/AGENTS.md` does not match the enforced app-root/backups/transaction-ID/operation-ID path (`examples/transaction-journal.example.json:90`; enforcement at `src-tauri/src/transaction.rs:433-441`, `2420-2432`).
- The journal example has only the first seven stages, while every current journal constructor seeds all twelve (`examples/transaction-journal.example.json:16-80`; `src-tauri/src/transaction.rs:331-340`).
- The lock example rollback record uses a shortened ID, while finalization requires `transactions/<full UUID>/rollback-record.json` (`examples/installation-lock.example.json:156-158`; `src-tauri/src/transaction.rs:3203-3214`).

These examples are lower-precedence than code and schemas, but they are not safe fixtures for recovery or integration tests in their current form.

## Skipped meaningful scenarios

Absent from the scoped tests or not executable under the report-only constraint:

- process kill after rollback file restore and before either checkpoint;
- process kill between parent and child rollback checkpoints;
- process kill after predecessor-lock restore;
- process kill after parent `rolled_back` and before child `completed`;
- ordinary rollback after a parseable success-lock edit that retains the rollback record;
- ordinary rollback after success-lock deletion or rollback-record removal, with an assertion that no files change before refusal;
- finalization with a substituted parseable lock, altered lock file hashes/provider/source fields, missing lock, missing/corrupt rollback record, or a changed live destination;
- pre-apply maintenance resume after predecessor-lock deletion;
- pre-apply resume after a skipped/external destination changes;
- first-install skip with no prepared incoming bytes or mismatched supplied incoming bytes;
- source-size and `PreparedFile.destination` mismatch;
- user deletion after replace/merge/generate before rollback;
- all-files-missing/all-skip managed removal;
- managed removal clearing the project Meshy reference while retaining the OS-vault item;
- non-Codex plan with non-empty flatten extras;
- endpoint/provider conditional validation;
- flattened collision/link/secret/aggregate-size rejection inside transaction staging;
- complete update, repair, and reinstall transactions, including source/result hash divergence and merged/provider-adapted bytes;
- deterministic multi-operation category order;
- completed rollback child's twelve stage states;
- every operation action under before/after logical and process-kill faults;
- disk full, file lock, permission denial, network loss, timeout, cancellation, and journal-write failure;
- Windows/macOS native interruption recovery and cloud-sync stabilization.

## Recommended parent actions

1. Separate child inverse-backup verification from live restored-state verification. On retry, validate the child backup against its recorded `backup_sha256`, then allow the live destination to equal either the recorded post-transaction precondition or the expected restored result according to the parent/child checkpoints.
2. Add a rollback finalization state. Do not persist the parent as non-retryable `rolled_back` until the child result-lock evidence and rollback record are durable. Make a retry after predecessor-lock restoration recognize the already-restored lock without recapturing it as a new inverse precondition.
3. Serialize the exact success lock before `finalizing`, persist its expected SHA-256 and existence in the parent journal, and verify that hash during finalization and before ordinary rollback. Check ordinary rollback's current lock before changing any managed file.
4. During finalization recovery, compare lock file entries and current live destinations with journal operation observations, not only rollback-record/journal equality.
5. Make pre-apply resume require both directions of predecessor-lock evidence: recorded predecessor present → same live hash; recorded predecessor absent → live lock absent. Recheck skip/external preconditions before replay mutates anything.
6. Require and hash-bind incoming bytes for every skip whose incoming source/result hash becomes a lock baseline. Compare prepared destination and source size with the operation.
7. Add explicit journal fields for source identity, local state, conflict choice, staged hash, and observed live result. Do not use `after_sha256` for staged bytes.
8. Make managed removal clear all project optional-workflow credential references regardless of which workflow IDs appear in the plan, while leaving the OS-vault entry untouched. Handle all-skip removal as an honest completed removal and represent absent files without a fabricated current hash.
9. Enforce `flatten_additional_files.is_empty()` unless Codex flattening is selected, add provider/endpoint cross-field schema rules, and transaction-revalidate the prepared flat artifact set or persist a verified flatten manifest/hash.
10. Enforce deterministic operation category ordering in the transaction core. Give rollback children truthful stage transitions or define and schema a separate rollback stage model.
11. Add process-kill hooks after each rollback mutation, after each parent/child checkpoint, around predecessor-lock restoration, and around parent/child finalization. Assert retry outcome and final hashes, not only absence of a success lock.
12. Add injected filesystem/network/journal adapters for disk full, file lock, permission denial, network loss, checksum/validation failure, timeout, cancellation, and atomic-journal-write failure.
13. Tighten the schemas so current resumable journals require root, plan, operation, predecessor/result-lock, and rollback-record evidence; retain legacy readability through an explicit legacy schema/migration path.
14. Replace the transaction/lock/journal examples with a mutually consistent, implementation-valid set using a full UUID, explicit absolute external path plus `external: true`, all twelve journal stages, current backup paths, and separate source/result hashes for merge/generated output.
