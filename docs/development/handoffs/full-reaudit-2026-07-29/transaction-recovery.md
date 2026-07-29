# Transaction and recovery audit — HEAD `bcfe329`

Date: 2026-07-29
Scope: bounded read-only audit of the transaction, recovery, rollback, maintenance, merge, migration, schema/example, command, and inline Rust-test files named in the parent prompt.
Result: **not ready for a completion claim**. The normal successful file path has strong hash, backup, final-readiness, lock, and rollback protections, but real process-kill discovery, transaction exclusivity, one delete interruption boundary, flattened keep/skip conflicts, merged-update bases, and non-file operation journaling remain incomplete.

No application code, schema, test, or general documentation was changed. This report is the only written artifact. No test command was run because the audit was explicitly read-only and a Cargo test run would write build artifacts; all test conclusions below are based on checked-in test inspection.

## Executive findings

| ID | Severity | Finding | Primary risk |
| --- | --- | --- | --- |
| TR-01 | Critical | Real process kills during ordinary stage/apply states are not discovered, and the core runner does not refuse a second transaction for the same project. | A partial apply can be hidden and then overlapped by a new mutation, invalidating backups and creating data-loss risk. |
| TR-02 | High | A crash after an existing `delete_managed` target is removed but before its observed result is journaled cannot resume and is rejected by rollback. | The transaction reaches a manual-recovery dead end at an explicitly required operation boundary. |
| TR-03 | High | A transient journal failure immediately after the success-lock write converts the in-memory `finalizing` state to `interrupted`; the durable lock can say success while resume refuses finalization. | Split-brain/false-success state between the project lock, command result, and recovery UI. |
| TR-04 | High | A reviewed flattened Chat-source conflict resolved with `keep` or `skip` is removed from prepared bytes but remains a generated artifact; mutation-boundary flatten validation compares that against a rebuilt artifact and rejects apply. | Valid keep/skip outcomes for existing `chatgpt_project_sources/` files cannot complete. |
| TR-05 | High | Update merge contexts reconstruct the base from the old repository source, including for `merged` ownership, instead of using verified prior installed-result bytes. | An accepted three-way merge can be based on the wrong ancestor and lose or mis-handle prior merged contributions. |
| TR-06 | High | Git and MCP operations are outside the per-operation journal model; preserve-mode remote identity is recorded only after Git returns. | A crash after adding a remote can leave an unrecorded side effect that rollback cannot reverse; operation-boundary fault coverage is absent. |
| TR-07 | Medium | Rollback child journals are created with all 12 stages, but only backup is marked complete before the child itself is marked `completed`. | The rollback audit record claims terminal completion with a false stage ledger. |
| TR-08 | Medium | Journals advertise resume from the beginning, but resume requires staged files for every mutating operation. | Failures before staging are offered as resumable even though resume deterministically fails. |
| TR-09 | Medium | Journal/schema evidence does not encode expected pre-mutation existence, does not carry external actions, and schemas do not enforce exact stage order/count. | Ambiguous recovery evidence and weaker schema-level invariants than the product contract. |
| TR-10 | Medium | Structured JSON merges positional arrays without a manifest identity key; TOML/JSON upstream key deletions are silently retained. | Incorrect structured results, especially for reordered arrays or removed managed keys. |
| TR-11 | Medium | Apply uses reviewed operation order without enforcing the documented category ordering, and file metadata is not captured/restored by the audited operation records. | Determinism and metadata rollback are not demonstrated for mixed operation plans. |

## Stage coverage

The exact ordered list is enforced at runtime by `validate_plan` in `src-tauri/src/transaction.rs:23-33`; `run_transaction` executes indices 0 through 11 in order at `src-tauri/src/transaction.rs:543-831`. The plan schema is weaker: `docs/schemas/installation-plan.schema.json:259-276` requires only an array with `minItems: 12`, not the exact 12 constants or exact order.

| # | Required stage | Current evidence | Verdict |
| --- | --- | --- | --- |
| 1 | preflight | `validate_plan`, canonical root, flatten revalidation, application-root/link checks, transaction roots, plan, journal, and predecessor-lock capture occur at `transaction.rs:507-542`; stage checkpoint at `543-556`. | **Partial.** There is no core check for another incomplete journal. The predecessor lock is copied before the backup stage and even before preflight is marked active. |
| 2 | repository source resolution | Exact repository/revision/manifest evidence is added at `transaction.rs:557-581`. Plan builders resolve exact revisions in `commands.rs:2424-2466` and update resolution in `commands.rs:3511-3574`. | **Pass with synthetic replay.** The journaled transaction carries prior core plan-builder evidence rather than performing network resolution here, matching `docs/14_transaction_rollback.md:82-94`. |
| 3 | selective download | Prepared payload bindings are checked at `transaction.rs:582-597` via `validate_prepared_files` (`1003-1140`). | **Partial.** It proves selected prepared bytes and operation binding, but does not itself download or prove network-resume behavior. |
| 4 | checksum verification | The same payload is revalidated at `transaction.rs:598-613`; generated artifacts are checked in `validate_plan` at `142-159`. | **Pass for prepared bytes.** Separate source/result hashes are preserved. |
| 5 | dry-run review | Approval validation is in `transaction.rs:113-132`; renderer submission is compared to the core fingerprint at `commands.rs:4296-4321`; stage ledger at `transaction.rs:614-627`. | **Pass.** The submitted plan is not authoritative. |
| 6 | backup | Live targets are hash-rechecked, copied, verified, and journaled at `transaction.rs:1246-1326`; predecessor lock at `470-499`. | **Partial.** Backup-before-live-mutation passes. Metadata capture/restoration is absent, and predecessor-lock backup is recorded before stage 6. |
| 7 | staging | Staged destinations are outside live paths and are link-checked, written, hashed, and journaled per file at `transaction.rs:1328-1397`. | **Pass.** Staging itself is safely repeatable. |
| 8 | validation | Staged hashes and parsers for descriptors, PNG, TOML, JSON, AGENTS, and wiki text run at `transaction.rs:1399-1490`; flatten inputs are rebuilt at `1169-1243`. | **Partial.** Parsing is present, but JSON schema/component-specific validation and the valid flat keep/skip case are not complete. |
| 9 | apply | Apply intent is durable before mutation, preconditions and staged hashes are rechecked, atomic copy/replace is used, and observed hashes/existence are recorded at `transaction.rs:1492-1635`. | **Partial.** The delete crash gap TR-02 remains; the documented category order is not enforced; skip/external operations bypass the after-operation fault hook. |
| 10 | post-install checks | Live hashes and parsers are checked at `transaction.rs:1737-1785`. | **Partial.** Git is executed only after this stage is marked complete (`723-749`), so “post-install checks” does not actually include the Git action it is expected to verify. |
| 11 | readiness report | Readiness is built and atomically persisted at `transaction.rs:750-781`; blocking checks stop before lock creation; final live verification runs at `782` and `1792-1850`. | **Pass for core lock gating, partial for operation journaling.** MCP health runs inside readiness at `1853-2179` without a journal operation or command-boundary checkpoint. |
| 12 | rollback record | Exact lock bytes are hashed before finalization, the journal enters `finalizing`, rollback record/hash are durable, and only then is the lock written at `transaction.rs:790-840`. | **Pass on the normal path.** TR-03 remains for a journal failure immediately after lock commit, and rollback child stage accounting does not satisfy the same 12-stage ledger. |

The successful-lock gate itself is strong on the ordinary path: readiness blockers are rejected at `transaction.rs:770-781`, every live operation is rechecked at `782`/`1792-1850`, exact serialized lock bytes are journaled at `790-812`, and the lock is written only after the rollback record and its hash at `813-829`. Tests `blocked_readiness_never_writes_a_success_lock` (`transaction.rs:5305-5331`), `failure_before_final_rollback_checkpoint_does_not_write_lock` (`5667-5693`), and `finalization_rejects_a_substituted_success_lock` (`5480-5520`) cover important portions.

## Journal state findings

### Positive evidence

- New operations carry ID, destination, immutable ownership from the plan, component, source path/size, action, location scope, external flag, before hash, expected result hash, source hash, result hash, rollback instruction, and reviewed resolution in `new_journal` (`transaction.rs:316-394`).
- Backup path/hash are added after verified backup (`transaction.rs:1304-1323`), staged hash/status after staging (`1384-1394`), and observed result hash/existence after apply (`1597-1619`).
- Every audited journal write call routes through `persist_journal` → `atomic_write_json` (`transaction.rs:865-868`), including rollback journals. No direct `fs::write` to a journal was found in the named files. The implementation of `atomic_write_json` is outside the bounded file list, so its fsync/file-replace/directory-fsync internals were not independently re-audited here.
- Journals are root- and UUID-bound by `validate_journal_project_root` (`transaction.rs:397-435`); rollback and discard invoke it before live mutation/deletion (`3095`, `3915`).
- Finalization recovery validates the exact lock bytes, rollback-record reference/hash, transaction/project/plan binding, operation IDs/status/observations, and live filesystem evidence at `transaction.rs:3529-3693`.
- `migrate_journal` is used by `read_journal` (`transaction.rs:3522-3527`, `migrations.rs:315-323`). Legacy action/rollback/ownership absence is treated conservatively by rollback at `transaction.rs:3179-3200`.

### TR-01 — incomplete journals are hidden and not exclusive

`stage_start` persists ordinary active states such as `preflight`, `resolving`, `downloading`, `verifying`, `backing_up`, `staging`, `validating`, `applying`, `post_check`, and `reporting` (`transaction.rs:925-958`). A real process kill does not execute the error catcher at `transaction.rs:844-860`, so those active states remain on disk.

Startup discovery only accepts `interrupted`, `rolling_back`, or `finalizing` (`commands.rs:4373-4412`, especially `4400-4407`). It also silently skips unreadable/corrupt journals at `4396-4399`. Therefore process termination during ordinary staging or apply can leave a project-mutating journal that the UI never reports.

The core runner does not compensate: `run_transaction` proceeds from plan/root validation directly to new transaction storage and mutation (`transaction.rs:501-542`) without scanning for another bound incomplete journal. `apply_installation` likewise calls it directly (`commands.rs:4296-4327`). A new transaction can therefore overlap a hidden partial transaction.

This violates preflight’s required incomplete-journal check, process-kill recovery, and false-success prevention. It is the highest data-loss risk in scope.

### TR-03 — post-lock transient journal failure creates split-brain state

After the exact success lock is written (`transaction.rs:824-830`), `stage_complete` persists the final stage with `?` (`831`). If that write fails, control reaches the general error handler, which changes the in-memory state from `finalizing` to `interrupted` and best-effort persists it (`844-859`). If the failure is transient and this second write succeeds, the disk has:

- an exact success lock,
- a verified rollback record,
- an `interrupted` journal with `project_apply_started = true`.

`resume_transaction` only enters finalization reconciliation when state is exactly `finalizing` (`transaction.rs:3720-3727`); the resulting `interrupted` state is rejected because apply started (`3729-3737`). Rollback remains possible, but the app cannot reconcile evidence that was already durable. This is a lock/command/recovery false-success split.

### TR-08 — resume is over-advertised

`new_journal` sets resume and discard allowed immediately (`transaction.rs:381-387`), and ordinary failures before apply are converted to a recommended `resume` (`847-853`). But `resume_transaction` requires every non-skip/non-delete mutating operation to already have a regular staged file with the expected hash (`transaction.rs:3789-3875`). A failure in stages 1-6, before a given staged file exists, is therefore not resumable despite the journal/UI recommendation.

The only positive resume test fails immediately before validation, after all staging is complete: `interrupted_pre_apply_transaction_can_resume_from_verified_staging` at `transaction.rs:5369-5411`. There is no early-stage resume test.

### TR-09 — schema and example evidence is weaker than runtime evidence

- `transaction-journal.schema.json:8-17` does not require `transaction_kind`, `project_root`, `plan_sha256`, result-lock evidence, or rollback-record hash.
- Journal operations require only ID/status/destination/ownership (`transaction-journal.schema.json:143-151`). `action`, rollback instruction, source/result/backup/staged/observed hashes, and existence are optional to preserve legacy readability.
- There is no `before_exists`/expected-existence field. `before_sha256: null` cannot by itself distinguish “expected absent” from “unknown,” contributing to TR-02.
- The journal has no external-action collection or per-Git/MCP operation records.
- Journal stage arrays have no exact count/order requirement (`transaction-journal.schema.json:101-141`); plan stage arrays have only `minItems: 12` (`installation-plan.schema.json:267-276`).
- The checked-in journal example repeats `staged_sha256` in both operations (`docs/examples/transaction-journal.example.json:134-136` and `156-158`). `legacy_journal_defaults_to_an_installation_transaction` parses this example through `serde_json` (`migrations.rs:526-537`), which does not test duplicate-key rejection or schema conformance.

Runtime new-plan validation is stricter than these schemas, but recovery is explicitly a persistence/schema surface; the mismatch should not remain.

## Apply and rollback findings

### Positive apply and rollback behavior

- Every replace/delete target is backed up before apply, with a live precondition and backup hash check (`transaction.rs:1246-1326`).
- Live preconditions and staged expected result hashes are rechecked immediately before each mutation (`transaction.rs:1523-1577`).
- Mutating intent is journaled as `applying` before delete/replace (`1578-1588`).
- Atomic temporary copy, file flush, and platform replace/rename are used for installed bytes (`1638-1735`).
- Destination bytes/existence are observed immediately and checked again in post-install and final-live passes (`1597-1619`, `1737-1785`, `1792-1850`).
- Rollback starts by validating the current lock before project files (`transaction.rs:2799-2851`, `2939-2941`, `3090-3114`), creates/reuses a child rollback transaction, and backs up post-transaction bytes for inverse rollback (`2680-3051`).
- Parent and child `rollback_applying` intents are persisted before rollback mutation (`3227-3236`); live later-user-work hashes are checked at `3237-3274`; restored hashes are verified at `3327-3340`.
- Predecessor lock restoration/removal is exact and hash-aware (`3423-3519`).
- Explicit skips/external/legacy-null rollback instructions are no-ops (`3179-3200`).
- Inverse rollback checks later file and lock edits. Tests cover user-file refusal (`transaction.rs:4915-4953`), lock refusal (`4955-4997`), ordinary later-lock refusal (`5522-5552`), and a successful inverse (`4833-4913`).

### TR-02 — crash in the delete boundary is not roll-backable

For an existing `delete_managed` target:

1. `applying` intent is persisted (`transaction.rs:1578-1588`).
2. The file is removed (`1590-1593`).
3. Only afterward are `after_exists = false` and the post-operation journal written (`1597-1619`).

If the process dies between steps 2 and 3, resume is forbidden because project apply started (`transaction.rs:3734-3737`). Rollback sees an `applying` operation, an absent current file, and a non-null `before_sha256`; it rejects this as uncertain at `transaction.rs:3257-3272` instead of using the verified backup and the filesystem evidence that the intended deletion occurred.

The equivalent replace boundary accepts either the before or expected incoming hash (`3257-3265`), but delete does not accept the intended absent result. No process-kill test targets this window.

### TR-04 — flattened keep/skip cannot cross the transaction boundary

Conflict resolution for `keep`/`skip` changes the operation to `Skip`, clears the result hash, and removes the prepared file (`commands.rs:4200-4209`). `refresh_flattened_outputs` intentionally preserves a reviewed flat keep and keeps the generated artifact (`commands.rs:3135-3205`); the inline test explicitly leaves no prepared flat file while retaining the flat artifact (`commands.rs:5008-5288`, especially `5213-5226` and `5266-5287`).

At mutation time, `validate_flatten_transaction_inputs` rebuilds the full flat output (`transaction.rs:1205-1237`) but filters the expected generated artifact out when its operation is a keep/skip (`1211-1228`, via `flatten_input_uses_incoming` at `1147-1164`). The rebuilt map still contains the artifact, so `expected != rebuilt_map` and apply fails at `1238-1241`.

Replace and rename retain prepared bytes and do not hit this mismatch. The missing scenario is an end-to-end transaction with an existing flat destination resolved to keep or skip.

### TR-05 — merged update uses an unverified ancestor

`update_operations` copies incoming manifest ownership and sets the prior installed hash as `base_sha256` (`transaction.rs:4169-4223`), but the lock stores only that hash, not the installed result bytes. When a modified maintenance file gets a merge context, `build_maintenance_plan` fetches the old repository source (`commands.rs:3842-3854`), adapts AGENTS/Codex config if applicable (`3855-3868`), and uses those bytes as `MergeContext.base` (`3881-3889`).

For `Ownership::Merged`, the old repository source is not necessarily the prior installed merged result. The contract expressly says repair/reinstall/update must not reconstruct a merged file from a raw source blob without a verified merge base. Update still offers `merge` whenever this context exists (`commands.rs:3976-4007`).

Related ownership issue: incoming update files take `selection.ownership` (`commands.rs:3600-3617`), and `update_operations` copies that into the operation (`transaction.rs:4200-4204`). `build_lock` then writes operation ownership (`transaction.rs:2214-2221`, `2342-2374`). There is no check that an existing path retains its locked ownership if a future manifest changes the field. The test `merged_ownership_survives_a_replace_and_stays_non_removable` manually supplies `Merged` on both plans (`transaction.rs:5091-5156`); it does not test the maintenance planner or a changed incoming ownership.

### TR-06 — external/Git operation boundaries are incomplete

Plan external actions have IDs and detailed reviewed evidence, but `validate_plan` rejects `OperationAction::External` in the operation list (`transaction.rs:184-191`), and the journal schema has no external-action records. MCP health executes during readiness (`transaction.rs:2004-2015`) without per-command intent/result checkpoints or command-boundary fault injection.

Git executes after post-install stage completion and before readiness stage start (`transaction.rs:715-750`). Initialize mode pre-journals a requested `git_initialized = true` (`723-725`), but preserve-mode remote name/URL are stored only after `apply_git_setup` succeeds (`740-748`). A process kill after a remote is added but before lines 743-748 leaves no expected name/URL for rollback (`transaction.rs:3130-3140`), even though the contract requires those expected values before mutation.

### TR-07 — rollback child stage ledger is false

`new_rollback_journal` creates all 12 stages as `pending` (`transaction.rs:2680-2711`). `prepare_rollback_transaction` marks only stage index 5, backup, complete (`3044-3050`). The rollback then performs Git cleanup, file apply, lock restoration, records, and final completion without activating/completing the other stage entries (`3090-3408`); the child is marked `state = "completed"` at `3372-3388`.

Rollback operation and record durability are materially better than the stage ledger suggests, but the required stage-coverage evidence is not truthful.

### TR-10 — structured merge behavior does not match accepted rules

- `merge_json_value` merges arrays positionally whenever lengths match (`merge.rs:263-275`). There is no manifest-declared identity key input. `docs/10_merge_conflict_rules.md:60-63` requires identity-key semantics and manual review for unknown arrays.
- TOML and JSON object merge loops iterate incoming keys only (`merge.rs:166-200`, `242-260`). A key present in base/local but deleted upstream is never removed, even when local still equals base.
- `allowed_choices` offers merge for any text/TOML/JSON `Conflict` except `UserOwnedConflict` (`merge.rs:61-77`). `classify` uses `Conflict` when local is absent or incoming is absent (`53-57`), so the generic API can advertise a merge without all three inputs. The command maintenance path is more conservative because it adds merge only when a `MergeContext` exists (`commands.rs:3997-3999`).
- Tests cover binary exclusion, one safe text case, and disjoint TOML additions only (`merge.rs:328-368`). No deletion, JSON array identity/reorder, missing-side, AGENTS, or persistence-signature tests exist.

### Maintenance and credential behavior

The following accepted behaviors are present:

- Update preserves latest/pinned mode and requires a fresh core-session maintenance reanalysis (`commands.rs:3243-3332`, `3511-3547`).
- Repair/reinstall use the locked source revision (`commands.rs:3643-3665`); adding 3D pins the exact lock revision and manifest hash (`3666-3729`).
- Healthy repair files are no-ops; missing ordinary managed/generated files become creates; modified files are skipped for review (`transaction.rs:3974-4050`).
- Reinstall preserves modified, merged, and external files pending review (`transaction.rs:4105-4164`).
- Managed removal deletes only unchanged non-merged/non-external files and preserves modified/merged/external files (`transaction.rs:4052-4103`); completion reports core not ready (`1858-1927`).
- Removal clears project credential references in the result lock (`transaction.rs:2442-2478`). Vault deletion remains a separate explicit command (`commands.rs:1049-1077`), so managed removal does not implicitly delete the OS credential.
- A first-install kept modification is tracked and stays non-removable; checked-in evidence is `transaction.rs:4797-4831`.

Remaining maintenance gaps are TR-05, the absent end-to-end update/repair/reinstall fault paths, and the fact that removal’s generated readiness summary hardcodes Codex/App Server/ChatGPT labels even for a non-Codex installed lock (`transaction.rs:1872-1885`). The latter does not create a success claim because `core_ready` is false, but it is inaccurate audit output.

## Fault scenario matrix

| Fault/scenario | Checked-in mechanism or test | What is asserted | Audit verdict |
| --- | --- | --- | --- |
| Before/after each of 12 stages | `TransactionOptions.fail_before_stage/fail_after_stage`; `stage_and_operation_fault_matrix_never_writes_success_lock`, `transaction.rs:5815-5842` | Runner errors and no success lock. | **Partial.** Single create operation; no recovery/final-hash assertion. “After” is an injected returned error, not process termination. |
| Before/after file operation | Same matrix at `transaction.rs:5843-5864` | One create operation errors and no lock. | **Partial.** Does not cover replace, merge, rename, skip, external launcher, delete, Git, MCP, or rollback operations. Skip/external bypass the after hook at `transaction.rs:1509-1521`. |
| Real process termination | Cross-process abort test at `transaction.rs:4516-4666` | After rollback record, after lock write, rollback-child backup, and inverse-child backup can recover. | **Partial.** No kill during normal stages, staging file write, apply intent→mutation→result, Git, readiness health, journal write, or lock-stage journal update. |
| Disk full | No injectable filesystem adapter/test in named files. | None. | **Missing.** |
| File/antivirus lock | No adapter/test in named files. | None. | **Missing.** |
| Permission denial | No adapter/test in named files. | None. | **Missing.** |
| Network loss/partial download | No transaction fault test in named files; source fetch happens during plan building. | None here. | **Missing in this bounded transaction suite.** |
| Checksum mismatch/cache corruption | `transaction_revalidates_prepared_bytes_before_backup`, `transaction.rs:4418-4448`; staged hashes at runtime. | Fails before backup/live mutation and no lock. | **Covered for tampered prepared bytes.** No cache/network-resume case here. |
| Staged validation failure | Parser paths exist at `transaction.rs:1399-1490`. | No targeted malformed descriptor/TOML/JSON/PNG/AGENTS stage test. | **Mechanism present, test missing.** |
| Readiness/health failure | `blocked_readiness_never_writes_a_success_lock`, `transaction.rs:5305-5331`. | No lock on a blocking readiness result. | **Core gate covered.** No targeted MCP timeout/crash/identity failure transaction test in named files. |
| Command timeout | No transaction fault hook around MCP/Git/3D command boundaries. | None. | **Missing.** |
| Cancellation | No transaction cancellation hook/test. | None. | **Missing.** |
| Journal write failure | No injectable journal writer/test. | None. | **Missing; TR-03 is untested.** |
| Rollback after later file edit | `transaction.rs:4915-4953`. | Refuses and preserves user bytes. | **Covered.** |
| Rollback after later lock edit | `transaction.rs:4955-4997`, `5522-5552`. | Refuses before file restoration. | **Covered.** |
| Rollback retry after restored operation | `transaction.rs:5050-5089`. | Recognizes already-restored destination. | **Covered for replace.** |
| Predecessor lock restoration | `transaction.rs:5246-5303`. | Exact predecessor lock and file restored. | **Covered.** |
| Inverse rollback | `transaction.rs:4833-4913`, `4605-4666`. | Post-transaction bytes/lock can be restored; child identity/backups retained. | **Covered for one replace.** |
| Flatten conflicts | Refresh-only tests at `commands.rs:5008-5320`. | Keeps a reviewed flat decision and excludes unresolved incoming bytes. | **Partial.** No apply/recovery/rollback; TR-04 is not detected. |
| Update/repair/reinstall/removal | Helper tests at `transaction.rs:4668-4831`, `5695-5812`; reanalysis/add-3D tests at `commands.rs:4803-4939`. | Selected planner properties. | **Partial.** No end-to-end update/reinstall/repair fault matrix, merged update base, ownership-change, or credential-removal rollback test. |

## Data-loss and false-success risks

1. **Highest data-loss risk — overlapping a hidden partial transaction (TR-01).** A kill during `applying` can leave live files changed and no success lock. Because startup does not discover that state and core preflight does not enforce exclusivity, a second transaction can take those partial bytes as new local state, overwrite prior backup assumptions, or create a misleading new lock.
2. **Incorrect merge ancestor (TR-05).** A user-approved update merge can omit or misclassify contributions that existed in the prior installed merged result but not in the old raw repository source.
3. **Structured array merge (TR-10).** Positional JSON merge can apply changes to the wrong logical array item after reordering.
4. **Success-lock split brain (TR-03).** The lock can be durable while the API reports failure and the journal is rewritten to a state that cannot use finalization recovery.
5. **Rollback incompleteness, not silent deletion (TR-02/TR-06).** Delete-boundary crashes fail closed and require manual work; preserve-mode Git remote crashes can leave an unrecorded remote. These are safer than guessing, but they do not meet “resumable or explicitly roll-backable.”
6. **No evidence of silent deletion for reviewed skips or later user edits.** Explicit skip/external/null rollback is conservative, lock edits are checked before file restore, and inverse rollback refuses later file changes.
7. **No ordinary-path false success on blocking core readiness.** The readiness report is persisted before the gate and the lock is withheld on blockers.

## Exact code and test evidence summary

Positive evidence:

- Exact 12-stage plan validation: `transaction.rs:23-33`.
- Core-owned plan session/fingerprint: `commands.rs:389-424`, `4296-4321`.
- Plan/journal creation before stage mutations: `transaction.rs:520-542`.
- Backup before live mutation: `transaction.rs:1246-1326`.
- Staging and validation: `transaction.rs:1328-1490`.
- Apply intent, precondition, expected staged hash, observed result: `transaction.rs:1492-1635`.
- Atomic live replacement route: `transaction.rs:1638-1735`.
- Post/final destination verification: `transaction.rs:1737-1850`.
- Readiness-before-lock and finalizing record: `transaction.rs:750-840`.
- Exact finalization reconciliation: `transaction.rs:3529-3693`.
- Root-bound rollback/discard: `transaction.rs:397-435`, `3090-3114`, `3897-3957`.
- Separate rollback child/inverse backup: `transaction.rs:2680-3051`, `3090-3408`.
- Maintenance planners: `transaction.rs:3974-4273`; command assembly: `commands.rs:3418-4109`.
- Explicit credential deletion only: `commands.rs:1049-1077`; project-reference clearing: `transaction.rs:2442-2478`.

Negative evidence:

- Active-state discovery omission: `commands.rs:4400-4407`.
- No incomplete-journal preflight: `transaction.rs:501-542`; direct call at `commands.rs:4296-4327`.
- Delete crash rejection: `transaction.rs:1590-1619`, `3257-3272`.
- Post-lock journal split: `transaction.rs:824-859`, `3720-3737`.
- Flat keep/skip mismatch: `commands.rs:4200-4209`, `3135-3205`; `transaction.rs:1147-1243`; test gap at `commands.rs:5008-5288`.
- Raw-source merged base: `commands.rs:3842-3889`, merge offered at `3976-4007`.
- Incoming ownership replacement: `commands.rs:3600-3617`; `transaction.rs:4200-4204`, `2214-2374`.
- Git outside stage/operation journal and late remote evidence: `transaction.rs:715-750`, rollback dependency at `3130-3140`.
- Rollback child false stage ledger: `transaction.rs:2680-2711`, `3044-3050`, `3372-3388`.
- JSON positional arrays/deletion omissions: `merge.rs:166-200`, `242-275`.
- Duplicate journal-example keys: `transaction-journal.example.json:134-136`, `156-158`.

## Skipped meaningful scenarios

These scenarios are required or materially risk-bearing but have no checked-in proof in the bounded files:

- process kill in every ordinary active stage state and before/after every file mutation;
- process kill after `delete_managed` removal and before observed-result persistence;
- process kill after preserve-mode Git remote addition and before journaled remote identity;
- transient and permanent journal failures before apply intent, after live mutation, after rollback mutation, and immediately after lock commit;
- recovery discovery of active and corrupt journals, plus core refusal to start an overlapping transaction;
- resume from pre-staging and partially staged checkpoints;
- full action matrix: create, replace, merge, rename, skip, external launcher, delete, generated, Git initialize, Git remote, MCP health;
- disk full, permission loss, file/antivirus lock, command timeout, network loss, and cancellation on Windows and macOS;
- rollback fault injection before/after every parent and child operation, including lock restoration and record writes;
- rollback after a delete intent with the live file already absent;
- end-to-end flattened keep, skip, replace, rename, interruption, resume, and rollback;
- update of a previously merged file with verified prior installed-result bytes;
- ownership preservation when incoming manifest ownership differs from the lock;
- JSON array identity/reorder and upstream key deletion;
- complete repair, reinstall, update, managed-removal, and explicit credential-removal transactions with predecessor-lock rollback;
- external launcher descriptor maintenance and rollback under interruption;
- duplicate-key rejection and actual JSON Schema validation for all three examples.

## Recommended parent actions — bounded fix order

1. **Make incomplete transaction detection and exclusivity core-owned.** Treat every nonterminal journal state, including ordinary active states and unreadable/corrupt candidate journals, as an inspect/recovery blocker for the bound root. Enforce this inside transaction preflight, not only in startup UI. Add a root-scoped transaction lease/lock with safe stale-owner recovery.
2. **Fix filesystem-evidence recovery at irreversible boundaries.** Add explicit expected-before existence and expected-after existence to new journal operations. Permit rollback of an `applying` delete when the verified backup exists and live absence matches the intended delete result. Test a real process kill in that exact window.
3. **Preserve `finalizing` after lock commit.** Once the lock write succeeds, never downgrade to generic `interrupted`; journal failures must retain/reconstruct a finalization-reconcilable state from exact lock and rollback-record evidence.
4. **Repair flattened keep/skip semantics.** Model the preserved flat destination as a verified no-op while comparing rebuilt incoming evidence separately, or exclude the rebuilt artifact symmetrically. Add end-to-end apply, interruption, and rollback tests for all flat conflict choices.
5. **Require a verified installed-result merge base.** Store/recover prior installed merged bytes in a bounded verified base cache/backup, or refuse merge and offer only conservative outcomes when the bytes are unavailable. Preserve locked ownership for existing paths regardless of incoming manifest ownership unless an explicit migration decision exists.
6. **Journal Git/MCP/external operations as real operations.** Persist stable IDs, preconditions, expected identities, intent, observed result, and rollback boundary before/after each action. For preserve remotes, journal expected name/URL before calling Git.
7. **Make rollback child stage evidence truthful.** Either execute/mark the applicable 12 stages in order or define and schema-validate an explicit rollback-stage model approved by the product contract.
8. **Align schemas and examples.** Enforce exact stage constants/order, require all new-journal evidence while retaining an explicit legacy migration form, add expected-existence fields and external operations, remove duplicate example keys, and validate unique-key JSON plus schemas in release tests.
9. **Correct structured merges.** Require manifest array identity keys, reject unknown arrays, implement upstream deletions only when local remains at base, and add TOML/JSON deletion/reorder tests.
10. **Expand fault tests from “no lock” to recovery invariants.** For each stage/action and native fault class, assert discovery, allowed recovery choices, exact final hashes, predecessor/result lock state, parent/child journal durability, no later-user-work loss, and no success lock unless every final check and record is durable.

Parent completion should remain blocked until at least actions 1-6 have implementation and targeted regression evidence. Actions 7-10 are also required before claiming the full transaction/recovery contract and release fault gate are complete.
