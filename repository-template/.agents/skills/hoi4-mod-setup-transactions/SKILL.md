---
name: hoi4-mod-setup-transactions
description: Use for installation plans, dry runs, backups, staging, apply, transaction journals, interrupted recovery, rollback, update, repair, reinstall, or managed removal.
---

# Staged transaction and recovery

## Required sources

Read:

- `AGENTS.md`
- `docs/10_merge_conflict_rules.md`
- `docs/14_transaction_rollback.md`
- `docs/15_update_repair.md`
- `schemas/installation-plan.schema.json`
- `schemas/transaction-journal.schema.json`
- `schemas/installation-lock.schema.json`
- corresponding examples

## Stage order

Every mutation uses:

1. preflight
2. repository source resolution
3. selective download
4. checksum verification
5. dry-run review
6. backup
7. staging
8. validation
9. apply
10. post-install checks
11. readiness report
12. rollback record

Do not collapse these into a single copy operation.

## Operation model

Every operation needs:

- stable operation ID
- action type
- source and destination identity
- expected precondition
- expected incoming hash
- ownership type
- conflict decision
- backup reference
- stage status
- apply checkpoint
- observed result
- rollback instruction

Ownership is immutable plan and journal evidence. It is never inferred from
the current action, source path, or whether a destination was skipped. A
missing ownership value is rejected for new plans; legacy journals remain
readable but rollback treats missing action or rollback metadata as a
non-destructive no-op until the project is re-planned.

Persist the journal before and after irreversible boundaries. Use atomic file replacement for journal writes.

## Apply rules

- Backup all replace or delete targets before mutation.
- Stage files outside live destinations.
- Validate staged output.
- Recheck preconditions immediately before each live operation.
- Use atomic rename or replace where supported.
- Verify destination hashes after apply.
- Do not write a successful lock until all required post-install checks pass.
- Persist the readiness report, then refuse stage 12 and the success lock when any blocking core check is `block`; optional `incomplete`, `planned_unavailable`, and unsupported optional routes remain non-blocking.
- Persist an `applying` operation intent before replacing or deleting a live destination. Use the platform atomic replace route where available and verify the expected incoming hash, not only an observed self-hash.
- Bind UI apply to a core-owned reviewed plan session and prepared bytes; do not accept a renderer-edited plan as authoritative.
- Track source hash separately from result hash: generated files, structured merges, and optional MCP TOML adaptation may have a verified incoming source hash and a different deterministic installed hash.
- Carry the resolved manifest's exact `wiki.required_pages` list in both plan and lock. Readiness must use that list; if a legacy lock lacks it, keep the lock readable but report source/wiki evidence incomplete until update or repair refreshes the lock.
- External launcher descriptors are represented by an explicit absolute destination plus `external=true`; lock and readiness code must validate that path separately from project-relative destinations.
- Manifest-declared repository scripts are surfaced as high-risk external actions in the dry run and remain approval-bound. Their command source and platform are copied from the verified manifest; a successful project transaction must not imply that an optional external action or provider health check passed.
- When the MCP component is selected, transaction readiness must use the
  reviewed manifest external action to perform the bounded initialize and
  read-only `tools/list` probe after apply; failure blocks the success lock.
  Installed-project refresh performs the same lock/config/source binding, while
  deselected and unsupported-platform MCP states remain non-blocking.
- External-action details in the plan are secret-free and include argument arrays, cwd, environment names, declared writes, network/privilege evidence, and rollback boundary. `not_declared_by_source` is an honest boundary, not permission to assume rollback.
- A configured Git remote requires `git_remote_approved` in the core-owned
  reviewed plan. Final dry-run approval may set that flag for the local remote
  configuration only; it never approves online repository creation or push.
  Preserve-mode remote additions record the expected name and URL in the
  journal and rollback removes the remote only when the current URL is still
  unchanged; an already-matching preserve-mode remote is a no-op and a
  different URL is rejected. New Git initialization is reversed before file restoration so
  the transaction-created index cannot make managed files appear to be later
  user work.
  Staging and backup roots are reparse-point checked before and during file
  operations. A first-install skip of a modified managed file records the
  incoming hash as the installed baseline and the local hash as a modification,
  so later managed removal preserves that user content.

## Recovery

On startup, detect incomplete journals. Offer resume, rollback, or discard staging only when the recorded state makes the action safe.

Resume must compare the recorded plan hash, operation preconditions, journal expectations, staged-file hashes, and observed filesystem state. `resume_transaction` replays the full runner only from a pre-apply interrupted checkpoint; once project apply has started it refuses resume and requires rollback or manual review. Never trust the last journal line alone.

The final lock write has a separate `finalizing` journal state. If the process
stops after the lock and rollback record are durable, resume verifies both
artifacts and only completes the journal; it never replays file operations.
Rollback uses `rolling_back` plus `rollback_applying` per-operation
checkpoints, verifies an already-restored destination before retrying, and
leaves rollback enabled when an error requires reinspection. Before restoring
the parent transaction, it creates or reopens a child journal with
`transaction_kind: "rollback"`, `parent_transaction_id`, and a separate
backup set of the live post-transaction bytes; retries reuse that child ID.

When the child completes, it records the post-rollback lock state and keeps
the inverse backup. The Ready screen can pass that child ID back through the
same guarded rollback command to restore the managed files and lock state that
existed before rollback. The inverse action checks the recorded root and live
hashes before applying, refuses later user edits, and does not recreate Git or
other external side effects.

Every new journal binds its canonical `project_root`; recovery discovery and commands reject a journal for any other root. Journals from before that binding existed are readable for audit but are not resumable.

Rollback and staging discard must enforce that binding inside the transaction module before resolving any destination or deleting any staging directory. Verify the journal path and transaction UUID as well as the caller-provided root.

Discard staging removes only the exact transaction UUID staging directory after the same pre-apply state checks. It preserves the journal and backup material for audit/review, and records a terminal `staging_discarded` journal state.

Rollback restores original content and metadata within supported limits. It must not delete user work created after the transaction without review.

When a prior installation lock exists, copy and hash it into the transaction backup before mutation. A maintenance lock refresh carries forward skipped files, ownership, merge choices, local modifications, optional states, and rollback history. Rollback restores the verified predecessor lock; it removes a current lock only when no predecessor exists and the current lock contains this transaction's rollback record. Explicit `skip`, `external`, and `rollback: none` operations are rollback no-ops and must never delete the user's preserved destination.

Opaque OS-vault credential references are carried separately from secret values. A lock refresh preserves the prior Meshy reference when no new reference is supplied; managed removal does not delete the OS credential implicitly.

## Maintenance

- Update compares base, local, and incoming.
- Repair defaults to the locked revision.
- Reinstall preserves local modifications through the same conflict engine.
- Managed removal deletes only owned, unmodified content by default.
- A credential removal is a separate explicit choice.
- Optional workflow health is separate from the core lock gate. A stored Meshy reference may be carried forward, but a workflow is `incomplete` or `selected_pending` until its approved, manifest-derived health route reports success; missing optional credentials never block the core readiness gate.
- Reinstall and repair must re-read the locked revision and preserve modified files for review. Managed removal must not delete merged ownership wholesale. The current planners are `repair_operations`, `reinstall_operations`, `update_operations`, and `managed_removal_operations` in `src-tauri/src/transaction.rs`.
- Update preserves the lock's latest/pinned source mode and filters generated artifact IDs out of manifest component expansion. Descriptor, external launcher, and thumbnail artifacts are planned independently for both new and existing projects.
- Repair and reinstall do not reconstruct a merged file from a raw source blob without a verified merge base; missing or changed merged files become reverse-merge review entries.
- Update planning requires a fresh confirmed Codex semantic reanalysis record from the current core session. The UI must first complete a bounded read-only scan, show the approved evidence manifest, run the existing-project schema-constrained turn, and pass only the core-confirmed record into `build_maintenance_plan`. The record is bound to the canonical project root, latest scan ID, and evidence-manifest SHA-256; repair, reinstall, and managed removal may continue from the validated locked analysis according to their signed-in/recovery rules.

## Lock and rollback evidence

Read journals through `migrations::migrate_journal`. The journal records action, component, location scope, source/result/backup hashes, expected and observed hashes, whether a destination existed after apply, and per-operation intent. Rollback must refuse ambiguous later user work, restore verified backups and lock state, and persist both the parent and child rollback records after the state transition. `rollback_source_path` identifies the parent backup that supplies restored bytes; the child `backup_path` identifies the inverse backup; completed child journals also record the lock state expected before an inverse action.

## Fault testing

Inject failure at every stage and operation boundary:

- process termination
- disk full
- file lock
- permission denial
- network loss
- checksum mismatch
- validation failure
- command timeout
- cancellation
- journal write failure

Verify that recovery never creates a false success lock and rollback restores expected hashes.

The current Rust fault test iterates every stage and operation before/after boundary and asserts that no success lock is written; targeted regression tests also cover skipped-file rollback, immutable ownership, remote approval, reanalysis scan binding, inverse rollback refusal after later file or lock edits, and the completed child inverse path. Native execution still requires the supported CI toolchains.

## Update this skill when

Update this skill when stages, operation records, journal transitions, backup policy, atomicity, conflict decisions, lock timing, recovery options, repair, reinstall, rollback, or removal behavior changes.

## Completion standard

Transaction work is complete only when the twelve stages, journal and
predecessor-lock evidence, backup/staging/apply boundaries, readiness gate,
rollback record, maintenance path, and fault scenarios agree; optional
external actions never turn into implicit core success.
