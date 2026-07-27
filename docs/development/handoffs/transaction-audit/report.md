# Transaction and recovery audit

Date: 2026-07-26
Verdict: **Fail — the bounded implementation does not satisfy the transaction contract and can lose data or leave a false-success lock.**

## Scope and method

Static, read-only review of:

- `src-tauri/src/transaction.rs`
- `src-tauri/src/merge.rs`
- `src-tauri/src/migrations.rs`
- installation plan, transaction journal, and installation lock schemas
- corresponding plan, journal, lock, and conflict examples
- transaction skill and transaction/merge/maintenance/testing/acceptance documents
- inline tests in the three named Rust files

No production code, schema, example, test, documentation, or skill change is proposed by this handoff. No test command was run because this auditor was authorized to write only this report; build and test commands can create target or cache files.

The requested `hoi4setup_transaction_recovery_auditor` was launched in a standalone Codex invocation with `fork_context=false` and an explicit prompt containing the exact stages, files, accepted behavior, fault scenarios, forbidden writes, and handoff path. It did not produce a usable handoff, so its output is not relied upon below.

A separate concurrent implementation changed transaction-related models, security, source, and several bounded files while this audit was running. Three schema additions were briefly removed while their provenance was unclear, then restored exactly once the broader concurrent change was identified. This report was refreshed against the resulting bounded snapshot after it remained stable for 30 seconds. Findings distinguish the audited implementation from documented or schema-only intent.

Pinned bounded snapshot SHA-256:

- `src-tauri/src/transaction.rs`: `3452524A9765CAB110D95FF6E41A9D8845824F5C23ED80BFA1F50BB150886E08`
- `src-tauri/src/merge.rs`: `957E3EDAF1AE00E14AB08F5BA28C84320BDC96F4ADCAC9D83B5C35605353232B`
- `src-tauri/src/migrations.rs`: `9913D564EC898E62B95F46A17CB8EEB2715195D96942A27B7E99C7291E7E83D8`
- `schemas/installation-plan.schema.json`: `D58B3E13F9387128DA036831ED1847F3EB9BB4221395265192E19553713E7170`
- `schemas/transaction-journal.schema.json`: `EA714B3A16E2F6A42EF05D9A50E798DC1D4D03B18DB7D7FE5EE4DE3EA3E52D44`
- `schemas/installation-lock.schema.json`: `C9890FFCB2B98BC88357C874325EDD02F84F6D5CC69F71E41D88433B61A927AA`

## Executive findings

| ID | Severity | Finding |
| --- | --- | --- |
| TRX-001 | Critical | The runner records several of the twelve stages without performing their required work, performs checksum verification after backup during staging, and has no readiness implementation. |
| TRX-002 | Critical | Live replacement removes the destination before rename and has no per-operation intent checkpoint; an interruption can leave the live file absent while rollback ignores the operation. |
| TRX-003 | Critical | A path planned as absent is not rechecked for absence, and conflict choices are not bound to operations; a file created by the user during staging can be overwritten without backup. |
| TRX-004 | Critical | The lock is written before all required journal/rollback persistence is complete and remains after rollback; a failed or rolled-back transaction can still look successfully installed. |
| TRX-005 | Critical | Rollback is not checkpointed or resumable and can overwrite user work created at a path after a managed delete. |
| TRX-006 | High | Destination verification records any observed hash as success instead of comparing it with the expected incoming hash; delete, external, semantic merge, and readiness checks are omitted. |
| TRX-007 | High | Operation IDs need not be unique, and plan/journal schemas omit required ownership, conflict, expected-state, checkpoint, observed-result, and rollback evidence. |
| TRX-008 | High | Lock construction can emit schema-invalid component states and omits required merge result and rollback-record integrity evidence. |
| TRX-009 | High | There is no filesystem-evidence-based resume implementation; `recovery_action` trusts the journal recommendation alone. |
| TRX-010 | High | Update and credential-removal planners are absent; reinstall bypasses the conflict engine; managed removal can delete a merged file wholesale. |
| TRX-011 | High | Conflict signatures are not persisted, JSON arrays are merged by index without an identity key, and merge validation is disconnected from staged validation. |
| TRX-012 | High | Fault injection and tests do not cover every stage/operation boundary or any required concrete fault class. |
| TRX-013 | High | External operations are marked verified without execution, external-action approval is not enforced, and `chmod` has no metadata implementation. |
| TRX-014 | Medium | Backup and rollback do not record or verify backup hashes and metadata, and rollback records do not contain the declared rollback instruction. |
| TRX-015 | Medium | Journal atomicity is delegated to an out-of-scope helper and has no in-scope durability test; the bounded code still lacks required before/after boundary states regardless of helper behavior. |

## Detailed findings

### TRX-001 — false twelve-stage coverage and ordering

Required behavior is explicit in `.agents/skills/hoi4-mod-setup-transactions/SKILL.md:21-38` and `docs/14_transaction_rollback.md:23-71`.

Implementation evidence:

- `src-tauri/src/transaction.rs:188-227` only calls `stage_checkpoint` for preflight, source resolution, selective download, checksum verification, and dry-run review. No source resolution or download occurs there. The only source check is commit-format validation at `src-tauri/src/transaction.rs:33`.
- Prepared-file checksums are actually checked at `src-tauri/src/transaction.rs:514-524`, after backup has already run at `src-tauri/src/transaction.rs:230-244`. This violates “verify every file before staging” in `docs/14_transaction_rollback.md:37-39`.
- `src-tauri/src/transaction.rs:315-322` marks readiness complete without creating or evaluating a readiness report or core gate.
- Work for backup, staging, validation, apply, and post-checks runs before the corresponding `stage_checkpoint` call at `src-tauri/src/transaction.rs:229-293`. Git setup then runs at `src-tauri/src/transaction.rs:294-314`, after the post-install stage was already marked complete and outside that stage's fault hooks.
- The rollback-record checkpoint completes at `src-tauri/src/transaction.rs:324-331`; the record is only written afterward at `src-tauri/src/transaction.rs:332`.

The plan schema does not enforce the contract by itself: `schemas/installation-plan.schema.json:160-184` requires only at least twelve arbitrary strings, with no exact values, order, maximum, or uniqueness. Runtime `validate_plan` does enforce the exact vector at `src-tauri/src/transaction.rs:22-32`, but that does not make the recorded no-op stages real.

### TRX-002 — destructive replacement window is not recoverable

`apply_operations` persists only the transaction-wide `project_apply_started` flag before entering the loop (`src-tauri/src/transaction.rs:585-587`). It does not persist an operation intent containing the expected before/after states before mutation.

`copy_atomic`:

1. copies and fsyncs a temporary file (`src-tauri/src/transaction.rs:684-690`);
2. removes the existing live destination (`src-tauri/src/transaction.rs:691-693`);
3. renames the temporary file (`src-tauri/src/transaction.rs:694`);
4. returns to code that records `applied` only at `src-tauri/src/transaction.rs:651-660`.

If termination, file lock, permission loss, or rename failure occurs after removal and before the journal write, the live file is missing while the journal still says `staged`. `rollback_transaction` skips every status other than `applied` or `verified` at `src-tauri/src/transaction.rs:837-839`, so it does not restore the existing backup. This is a direct data-loss path.

The implementation is not an atomic replacement: it deliberately creates a missing-destination interval. It also does not fsync the destination directory after rename, contrary to `docs/14_transaction_rollback.md:73-75`.

### TRX-003 — live preconditions and conflict decisions do not protect new user work

Conflict validation at `src-tauri/src/transaction.rs:88-103` checks only that each conflict has a selected value contained in its option list. It never binds a conflict path/selection to the corresponding operation action or `resolution`. A conflict selected as `keep` can coexist with a `replace` operation.

Live precondition logic at `src-tauri/src/transaction.rs:609-630` compares a hash only when `local_sha256` is present. When an operation was planned with `local_state = absent` and `local_sha256 = null`, it does not require the destination to remain absent.

Failure scenario:

1. create operation is planned while the destination is absent;
2. backup skips it because it is absent (`src-tauri/src/transaction.rs:462-465`);
3. the user creates the file during staging;
4. apply observes a file but accepts it because absent-state is not rechecked;
5. `copy_atomic` removes and replaces the unbacked-up user file.

The same gap exists for an `unmodified` or other operation with a missing `local_sha256`. This violates `docs/10_merge_conflict_rules.md:3-22` and the live-precondition rule at `.agents/skills/hoi4-mod-setup-transactions/SKILL.md:61-67`.

### TRX-004 — false-success lock lifecycle

The success sequence is:

- construct the lock (`src-tauri/src/transaction.rs:323`);
- checkpoint rollback record and write its first snapshot (`src-tauri/src/transaction.rs:324-332`);
- write the project lock (`src-tauri/src/transaction.rs:333-335`);
- only then set/persist journal completion and rewrite the rollback record (`src-tauri/src/transaction.rs:336-345`).

If either persistence call after line 335 fails, the closure returns an error, the error path marks the in-memory journal interrupted (`src-tauri/src/transaction.rs:349-364`), but the successful-looking lock remains. The error-path journal write is itself ignored at line 364.

The lock is also not removed or replaced by `rollback_transaction` (`src-tauri/src/transaction.rs:832-891`). The rollback test explicitly tolerates a surviving readable lock instead of asserting its removal or rollback state (`src-tauri/src/transaction.rs:1109-1116`).

This violates the completion-artifact rule in `docs/14_transaction_rollback.md:77-87` and acceptance criterion 73 at `docs/22_acceptance_criteria.md:103-110`.

### TRX-005 — rollback can overwrite later work and cannot resume

For a managed delete, apply records `after_sha256 = null` because the destination is absent (`src-tauri/src/transaction.rs:633-657`). During rollback, the later-user-work guard runs only when an after hash exists (`src-tauri/src/transaction.rs:853-859`). A new file created by the user at that deleted path is therefore overwritten by the backup at `src-tauri/src/transaction.rs:861-864`.

Rollback persists once before the loop and once after all file and Git operations (`src-tauri/src/transaction.rs:842-843,872-891`). It has no per-operation intent, result, or verification checkpoint. If interrupted after restoring one file, the journal still labels that operation `applied`/`verified`; a retry sees the restored original hash rather than the old after hash and reports it as a user edit. Git initialization rollback exists at `src-tauri/src/transaction.rs:879-882`, but it is also uncheckpointed. Rollback cannot safely resume.

Rollback also:

- does not verify backup content against `before_sha256`;
- does not verify restored destination hashes;
- does not restore recorded metadata;
- is not a new transaction with its own backup, contrary to `docs/15_update_repair.md:23-25`;
- leaves the installation lock unchanged.

### TRX-006 — post-apply verification does not verify the expected result

Staged bytes are checked against expected hashes at `src-tauri/src/transaction.rs:545-574`, but staged content is not rechecked immediately before copy.

After apply, the implementation computes any live hash and stores that value as `after_sha256` (`src-tauri/src/transaction.rs:640-657`). Post-install checks recompute the live hash and compare it only with that just-observed value (`src-tauri/src/transaction.rs:711-723`). It never compares the live destination with `operation.source_sha256` or the prepared expected hash.

Consequences:

- tampering with staged content between validation and apply can become the accepted result;
- delete absence is never checked because deletes are skipped at `src-tauri/src/transaction.rs:704-709`;
- external actions and configuration/health checks are skipped; Git setup exists but runs after the post-install stage was already completed;
- semantic parsers and component validators required by `docs/14_transaction_rollback.md:53-63` are absent;
- lock files use prepared expected hashes, not journal-observed final hashes (`src-tauri/src/transaction.rs:738-763`);
- every component is assigned validation `pass` unconditionally (`src-tauri/src/transaction.rs:764-777`).

### TRX-007 — operation identity and journal schemas cannot support safe recovery

`validate_plan` checks destination duplication but not operation-ID uniqueness (`src-tauri/src/transaction.rs:63-87`). Duplicate IDs cause:

- backup filename collision at `src-tauri/src/transaction.rs:473`;
- first-match journal updates at `src-tauri/src/transaction.rs:482-488,531-540,651-660`;
- ambiguous rollback evidence.

The operation contract requires the fields listed at `.agents/skills/hoi4-mod-setup-transactions/SKILL.md:40-57`. Schema evidence:

- Plan operations require ID, action, destination, nullable source hash, local state, and rollback only (`schemas/installation-plan.schema.json:211-298`). They have no ownership, bound conflict decision, backup reference, checkpoint, or observed result.
- `resolution` is an unconstrained nullable string (`schemas/installation-plan.schema.json:281-286`).
- Journal operations require only ID, status, and destination (`schemas/transaction-journal.schema.json:106-153`). They omit action, source identity, expected precondition, expected incoming hash, ownership, conflict choice, rollback instruction, intent checkpoint, and an explicit observed existence/type result. The newly added `external` flag does not supply those missing semantics.
- Journal stages have no twelve-stage count/order constraint (`schemas/transaction-journal.schema.json:64-105`).

The journal example mirrors this insufficiency: `examples/transaction-journal.example.json:75-90` contains only status, destination, backup path, and before/after hashes.

### TRX-008 — lock construction loses or emits invalid state

The plan permits optional states including `ready` and `interest_recorded` (`schemas/installation-plan.schema.json:128-140`). `build_lock` copies an optional workflow state directly into the component state at `src-tauri/src/transaction.rs:764-777`. Lock component state allows only `installed`, `incomplete`, `not_selected`, `planned_unavailable`, `unsupported_platform`, and `removed` (`schemas/installation-lock.schema.json:71-109`). A normal `ready` 3D workflow or `interest_recorded` LoRA workflow can therefore produce a lock that violates its schema.

Other lock evidence losses:

- Generated ownership is now inferred for `OperationAction::Generate` at `src-tauri/src/transaction.rs:743-761`, but plan and journal operations still have no explicit ownership field.
- Lock merge choices always get `result_sha256: None` (`src-tauri/src/transaction.rs:812-821`), while `docs/10_merge_conflict_rules.md:76-87` requires the result hash. The example includes a hash at `examples/installation-lock.example.json:69-73`, so example and implementation disagree.
- Lock rollback references are strings built at `src-tauri/src/transaction.rs:825-828`, while the actual transaction record is written under the application-data transaction root at `src-tauri/src/transaction.rs:178-184,332-345`; no in-scope resolver or integrity hash binds the reference to that file.

### TRX-009 — resume is not implemented from filesystem evidence

`recovery_action` returns the journal's recorded recommendation whenever state is interrupted (`src-tauri/src/transaction.rs:900-905`). It does not inspect a destination, staged file, backup, before hash, after hash, or lock.

There is no resume function in the bounded implementation. `read_journal` directly deserializes bytes (`src-tauri/src/transaction.rs:894-897`) and does not call `migrate_journal` from `src-tauri/src/migrations.rs:51-53`. The migration tests cover generic project state only, not a lock or journal (`src-tauri/src/migrations.rs:61-72`).

This fails `.agents/skills/hoi4-mod-setup-transactions/SKILL.md:69-75` and `docs/14_transaction_rollback.md:93-99`.

### TRX-010 — maintenance planners are incomplete or unsafe

- **Update:** no update operation planner exists in the bounded implementation. There is no base/local/incoming comparison or unchanged conflict-signature reuse.
- **Repair:** `repair_operations` (`src-tauri/src/transaction.rs:911-953`) protects a modified file by returning skip and uses locked hashes, but it checks only file presence/hash. It does not cover parse-invalid generated output, MCP health, wiki coverage, or external dependency evidence required by `docs/15_update_repair.md:15-17`.
- **Reinstall:** `reinstall_operations` always emits `Replace`, `local_sha256 = None`, and `local_state = Unknown` (`src-tauri/src/transaction.rs:999-1016`). For an existing destination, apply rejects that operation at `src-tauri/src/transaction.rs:621-629`; no base/local/incoming conflict engine is invoked.
- **Managed removal:** `managed_removal_operations` ignores `file.ownership` and deletes every unchanged locked file (`src-tauri/src/transaction.rs:957-997`). An unchanged `merged` file is deleted wholesale, contradicting `docs/15_update_repair.md:27-29`, which requires reversing managed contributions and preserving merged files.
- **Credential removal:** there is no planner for removing project credential references and no separate explicit OS-credential deletion choice. The file-only removal planner does not implement the required behavior.
- **Rollback maintenance:** rollback is performed against the original journal rather than as a new staged transaction with backup and preview.

The only maintenance test checks that one modified managed file becomes skip for repair and removal (`src-tauri/src/transaction.rs:1183-1222`). It does not cover update, locked revision selection, missing/corrupt repair, merged removal, reinstall conflicts, credential references, or rollback-as-transaction.

### TRX-011 — merge/conflict evidence cannot safely support reuse

- Conflict IDs are based only on path (`src-tauri/src/merge.rs:100-110`). Base, local, and incoming hashes held by `ConflictDecision` are dropped when converted to `PlanConflict`.
- The plan conflict schema contains path/options/selected only (`schemas/installation-plan.schema.json:300-345`); it cannot prove the identical conflict signature required by `docs/10_merge_conflict_rules.md:85-87`.
- `apply_to_identical` has no component, ownership, state, action, or file-kind signature required by `docs/10_merge_conflict_rules.md:72-74`.
- JSON arrays of equal length are merged by position (`src-tauri/src/merge.rs:266-277`), contrary to the manifest-declared identity-key requirement in `docs/10_merge_conflict_rules.md:60-62`.
- `validate_merged_result` exists (`src-tauri/src/merge.rs:283-315`) but `validate_staging` only checks hashes and never calls it (`src-tauri/src/transaction.rs:545-574`).
- AGENTS handling is generic line merge plus template-token detection; the project adaptation and size/path/reference checks in `docs/10_merge_conflict_rules.md:52-54` are absent.

Merge tests cover binary exclusion, a simple conservative line case, and one TOML case only (`src-tauri/src/merge.rs:331-371`). There is no JSON identity/reordering test, AGENTS adaptation test, conflict-signature test, or transaction wiring test for merged-result validation.

### TRX-012 — fault injection and tests are far below the required matrix

The implementation exposes generic stage and operation index hooks at `src-tauri/src/transaction.rs:13-20`. Stage hooks are implemented at `src-tauri/src/transaction.rs:401-446`; operation hooks are at `src-tauri/src/transaction.rs:587-593,661-666`.

Limitations:

- stage hooks do not surround real stage work for backup through post-checks;
- source/download/readiness stages contain no real work to fail;
- skip/external operations `continue` at `src-tauri/src/transaction.rs:594-606`, bypassing `fail_after_operation`;
- there are no hooks around temp write, fsync, destination removal, rename, destination hash, applied-journal write, verified-journal write, rollback operation, rollback-record write, lock write, or final journal write;
- the hooks produce one generic transaction error and do not simulate process kill, disk full, lock, permission denial, network loss, timeout, cancellation, or journal-write failure.

Existing fault tests:

- one failure after operation 0 (`src-tauri/src/transaction.rs:1112-1146`);
- one failure before stage 11 (`src-tauri/src/transaction.rs:1148-1175`).

No test iterates all twelve before/after stage boundaries or every before/after operation boundary. This fails `.agents/skills/hoi4-mod-setup-transactions/SKILL.md:85-100` and `docs/20_testing_strategy.md:44-46`.

### TRX-013 — unsupported actions can still look verified

- `validate_plan` does not require `external_actions_reviewed`; it checks only dry-run approval (`src-tauri/src/transaction.rs:33-47`).
- `OperationAction::External` is marked `verified` without execution or observed evidence (`src-tauri/src/transaction.rs:594-606`).
- Post-install checks skip external operations (`src-tauri/src/transaction.rs:704-709`).
- Components are then marked validation `pass` (`src-tauri/src/transaction.rs:764-777`).
- `OperationAction::Chmod` falls through the ordinary staged-file copy path (`src-tauri/src/transaction.rs:504-540,632-639`); there is no permission/metadata application or verification.

These paths can report success for work that never occurred.

### TRX-014 — backup and rollback evidence are incomplete

`backup_existing` copies a file and records only its path (`src-tauri/src/transaction.rs:448-490`). It does not hash the backup, capture permissions/timestamps, fsync the backup, or verify the copy. Backup references are persisted only after the whole loop, so an interrupted multi-file backup has no per-copy durable ledger.

The plan has a rollback enum (`schemas/installation-plan.schema.json:290-296`), but `new_journal` does not copy it into `JournalOperation` (`src-tauri/src/transaction.rs:135-146`), and the journal schema has no rollback instruction (`schemas/transaction-journal.schema.json:106-153`). The rollback record is a journal snapshot, so it does not contain the declared restoration action required by the operation contract.

### TRX-015 — atomic journal writing is unproven in the bounded scope

`persist_journal` delegates to `atomic_write_json` (`src-tauri/src/transaction.rs:370-373`). The helper implementation is outside the user-bounded audit scope, so atomic replace, file fsync, and directory fsync cannot be confirmed here. No named in-scope test injects or observes a journal write failure.

Even if the helper is fully atomic, the state machine still violates the contract because it lacks a durable per-operation intent before live mutation, does not persist rollback progress, and performs required fallible writes after the lock.

## Twelve-stage coverage

| # | Stage | Implementation | Test evidence | Result |
| --- | --- | --- | --- | --- |
| 1 | Preflight | Plan, commit format, dry-run flag, root canonicalization/directory only; no disk, permission, lock, incomplete-journal, platform, or connectivity checks. | None specific. | Partial |
| 2 | Repository source resolution | Checkpoint only; exact source resolution/manifest compatibility is not performed. | None. | Missing |
| 3 | Selective download | Checkpoint only; prepared bytes are accepted from the caller. | None. | Missing |
| 4 | Checksum verification | Checkpoint is no-op; real prepared hash check occurs later in staging after backup. | No mismatch test. | Misordered partial |
| 5 | Dry-run review | Checks `dry_run_reviewed`; does not require external review or persist reviewed file/Git/rollback evidence. | Plan fixture sets approval true only. | Partial |
| 6 | Backup | Runs before apply, but lacks hashes/metadata/durability and stage hooks occur after work. | Happy-path replace only. | Partial |
| 7 | Staging | Writes outside live paths and records per-file staged hashes; no semantic generation/merge validation. | Happy path only. | Partial |
| 8 | Validation | Hash-only validation; no parser/schema/containment/wiki/component/merged-result checks. | None specific. | Partial |
| 9 | Apply | Deterministic plan iteration and some precondition checks, but unsafe replacement and missing intent/expected-hash verification. | One happy path and one fail-after-op case. | Unsafe partial |
| 10 | Post-install checks | Rehashes live files against just-observed journal hashes; skips delete/external and semantic/health checks. Git setup runs only after this stage is marked complete. | No failure test. | Unsafe partial |
| 11 | Readiness report | Checkpoint only; no report, check aggregation, or core gate. | None. | Missing |
| 12 | Rollback record | Checkpoint precedes record write; snapshot lacks rollback instructions/hashes/metadata and is rewritten after lock. | One fault before stage 12, not after record/lock writes. | Unsafe partial |

Declared stage order is validated exactly at runtime, but actual work does not follow the declared semantics: checksum verification occurs after backup, and stage 6–10 “before” hooks occur after their work.

## Operation-record and journal-state findings

| Required field/evidence | Plan | Journal | Lock/record | Finding |
| --- | --- | --- | --- | --- |
| Stable unique operation ID | String present | String present | Not retained per rollback instruction | No uniqueness/stability validation; duplicate IDs collide |
| Action type | Present | Missing | Inferred incompletely | Recovery cannot distinguish create/delete/merge from journal |
| Source identity | Optional path/hash | Missing | Present only for locked files | Delete/external/generated evidence is incomplete |
| Destination identity | Present | Present | Present for locked files | No file type/metadata identity |
| Expected precondition | Local state + optional hash | Only nullable before hash | Missing | Absent state is not enforced |
| Expected incoming hash | Nullable source hash | Missing | Prepared expected hash | Live result is not compared with expected |
| Ownership | Missing | Missing | Inferred as managed/merged/generated | Recovery still lacks explicit ownership; external ownership unavailable |
| Conflict choice | Separate unbound record | Missing | Path/choice only | Cannot prove operation followed the choice |
| Backup reference | Missing | Nullable path | No integrity binding | No backup hash or metadata |
| Stage status/checkpoint | Stage list in plan | Stage and operation status | No final transaction evidence | Hooks do not wrap real stages |
| Observed result | Missing | Nullable after hash | Lock uses expected, not observed | Delete absence/type not represented |
| Rollback instruction | Present | Missing | Missing from journal snapshot | Rollback infers behavior from backup presence |

## Apply and rollback findings

Positive evidence is limited:

- backup runs before the normal apply call (`src-tauri/src/transaction.rs:229-275`);
- staging precedes hash validation and apply (`src-tauri/src/transaction.rs:244-275`);
- an existing file with a populated `local_sha256` is rehashed immediately before mutation (`src-tauri/src/transaction.rs:608-620`);
- post-copy destination existence is checked for non-delete actions (`src-tauri/src/transaction.rs:640-650`);
- rollback refuses to touch a non-delete destination whose current hash differs from a recorded after hash (`src-tauri/src/transaction.rs:846-852`);
- Git initialization is recorded before execution and has a rollback call (`src-tauri/src/transaction.rs:294-314,879-882`), although neither path is operation-checkpointed or fault-tested.

Those controls do not close the critical gaps described in TRX-002 through TRX-006.

## Maintenance behavior

| Operation | Contract | Bounded implementation | Result |
| --- | --- | --- | --- |
| Update | Base/local/incoming, exact target revision, conflict-signature reuse | No planner | Missing |
| Repair | Locked revision; restore missing/corrupt unmodified; review modified; validate broader evidence | Hash-only file planner; modified files skipped | Partial |
| Reinstall | Same/chosen revision through normal conflict engine | Blind replace with unknown local state; existing paths fail apply | Missing/unsafe |
| Managed removal | Reverse dependencies; delete owned unmodified; reverse merged contributions; preserve modified/merged/local additions | Deletes any unchanged locked file regardless of ownership | Unsafe |
| Credential reference removal | Remove project reference by default | No planner | Missing |
| OS credential removal | Separate explicit choice | No planner | Missing |
| Rollback | New transaction with preview and backup | Mutates from original journal; no new backup/preview/lock update | Unsafe |

## Fault scenario matrix

“Static hook” means a generic index hook exists; it is not evidence that the named concrete failure was injected or recovered.

| Failure scenario | Before stage | After stage | Before operation | After operation | Recovery / false-success evidence |
| --- | --- | --- | --- | --- | --- |
| Process termination | No kill test | No kill test | No kill test | No kill test | Unsafe remove/rename and rollback windows remain |
| Disk full | No | No | No | No | Journal/backup/stage/lock disk-full behavior untested |
| File/antivirus lock | No | No | No | No | Rename-after-remove can lose destination |
| Permission denial/loss | No | No | No | No | Backup/apply/rollback partial states untested |
| Network loss | Source/download are no-op | Source/download are no-op | Not applicable in runner | Not applicable | No evidence |
| Checksum mismatch | Generic stage hook only; real check is later | Generic stage hook only | No concrete injection | No concrete injection | Guard exists, no recovery test |
| Validation failure | Generic hook after validation work | Generic hook after validation work | No | No | Semantic validators not wired |
| Command/health timeout | No process execution | No | External op marked verified | After hook bypassed for external | Can create false component pass |
| Cancellation | No cancellation state/token | No | No | No | No evidence |
| Journal write failure | No injection | No injection | No intent-write injection | No applied/verified-write injection | Post-lock failure can leave false-success lock |
| User edit during dry run/staging | No stage test | No stage test | Hash guard only when local hash exists | No | Absent-path edit can be overwritten |
| Failure after destination removal | No boundary hook | No | No boundary hook | Hook occurs only after applied journal | Critical data-loss path |
| Failure after lock write | No boundary hook | No boundary hook | Not applicable | Not applicable | Lock remains if final journal/record write fails |
| Interrupted rollback | No rollback stage hooks | No | No rollback-op hooks | No rollback-op hooks | Rollback retry is not safe |
| Rollback after Git initialization | Generic stage hooks do not surround Git | No Git-stage after hook | No Git-operation hook | No Git-operation hook | Code exists, but no fault/recovery test and rollback is uncheckpointed |

Required generic boundary coverage is also not tested. Only `fail_after_operation = 0` and `fail_before_stage = 11` have test cases.

## Data-loss and false-success risks

1. Existing live file removed, rename fails, journal remains `staged`, rollback skips restoration.
2. User creates a file after an absent-path backup check; apply overwrites it with no backup.
3. Managed delete is rolled back after the user creates a replacement; rollback overwrites the replacement.
4. An unchanged merged file is removed wholesale by managed removal.
5. Rollback restores files but leaves the successful installation lock.
6. Lock write succeeds, then final journal or rollback-record persistence fails; transaction returns error with a surviving lock.
7. Readiness does nothing and component validation is set to pass unconditionally.
8. External/chmod work is marked verified without being performed.
9. Staged content changed after staging validation can be accepted because live hashes are compared only with themselves.
10. Duplicate operation IDs can overwrite backup files and update the wrong journal entry.
11. Lock optional component states can violate the lock schema.
12. A partial rollback cannot resume safely and can strand mixed original/installed content.

## Exact test evidence and meaningful omissions

Existing inline transaction tests:

- `apply_then_rollback_restores_original_hashes`, `src-tauri/src/transaction.rs:1076-1118`: one replace happy path; stale lock is not rejected.
- `injected_failure_does_not_write_success_lock`, `src-tauri/src/transaction.rs:1119-1153`: one create operation, failure after the applied journal checkpoint.
- `failure_before_final_rollback_checkpoint_does_not_write_lock`, `src-tauri/src/transaction.rs:1155-1182`: one stage boundary only.
- `repair_and_removal_preserve_modified_files`, `src-tauri/src/transaction.rs:1183-1222`: one modified managed file only.

Existing merge tests at `src-tauri/src/merge.rs:331-371` cover binary option exclusion, a simple line merge, and disjoint TOML keys. Migration tests at `src-tauri/src/migrations.rs:61-72` cover generic state version handling only.

No separately referenced transaction integration or fault test file was found in the bounded source set.

Meaningful skipped or absent scenarios:

- all 24 before/after stage cases;
- every operation index and skip/external branches;
- process kill at temp copy, fsync, remove, rename, hash, journal, rollback record, and lock;
- disk full, file lock, permission denial, network loss, timeout, cancellation, and journal-write failure;
- staged-file tampering and user edits after plan/backup;
- destination absent precondition race;
- delete destination absence verification;
- partial backup and partial rollback;
- resume from each observable filesystem state;
- rollback protection for later work after delete/create/replace/merge;
- duplicate operation IDs and duplicate merge destinations;
- update, reinstall conflict, merged removal, credential reference removal, and explicit OS credential removal;
- rollback lock/state transition;
- JSON array reordering/identity, AGENTS adaptation, merge-result validation wiring;
- schema validation of runtime-generated locks;
- Windows/macOS atomic replacement and cloud-synchronization behavior;
- property test for apply→rollback hashes, idempotency, and removal ownership;
- Git initialization rollback fault/recovery coverage and external health failure.

## Recommended parent actions

### P0 — block release and redesign transaction commit/recovery

1. Introduce a versioned per-operation record containing unique stable ID, action, source/destination identity, expected before state (including explicit absence), expected after state/hash, ownership, conflict signature/choice, backup hash/metadata/reference, intent/applied/verified checkpoints, observed state, and rollback instruction.
2. Persist and fsync operation intent before each live mutation. Replace files with a platform adapter that never removes the destination before an atomic replacement. Persist observed result immediately after mutation and verify it against the expected hash.
3. Implement resume as a filesystem reconciliation state machine. Classify each operation from expected-before, expected-after, backup, staging, and live evidence; block unknown states.
4. Make rollback a new staged transaction with its own backup and per-operation checkpoints. Verify absence after deletes, protect later user work, restore/verify original hashes and metadata, and update/remove the success lock.
5. Make lock write the final required fallible completion artifact after durable readiness and rollback records. Do not perform required journal/record writes after the lock, or define a recoverable terminal commit protocol that cannot expose an unverified lock.

### P0 — implement real stages and final verification

6. Put the actual source resolution, selective download, checksum, review evidence, backup, staging, semantic validation, apply, post-check, readiness, and rollback-record work inside correctly ordered stage boundaries.
7. Recheck staged hashes at the live boundary; compare live destinations with expected hashes/absence; run parsers, schemas, merge validators, health/Git checks, and readiness core gate before lock construction.
8. Reject duplicate operation IDs/destinations and bind every conflict selection to exactly one compatible operation.

### P1 — align schemas, examples, locks, and maintenance

9. Update schemas together so the exact twelve stages/order and the full operation model are representable and required. Add conditional requirements by action and ownership.
10. Fix component-state mapping, explicit operation ownership, merge result/signature persistence, rollback-record integrity/reference, and runtime schema validation before writing a lock.
11. Implement update, locked-revision repair, conflict-aware reinstall, ownership-aware reverse-dependency removal, project credential-reference removal, and a separate explicit OS credential deletion plan.
12. Route journal reads through validated migrations and add lock/journal migration tests.

### P1 — connect the conflict engine

13. Persist base/local/incoming signatures; allow bulk/reused decisions only for identical signatures.
14. Require manifest-declared JSON array identity or manual review, add AGENTS adaptation validation, and call merged-result validation from staged validation.

### P2 — build the required fault suite

15. Add deterministic adapters/injection points before and after every stage and every irreversible sub-boundary: intent journal, backup, stage write, validate, temp copy, fsync, replace, live hash, applied journal, verified journal, rollback operation, readiness record, rollback record, lock, and terminal journal state.
16. Exercise process kill, disk full, file lock, permission denial, network loss, checksum mismatch, validation failure, timeout, cancellation, journal failure, user races, and Git rollback on Windows and macOS. Assert no false-success lock and exact apply/rollback hashes.
17. Add property tests for unique IDs, idempotent verified operations, filesystem-evidence resume, apply→rollback restoration, and “removal never deletes unowned or merged user content.”

## Parent completion decision

Do not accept transaction, recovery, update/repair/reinstall/removal, or related acceptance criteria as complete. The current happy-path tests do not mitigate the critical interruption, user-work, rollback, and false-success-lock risks.
