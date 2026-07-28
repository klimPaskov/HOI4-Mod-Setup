# Current-worktree transaction and recovery audit

Audit date: 2026-07-28
Snapshot: `ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70` on `codex/bootstrap-hoi4-mod-setup`, with pre-existing uncommitted changes
Scope: the requested transaction, recovery, rollback, maintenance, flatten integration, schemas, examples, and inline `transaction.rs` / `commands.rs` tests. No product code was changed.

## Verdict

**FAIL.**

The current snapshot has the exact twelve-stage list, core-owned reviewed plans, pre-apply backups, staged validation, live precondition checks, destination hash checks, late success-lock timing, root-bound recovery, rollback child journals, inverse backups, and later-user-work refusal.

The acceptance gate still fails for four high-impact reasons:

1. maintenance flattening includes unresolved incoming files;
2. provider-adapted `core.agents` bytes do not have a correct maintenance source/result/base model;
3. finalization recovery does not cryptographically bind or verify the success lock and rollback-record artifact;
4. managed removal writes a lock shape that conflicts with the lock schema and preserves the project credential reference it is supposed to clear.

Rollback file restoration also records expected hashes without re-reading the restored destinations, and the checked-in fault matrix does not exercise the required real failure classes.

## Findings

### TX-01 — High — Maintenance flattening uses unresolved incoming files

**Fail. This is the specifically requested maintenance-flatten check.**

`flatten_operation_uses_incoming` excludes only:

- a `skip` whose resolution is literally `keep` or `skip`; or
- a modified operation whose resolution is `None`.

Evidence: `src-tauri/src/commands.rs:2811-2815`.

Maintenance planners assign unresolved modified operations sentinel resolutions rather than `None`:

- repair: `review_required` or `reverse_merge_required` at `src-tauri/src/transaction.rs:3335-3369`;
- reinstall: `review_required` or `reverse_merge_required` at `src-tauri/src/transaction.rs:3456-3489`;
- update: `review_required` at `src-tauri/src/transaction.rs:3521-3547`.

The maintenance builder fetches and stores those incoming bytes in `prepared` before any conflict is selected (`src-tauri/src/commands.rs:3442-3590`), then passes them through `accepted_flatten_prepared_files` into the flattened artifact builder (`src-tauri/src/commands.rs:3593-3613`). The corresponding `PlanConflict.selected` remains `None` (`src-tauri/src/commands.rs:3674-3705`).

Consequences:

- the initial maintenance flat preview is derived from bytes the user has not accepted;
- unresolved incoming content can trigger flatten collision, secret, or size rejection before the user can choose keep;
- the flat conflict set is not provenance-correct at review time;
- apply is eventually blocked by unresolved conflicts, so this is not by itself a silent live overwrite, but it violates the reviewed-input contract and can block safe maintenance.

The post-choice refresh is sound in principle: resolving a non-flat conflict rebuilds flat output (`src-tauri/src/commands.rs:3917-3919`), and keep/skip removes the source prepared bytes (`src-tauri/src/commands.rs:3828-3837`). The only flatten regression test starts with every non-flat conflict already resolved to `keep`; it does not exercise unresolved maintenance input (`src-tauri/src/commands.rs:4523-4800`).

### TX-02 — High — Provider-adapted `core.agents` maintenance hashes and merge base are inconsistent

**Fail.**

Initial planning correctly keeps the downloaded source hash separate from adapted result bytes: it verifies the source, adapts `core.agents`, and records the adapted result hash (`src-tauri/src/commands.rs:2354-2379`, `src-tauri/src/commands.rs:2438-2449`).

Maintenance re-fetches the locked source and adapts `core.agents` again (`src-tauri/src/commands.rs:3449-3508`), but then rejects any adapted hash that differs from the source hash unless the component is `codex.config` (`src-tauri/src/commands.rs:3509-3531`). Therefore update, repair, and reinstall fail whenever AGENTS adaptation changes the verified source bytes, including while preparing a modified-file conflict.

The maintenance merge base has the same asymmetry. The old base is adapted only for `codex.config`; `core.agents` uses the raw repository source as the base (`src-tauri/src/commands.rs:3553-3566`), while the incoming side uses adapted bytes (`src-tauri/src/commands.rs:3491-3508`, `src-tauri/src/commands.rs:3579-3586`). A three-way AGENTS merge can therefore compare:

- raw old template;
- locally installed/adapted content plus user edits;
- newly adapted incoming content.

That is not the verified installed base required by the merge contract. No scoped test covers update, repair, reinstall, or merge of provider-adapted `core.agents`.

### TX-03 — High — Finalizing recovery does not verify the exact lock or rollback-record artifact

**Fail for interruption false-success prevention.**

The nominal timing is good: readiness is persisted and blocking checks are rejected before lock construction (`src-tauri/src/transaction.rs:720-758`); the journal enters `finalizing`, the rollback record is written and stage 12 completes before the success lock is written (`src-tauri/src/transaction.rs:759-794`).

However, the installation journal initializes `result_lock_sha256` and `result_lock_exists` to `None` and never fills them for a successful installation (`src-tauri/src/transaction.rs:313-365`, `src-tauri/src/transaction.rs:788-804`). `finish_finalization`:

- reads and migrates whatever lock is currently present;
- checks only that its `rollback_records` list contains a path suffix for the transaction;
- does not compare the lock to a precommitted expected hash;
- does not open, hash, or validate the referenced `rollback-record.json`.

Evidence: `src-tauri/src/transaction.rs:2955-3000`.

A replaced or corrupted but parseable lock containing the expected rollback-record path can therefore turn a stale `finalizing` journal into `completed`. The cross-process test proves reconciliation after the real lock write, but does not test lock substitution, missing/corrupt rollback record, or expected-lock hash mismatch (`src-tauri/src/transaction.rs:3808-3894`).

### TX-04 — High — Managed removal preserves the project credential reference and emits a schema-invalid lock

**Fail.**

Removal intentionally sets `codex_analysis` to `None` (`src-tauri/src/commands.rs:3232-3242`) and the resulting plan carries no credential references (`src-tauri/src/commands.rs:3727-3749`). `build_lock` copies the plan analysis directly (`src-tauri/src/transaction.rs:2120-2147`), so the serialized removal lock contains `"codex_analysis": null`.

The current installation-lock schema requires `codex_analysis` and permits only an object, not `null` (`schemas/installation-lock.schema.json:7-23`, `schemas/installation-lock.schema.json:271-283`). The transaction does not schema-validate the lock before treating it as the success artifact.

The removal plan also leaves the Meshy vault reference in project state. `build_lock` starts with the prior optional-workflow map and falls back to the predecessor lock's `credential_reference` when the new plan supplies none (`src-tauri/src/transaction.rs:2049-2075`). That conflicts with the accepted removal behavior: clear project credential references, while making OS-vault deletion a separate explicit action (`docs/15_update_repair.md:29-31`).

The explicit vault actions themselves are separate commands, as required:

- hosted-provider credential removal: `src-tauri/src/commands.rs:542-557`;
- Meshy credential removal: `src-tauri/src/commands.rs:1046-1062`.

Managed removal does not call them, so implicit OS-vault deletion was not found.

### TX-05 — Medium — Rollback marks restored files complete without verifying the live restored hash

**Fail for rollback destination verification.**

Parent rollback verifies the backup hash, calls `copy_atomic`, and immediately records the operation as `rolled_back` (`src-tauri/src/transaction.rs:2726-2783`). It does not re-hash the destination after the copy. `persist_rollback_checkpoint` writes the expected hash into `after_sha256`; it does not observe the filesystem (`src-tauri/src/transaction.rs:2548-2567`).

This affects normal rollback and inverse rollback. The predecessor lock path is stronger because it is re-hashed after restoration (`src-tauri/src/transaction.rs:2902-2928`), but managed files do not receive that final verification.

The happy-path test compares one restored file's bytes and verifies child-journal structure (`src-tauri/src/transaction.rs:4056-4135`). There is no corrupted-copy, disk-full-during-restore, multi-file partial rollback, or post-copy hash-mismatch test.

### TX-06 — Medium — The twelve stages are exact but stages 1–5 are not a fully journaled execution

**Pass for names/order; fail for the full staged-transaction interpretation.**

The canonical constant has exactly 12 stages in the required order (`src-tauri/src/models.rs:9-22`), `validate_plan` requires exact equality (`src-tauri/src/transaction.rs:23-33`), and the runner invokes each stage in order (`src-tauri/src/transaction.rs:528-787`).

Actual source resolution, tree fetch, selective download, and checksum verification happen during plan building before the transaction journal exists (`src-tauri/src/commands.rs:2307-2371`). In `run_transaction`, stages 2–5 are only `stage_start` / `stage_complete` transitions (`src-tauri/src/transaction.rs:542-597`). Preflight validation and transaction-directory/plan writes also occur before the preflight stage start (`src-tauri/src/transaction.rs:493-534`).

This means:

- a crash or network failure during the real source/download/checksum work has no transaction journal;
- the stage fault matrix does not exercise those real boundaries;
- the journal's source stages have no generated evidence entries;
- transaction directories and `plan.json` can exist without the initial journal if interruption occurs before `persist_journal`.

The design document currently acknowledges that source stages are completed by the plan builder, but the result is not the requested fully journaled twelve-stage transaction.

### TX-07 — Medium — Journal operation evidence omits conflict choice and does not observe skipped operations

**Partial.**

New plans enforce non-empty unique operation IDs, ownership, hash syntax, destination uniqueness, and resolved conflict binding (`src-tauri/src/transaction.rs:152-298`). `new_journal` records action, destination, ownership, component, location, precondition hash, expected/source/result hashes, and rollback instruction (`src-tauri/src/transaction.rs:307-380`).

Gaps:

- `JournalOperation` has no resolution/conflict-choice field (`src-tauri/src/models.rs:807-850`), and the journal schema has none (`schemas/transaction-journal.schema.json:146-277`);
- the plan hash indirectly binds the separate plan, but the journal operation itself does not satisfy the required conflict-choice evidence;
- skip/external operations are marked `verified` without recording an observed hash, existence, or operation-specific `last_checkpoint` (`src-tauri/src/transaction.rs:1190-1209`);
- `fail_after_operation` is bypassed for skip/external operations because the loop continues first;
- the journal example omits `expected_sha256` even though current code populates it (`examples/transaction-journal.example.json:81-118`).

The journal schema also intentionally permits legacy null ownership/action/rollback and does not require `project_root`; recovery correctly refuses unbound journals, but the schema alone cannot distinguish a current resumable journal from audit-only legacy data (`schemas/transaction-journal.schema.json:7-16`, `schemas/transaction-journal.schema.json:49-50`, `schemas/transaction-journal.schema.json:150-253`).

### TX-08 — Medium — A user edit after post-checks can be followed by a stale success lock

**Fail for the final live precondition window.**

Live content is verified during post-install checks (`src-tauri/src/transaction.rs:1420-1467`). Readiness, optional health work, lock construction, rollback-record persistence, and stage-12 completion then occur before the lock write (`src-tauri/src/transaction.rs:720-793`).

There is no final recheck of every live destination immediately before writing the lock. For normal mutating operations, `build_lock` records prepared hashes rather than re-reading the live files (`src-tauri/src/transaction.rs:1938-1983`). A concurrent edit in this window can therefore produce a success lock whose `installed_sha256` does not match the filesystem. Later rollback is likely to refuse the changed file, which protects user work, but the success artifact is already false.

No scoped test edits a destination between post-install verification and lock commit.

### TX-09 — Medium — Schemas and examples do not encode the external-destination invariant

**Fail for example/implementation agreement.**

The installation-plan example labels the launcher operation `external_launcher` but omits `external: true` and uses a `%USERPROFILE%` placeholder rather than an explicit absolute destination (`examples/installation-plan.example.json:62-75`). Current `validate_plan` rejects a non-external operation with `external_launcher` scope (`src-tauri/src/transaction.rs:184-193`).

The installation-lock example has the same placeholder and also omits `external: true` (`examples/installation-lock.example.json:75-87`). Both schemas make `external` optional and do not encode the scope/absolute-path relationship.

The journal schema also declares `git_initialized` twice (`schemas/transaction-journal.schema.json:89-91`, `schemas/transaction-journal.schema.json:316-318`). The definitions agree, but duplicate JSON object keys are ambiguous input for schema tooling.

### TX-10 — Medium — Fault injection does not cover the required failure classes or every real boundary

**Fail.**

The runner exposes only four logical error injectors: before/after stage and before/after apply operation (`src-tauri/src/transaction.rs:14-21`). The stage test iterates 12 × 2 logical injection points and one create operation × 2, asserting only that the success lock is absent (`src-tauri/src/transaction.rs:4900-4950`).

Important limitations:

- `fail_after_stage` triggers before the stage is marked complete and persisted, so it is not a failure after the durable after-boundary (`src-tauri/src/transaction.rs:915-931`);
- source/download/checksum stages are marker-only in the runner;
- only one create operation is used; replace, generate, merge, rename, delete, external, and skip boundaries are not iterated;
- no fault is injected into backup-copy persistence, staging writes, validation, readiness/health execution, rollback file apply, predecessor-lock restoration, or journal replacement;
- process abort coverage exists only after rollback-record write, after lock write, after rollback backup, and after inverse-rollback backup (`src-tauri/src/transaction.rs:2570-2582`, `src-tauri/src/transaction.rs:3808-3957`);
- disk full, permission denial, file lock, network loss, checksum mismatch, validation failure, command timeout, cancellation, and journal-write failure have no scoped transaction fault fixtures.

The no-false-lock assertions are useful but do not establish required recovery outcome or final hashes at every injected point.

## Stage coverage

| Stage | Implementation | Audit result |
| --- | --- | --- |
| 1. Preflight | Plan/root validation and storage checks occur; journal stage is recorded afterward | **Partial** — not all preflight work is inside the durable stage |
| 2. Repository source resolution | Real work in plan builder; runner records markers | **Partial** |
| 3. Selective download | Real work in plan builder; runner records markers | **Partial** |
| 4. Checksum verification | Download verification before journal; staged hashes checked later | **Partial** |
| 5. Dry-run review | Core plan fingerprint and approval are enforced | **Pass** |
| 6. Backup | Existing mutation targets and predecessor lock are copied and hashed | **Pass**, with predecessor-lock capture occurring before stage 1 |
| 7. Staging | Prepared bytes written outside live destinations with per-file journal updates | **Pass** |
| 8. Validation | Staged hashes and supported file validators run | **Pass**, but no current lock-schema validation |
| 9. Apply | Intent persisted before mutation; live precondition checked; atomic copy/delete; observed result recorded | **Pass for normal mutating operations** |
| 10. Post-install checks | Live hash and format checks | **Pass** |
| 11. Readiness report | Report persisted; blocking checks prevent stage 12 and lock | **Pass** |
| 12. Rollback record | Finalizing journal and rollback record precede lock | **Partial** — finalization recovery lacks exact artifact binding |

Apply iterates `plan.operations` directly (`src-tauri/src/transaction.rs:1190-1317`). The required directory/create/managed/merge/descriptor ordering is not independently sorted or validated by the transaction core; it depends on planner insertion order.

## Journal state findings

### Confirmed controls

- Journal persistence is centralized through `persist_journal` and atomic JSON replacement (`src-tauri/src/transaction.rs:828-831`).
- The supporting atomic writer writes a temporary file, calls `sync_all`, replaces the destination, and attempts a parent-directory sync (`src-tauri/src/security.rs:310-390`).
- Stage start and normal completion persist the journal (`src-tauri/src/transaction.rs:879-931`).
- Apply writes durable `applying` intent before live replacement/delete and persists `applied` and `verified` afterward (`src-tauri/src/transaction.rs:1263-1316`).
- Journals bind canonical project root and transaction UUID; rollback/discard validate both root and journal path (`src-tauri/src/transaction.rs:307-420`, `src-tauri/src/transaction.rs:2584-2608`, `src-tauri/src/transaction.rs:3229-3289`).
- `read_journal` uses `migrations::migrate_journal` (`src-tauri/src/transaction.rs:2948-2953`).

### Residual journal gaps

- Parent-directory sync errors are ignored, and there is no journal-write/power-loss fault test.
- Conflict resolution is absent from journal operations.
- Skip/external operations lack observed-result evidence and an operation checkpoint.
- Installation finalization does not precommit the exact expected lock/rollback-record hashes.
- Stage source evidence arrays remain empty in the real runner.

## Apply and rollback findings

### Apply controls that pass

- Backup happens before live operation apply, with live precondition and backup-hash verification (`src-tauri/src/transaction.rs:934-1014`).
- Staged bytes are checked against both prepared and operation result/source hashes (`src-tauri/src/transaction.rs:1016-1131`).
- Live preconditions are rechecked immediately before each mutating operation (`src-tauri/src/transaction.rs:1211-1235`).
- Staged bytes are rechecked immediately before apply (`src-tauri/src/transaction.rs:1236-1262`).
- Atomic replacement is used for file writes, including Windows `ReplaceFileW` (`src-tauri/src/transaction.rs:1321-1417`).
- Post-install checks compare the live hash with the journal observation and expected result/source hash (`src-tauri/src/transaction.rs:1420-1467`).
- Blocking readiness prevents the success lock (`src-tauri/src/transaction.rs:740-751`; regression at `src-tauri/src/transaction.rs:4521-4547`).
- Core-owned prepared plan fingerprint/root/approval checks prevent renderer-edited apply (`src-tauri/src/commands.rs:389-419`, `src-tauri/src/commands.rs:3923-3961`).
- Existing generated-file replacement now uses `restore_backup`, and `validate_plan` rejects inconsistent generated rollback metadata (`src-tauri/src/commands.rs:2699-2708`, `src-tauri/src/transaction.rs:237-259`).

### Rollback controls that pass

- Parent rollback creates/reuses a separate child journal and backup set (`src-tauri/src/transaction.rs:2266-2545`).
- Rollback writes `rolling_back` and per-operation `rollback_applying` intent (`src-tauri/src/transaction.rs:2584-2701`).
- Later user work is compared with recorded post-apply hashes before restoration/deletion (`src-tauri/src/transaction.rs:2702-2724`).
- Explicit skip, external, rollback-none, and legacy missing metadata are non-destructive no-ops (`src-tauri/src/transaction.rs:2635-2665`).
- Predecessor lock restoration checks backup and restored hashes (`src-tauri/src/transaction.rs:2855-2945`).
- Completed rollback children retain inverse backups and lock-state evidence (`src-tauri/src/transaction.rs:2806-2839`).
- Inverse rollback checks the recorded lock before file apply and refuses later file/lock edits (`src-tauri/src/transaction.rs:2375-2483`; tests at `src-tauri/src/transaction.rs:4138-4220`).
- Transaction-created Git is reversed before file restoration; preserve-mode remote removal is URL-guarded (`src-tauri/src/transaction.rs:2614-2634`).

### Rollback gaps

- Restored managed files are not re-hashed before `rolled_back` is persisted (TX-05).
- Inverse rollback deliberately does not recreate Git or external side effects; this is honest but means inverse is a managed-file/lock restoration only.
- No checked test combines generated replacement, normal rollback, inverse rollback, and interruption after each file boundary.
- Reverse-merge removal is not implemented: merged files are always skipped by managed removal (`src-tauri/src/transaction.rs:3398-3429`), and removal conflicts offer only keep/skip (`src-tauri/src/commands.rs:3674-3697`).

## Maintenance behavior

| Mode | Confirmed behavior | Result |
| --- | --- | --- |
| Update | Preserves latest/pinned mode, resolves an exact source, requires fresh root/scan/evidence-bound reanalysis, and sends modified files to conflict review | **Partial** — unresolved bytes enter flattening; adapted AGENTS hash/base is wrong |
| Repair | Uses locked revision; healthy managed files are no-op; missing managed files recreate; modified/merged files are preserved for review | **Partial** — adapted AGENTS preparation can fail; merged repair is preserve-only |
| Reinstall | Re-fetches locked revision; modified and external files are skipped for review | **Partial** — all merged files are skipped, even unmodified; adapted AGENTS preparation can fail |
| Managed removal | Deletes only unchanged non-merged/non-external lock files; preserves modified/merged/external files | **Partial** — no reverse merge, project credential reference retained, removal lock schema mismatch |
| Credential removal | Separate explicit OS-vault commands; not invoked by managed removal | **Pass for separation**, no scoped end-to-end transaction test |

An already missing managed file is classified as modified during removal (`src-tauri/src/transaction.rs:3391-3429`). Because a skipped predecessor file remains in `build_lock`, the removal lock can retain a file entry for an absent destination and record an inaccurate local modification. A removal containing no `delete_managed` operation also misses the special non-ready removal report path (`src-tauri/src/transaction.rs:1475-1546`).

## Fault scenario matrix

| Scenario | Injection/test evidence | Result |
| --- | --- | --- |
| Before each of 12 stage markers | Logical injector and loop test | **Pass as marker coverage** |
| After each of 12 durable stage boundaries | Injector fires before completion persistence | **Fail** |
| Before/after every apply operation type | One create operation only | **Fail** |
| Process termination | Four finalization/rollback-backup checkpoints | **Partial** |
| Disk full | No scoped adapter/injector | **Missing** |
| File lock / antivirus lock | No scoped adapter/injector | **Missing** |
| Permission denial/loss | No scoped adapter/injector | **Missing** |
| Network loss | Real network work occurs pre-journal; no transaction fault | **Missing** |
| Checksum mismatch | Rejection code exists; no scoped boundary fault regression | **Partial** |
| Staged validation failure | Rejection code exists; no scoped fault regression | **Partial** |
| Health/command timeout | No transaction fault fixture | **Missing** |
| Cancellation | No transaction cancellation checkpoint | **Missing** |
| Journal write failure | No injector/test | **Missing** |
| Blocked readiness | Test proves no lock | **Pass** |
| Apply interruption then resume | Resume correctly refused after apply | **Pass** |
| Pre-apply interruption then resume | Plan/live/staging evidence checked | **Pass** |
| Later file or lock edit before inverse | Refused | **Pass** |
| Rollback final hashes at every point | One happy-path file only | **Fail** |

## Data-loss and false-success risks

1. **False reviewed input:** unresolved maintenance incoming files are exported into flat artifacts before acceptance (TX-01).
2. **Maintenance failure or unsafe merge basis:** adapted AGENTS bytes are rejected as checksum mismatch, and a modified AGENTS merge uses a raw source base (TX-02).
3. **False finalization:** a parseable substituted lock with the expected rollback path can complete a stale finalizing journal; the rollback record itself is not verified (TX-03).
4. **False removal artifact:** removal can write a lock that violates the current lock schema and still retains a project credential reference (TX-04).
5. **False rollback completion:** rollback journals record expected restored hashes without observing the restored destinations (TX-05).
6. **Stale success lock:** a user edit after post-checks but before lock write is not caught (TX-08).
7. **Schema/example drift:** external launcher examples do not carry the required external/absolute identity and are rejected or misinterpreted by current code (TX-09).

No path was found that silently overwrites a modified file through the normal approved planner: modified files default to skip/conflict, backup and live precondition checks precede mutation, and rollback refuses later edits. The principal current risks are false provenance, false completion evidence, incomplete maintenance, and an incorrect merge base rather than an unreviewed direct overwrite.

## Exact test evidence reviewed

Confirmed checked-in tests:

- cross-process finalization and rollback backup recovery: `src-tauri/src/transaction.rs:3808-3957`;
- removal non-ready report and signed-out removal-plan validation: `src-tauri/src/transaction.rs:3960-3988`;
- repair healthy-file no-op and first-install preserved modification: `src-tauri/src/transaction.rs:3990-4054`;
- apply, rollback child, and completed inverse path: `src-tauri/src/transaction.rs:4056-4135`;
- inverse refusal after later file/lock edit: `src-tauri/src/transaction.rs:4138-4220`;
- skipped-file rollback no-op and rollback checkpoint retry: `src-tauri/src/transaction.rs:4222-4312`;
- immutable merged ownership and remote approval: `src-tauri/src/transaction.rs:4314-4408`;
- project-root binding, predecessor-lock restoration, blocked readiness, resume, discard, and pre-lock failure: `src-tauri/src/transaction.rs:4429-4781`;
- modified repair/removal/reinstall preservation and merged removal review: `src-tauri/src/transaction.rs:4783-4898`;
- logical stage/operation no-success-lock matrix: `src-tauri/src/transaction.rs:4900-4950`;
- maintenance update reanalysis binding: `src-tauri/src/commands.rs:4358-4457`;
- reviewed flat-conflict preservation after accepted source changes: `src-tauri/src/commands.rs:4522-4800`.

## Skipped meaningful scenarios

The following are absent from the scoped tests or were not executable under this read-only audit:

- unresolved modified maintenance source excluded from flat artifacts before choice;
- keep, replace, merge, rename, and skip rebuilding maintenance flat output for update, repair, and reinstall;
- provider-adapted AGENTS update/repair/reinstall and verified adapted merge base;
- generated/flat existing-file replace → rollback → inverse rollback, with interruption at each boundary;
- finalization with substituted lock, missing/corrupt rollback record, or mismatched precommitted lock hash;
- user edit between post-check and lock commit;
- removal lock JSON Schema validation and clearing only the project credential reference;
- all-files-already-missing removal;
- post-copy rollback hash mismatch;
- each operation action under before/after logical and process-kill faults;
- disk full, file lock, permission denial, network loss, timeout, cancellation, and journal write failure;
- Windows/macOS native end-to-end recovery and cloud-synced-folder stabilization.

No tests were run during this audit because the instruction permits writing only this report; invoking Cargo would create or update build artifacts. Test conclusions above are static review of the checked-in test code, not a current execution claim.

## Recommended parent actions

1. Make flatten acceptance depend on an actual selected conflict decision, not sentinel `resolution` text. Exclude every modified operation with `PlanConflict.selected == None`; rebuild only from keep/local or replace/merge/rename accepted bytes.
2. Give `core.agents` the same explicit source-hash/result-hash treatment as every deterministic transformation. Recreate the old adapted result for the merge base and test all maintenance modes.
3. Before entering `finalizing`, serialize the exact lock and rollback record, persist their expected hashes in the journal, and make finalization recovery verify both artifacts byte-for-byte.
4. Re-hash every restored destination before persisting `rolled_back`; store observed values rather than copying expected values into the child journal.
5. Clear project credential references on managed removal without deleting the OS-vault item. Decide whether a removal lock retains the last validated analysis or the schema permits `null`, then schema-validate the emitted lock.
6. Recheck every live operation result immediately before committing the success lock.
7. Move real source/download/checksum work under durable stage orchestration, or explicitly revise the transaction contract and fault model. Do not count marker-only stages as network/checksum fault coverage.
8. Add filesystem/network/journal fault adapters and iterate every operation action and rollback operation before and after durable boundaries, asserting recovery state and final hashes in addition to lock absence.
9. Add conflict choice and observed no-op evidence to journal operations; fix the journal schema duplicate key and current examples' external absolute-path metadata.
10. Enforce or validate deterministic apply ordering in the transaction core and add an explicit verified-operation idempotency test.
