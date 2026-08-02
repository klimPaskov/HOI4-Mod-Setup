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
- `docs/schemas/installation-plan.schema.json`
- `docs/schemas/transaction-journal.schema.json`
- `docs/schemas/installation-lock.schema.json`
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
Do not create the backup or staging directories before their corresponding
stages. Source resolution, selective download evidence, and checksum
verification must fail before any predecessor or project-file backup is
written.

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

For large operation sets, append bounded per-operation records to one checkpoint log so durability does not require serializing the complete journal for every file. Backup, staging, apply, rollback-child backup, and rollback-apply records are compacted into an atomic full-journal snapshot every 1,024 completed operations and at each stage boundary. Before apply or rollback mutates a live file, persist durable intent records for a group of at most 64 operations; completed records then supply per-file recovery and progress evidence. Journal reads replay only records newer than the snapshot and reject links, wrong transaction or operation bindings, oversized records, oversized logs, and torn records except for an incomplete final append. Recovery resolves a crash inside a group from the durable intents, verified backups, and observed live hashes.

For Unix destinations, the manifest/lock executable declaration is durable
metadata evidence. Apply and verify it in staging and live paths, record
before/expected/after executable state in the journal, preserve it in backups,
restore it on rollback, and verify it during readiness. Standalone `chmod`
operations are not accepted; mode belongs to the reviewed file operation.

## Apply rules

- Backup all replace or delete targets before mutation.
- Stage files outside live destinations.
- Carry selected starter folders as normalized reviewed directory entries.
  Stage and validate them as directories, create them at apply without marker
  files, journal the paths that were absent before mutation, and remove only
  transaction-created directories that remain empty during rollback.
- Validate staged output.
- Recheck preconditions immediately before each live operation.
- Use atomic rename or replace where supported.
- Verify destination hashes after apply.
- Do not write a successful lock until all required post-install checks pass.
- Persist the readiness report, then refuse stage 12 and the success lock when any blocking core check is `block`; optional `incomplete`, `planned_unavailable`, and unsupported optional routes remain non-blocking.
- Persist an `applying` operation intent before replacing or deleting a live destination. Use the platform atomic replace route where available and verify the expected incoming hash, not only an observed self-hash.
- Bind UI apply to a core-owned reviewed plan session and prepared bytes. The renderer sends only the approved plan ID and project root when installation starts; never reserialize or accept a renderer-edited plan as authoritative.
- Track source hash separately from result hash: generated files, structured merges, and optional MCP TOML adaptation may have a verified incoming source hash and a different deterministic installed hash.
- Every remote prepared file has one plan `download_ledger` entry bound to its
  operation ID, component, source path, destination, exact source revision,
  manifest hash, source hash, size, ownership, platform, and executable
  declaration. Revalidate that
  ledger before backup. Generated inputs use an explicit `generated:` source
  and do not fabricate remote evidence.
- Treat provider/model/profile selection and the Codex-only flattened ChatGPT-source export as reviewed plan inputs. Flattening is a generated, root-contained operation: direct skill `SKILL.md` files become `<skill>.md`; required adapted `AGENTS.md`, README, and subagents are included; and collisions, links, and secrets are rejected before staging. Build it only from eligible files selected for the current plan. Never include offline-wiki pages, wiki media, descriptors, configuration, workflow assets, or other unrelated selected component files in the flat view or count them against its limits. When review keeps a selected local skill or subagent, read that exact root-contained regular file without following links and flatten the kept bytes; never enumerate unrelated skills or subagents already present in an existing project.
- Deterministically adapt every selected Codex subagent definition before
  staging so its developer instructions explicitly require
  `fork_context=false`, unless verified TOML already declares the same rule.
  Reject an explicit `fork_context=true`. Preserve the verified source hash
  separately from the adapted result hash so readiness validates exactly the
  installed bytes without weakening source evidence.
- When a non-flattened conflict changes the accepted source set, rebuild the flat
  view from accepted bytes only. Preserve an already reviewed flat keep/replace/
  rename decision only when its incoming hash and local precondition still
  match; otherwise recreate the conflict and require review again.
- Carry the resolved manifest's exact `wiki.required_pages` list and its
  snapshot/media/provenance/license metadata in both plan and lock. Readiness
  must use that exact evidence; if a legacy lock lacks either the page list or
  metadata, keep the lock readable but report source/wiki evidence incomplete
  until update or repair refreshes the lock.
- External launcher descriptors are represented by an explicit absolute destination plus `external=true`; lock and readiness code must validate that path separately from project-relative destinations.
- Manifest-declared repository scripts remain approval-bound external actions in the core plan. The dry run presents them as concise **Setup checks** with technical command details behind a second disclosure; do not expose internal risk labels or developer-facing component IDs in the normal summary. Their command source and platform are copied from the verified manifest; a successful project transaction must not imply that an optional external action or provider health check passed.
- When the MCP component is selected and the reviewed manifest supplies
  immutable executable, command-interpreter, and runtime identity evidence,
  transaction readiness must use the reviewed external action to perform the
  bounded initialize and read-only `tools/list` probe after apply; a verified
  route failure blocks the success lock. When any identity evidence is absent, the action is
  `planned_unavailable`, no same-named `PATH` command is run, and the optional
  state remains non-blocking. Installed-project refresh performs the same
  lock/config/source binding, while deselected and unsupported-platform MCP
  states remain non-blocking.
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

### Absent-root first install

- Only a new, non-maintenance plan may use `project_root_mode: create_leaf`.
  The plan and journal carry a canonical existing parent plus a normalized leaf
  equal to the reviewed project ID; the parent must validate before any
  transaction storage or project write proceeds.
- `run_transaction` journals and validates the absent destination through
  source resolution, download/checksum verification, dry-run, backup, staging,
  and staged-output validation without creating the project root. The root is
  created exactly at the apply boundary with durable `applying` and `created`
  lifecycle checkpoints.
- A failure before apply leaves the reviewed root absent and permits only the
  exact staging-discard path. Rollback removes a transaction-created root only
  after managed files and metadata are gone and the directory is empty; if
  user content appears, it removes no more than verified managed content and
  records `retained_user_content`.
- Inverse rollback recreates only the reviewed empty leaf, records its own
  lifecycle checkpoint, and refuses to overwrite later content or recreate Git
  or other external side effects.

## Recovery

When checking whether a rollback destination is already restored on Unix, only
inspect executable permissions after confirming that the destination file
exists. An absent file is a normal intermediate state for inverse rollback of a
newly created project root; report it as not yet restored so the verified backup
can recreate it.

On startup, detect every non-terminal journal state, including ordinary active
stage states left by process termination. The core transaction preflight must
also scan the bound application-data transaction root and refuse overlapping
mutations for the same canonical project. Corrupt journals whose bounded root
matches the selected project are a recovery blocker, not something to skip.
Offer resume, rollback, or discard staging only when the recorded state makes
the action safe.

Immediately before approving or applying a newly reviewed plan, the UI must
ask the core for an existing non-terminal transaction bound to the same
project. When one exists, route to its recovery choices without approving or
applying the new plan. If apply reports an overlap or interruption, discover
the bound non-terminal journal rather than looking up only the new plan ID.
Treat the in-flight command as active work, not an interrupted transaction:
show Progress and suspend renderer-side recovery discovery until the command
returns. Refresh a Recovery screen from core-normalized journal state so an
apply-started transaction offers rollback instead of a stale pre-apply resume
or staging discard.
Progress reads only the exact active transaction ID already approved by the
renderer/core session. It may display journal checkpoints while the command is
running, but it never edits the journal or treats a polled snapshot as success.

Resume must compare the recorded plan hash, operation preconditions, journal expectations, staged-file hashes, the exact predecessor-lock existence/hash state, and observed filesystem state. `resume_transaction` replays the full runner only from a pre-apply interrupted checkpoint; once project apply has started it refuses resume and requires rollback or manual review. Never trust the last journal line alone.

The final lock write has a separate `finalizing` journal state. The journal
records the exact serialized success-lock hash before the rollback record is
written. If the process stops after the lock and rollback record are durable,
resume verifies the exact lock bytes, rollback-record path/checksum,
transaction/project binding, live operation observations, and stage checkpoint
before completing the journal; it never replays file operations.
`staged_sha256` is separate from the observed live `after_sha256`.
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

Every direct journal read rejects a symlink or junction in the journal path,
including a linked transaction directory; discovery-time checks are not a
substitute for command-boundary validation.

Discard staging removes only the exact transaction UUID staging directory after the same pre-apply state checks. It preserves the journal and backup material for audit/review, and records a terminal `staging_discarded` journal state.

Rollback restores original content and metadata within supported limits. It must not delete user work created after the transaction without review.

When a prior installation lock exists, copy and hash it into the transaction backup before mutation. A maintenance lock refresh carries forward skipped files, ownership, merge choices, local modifications, optional states, and rollback history. Rollback validates the current lock against the exact recorded result or already-restored predecessor before touching project files, restores the verified predecessor lock, and removes a current lock only when no predecessor exists. Explicit `skip`, `external`, and `rollback: none` operations are rollback no-ops and must never delete the user's preserved destination.

The lock is the durable remembered installation state: scanning may expose only
its bounded non-secret summary, the current session carries the completed scan
context and readiness report, and installed readiness re-evaluates the lock's
components, source evidence, and optional states before reporting success.
Transient UI state must never manufacture an installed component or readiness
pass.

Legacy locks may contain `workflow.lora_comfyui_interest`. Keep them readable
for recovery, but ignore that preference during scanning and readiness and
remove it from every newly verified lock; it is no longer a product workflow.

Opaque OS-vault credential references are carried separately from secret values. A lock refresh may preserve the non-secret Meshy reference only in the selected `workflow.3d` entry; provider-key references remain core-owned and outside project state, plans, and locks. Managed removal clears optional-workflow references without deleting an OS credential implicitly. A flatten preference is copied only when the selected provider is Codex.

## Maintenance

- Update compares base, local, and incoming.
- Repair defaults to the locked revision.
- If a valid lock reports `workflow.3d` as `not_selected`, repair may expand
  that component and its manifest-declared dependencies at the exact locked
  revision. Add only missing component ownership; equal local bytes become a
  tracked no-op/replacement baseline and different bytes become a
  `review_required` keep/replace/merge/rename/skip conflict.
- Repair may add `workflow.super_events` or another optional component only
  when that component and its dependencies are declared by the immutable
  locked revision and its manifest hash still matches the lock. If the desired
  component is absent from that locked source, Update must resolve the current
  source first; Repair must not mix a newer component into the old lock.
- Reinstall preserves local modifications through the same conflict engine.
- Managed removal deletes only owned, unmodified content by default.
- A credential removal is a separate explicit choice.
- Optional workflow health is separate from the core lock gate. A stored Meshy reference may be carried forward only for `workflow.3d`, but a workflow is `incomplete` or `selected_pending` until its approved, manifest-derived health route reports success; missing optional credentials never block the core readiness gate.
- `workflow.super_events` has no credential health action; its installed or
  `not_selected` state is recorded in the lock and its readiness check is
  always `blocking: false`.
- Repair may add the 3D files at the lock's exact revision and may refresh only the opaque Meshy reference for an already-selected incomplete workflow; a previously `ready` workflow remains ready when its existing reference is carried forward.
- Reinstall and repair must re-read the locked revision and preserve modified files for review. Managed removal must not delete merged ownership wholesale. The current planners are `repair_operations`, `reinstall_operations`, `update_operations`, and `managed_removal_operations` in `src-tauri/src/transaction.rs`.
- Every successful Update, Repair, Reinstall, or Remove plan opens the ordinary dry-run review, or conflict review when unresolved conflicts exist. Do not leave a newly built maintenance plan on the action-selection screen.
- Update preserves the lock's latest/pinned source mode and filters generated artifact IDs out of manifest component expansion. Descriptor, external launcher, and thumbnail artifacts are planned independently for both new and existing projects.
- Repair and reinstall do not reconstruct a merged file from a raw source blob without a verified merge base; missing or changed merged files become reverse-merge review entries.
- Update planning requires a fresh confirmed selected-provider semantic reanalysis record from the current core session. The UI must first complete a bounded read-only scan, show the approved evidence manifest, run the existing-project common-schema turn, and pass only the core-confirmed record into `build_maintenance_plan`. The record is bound to the canonical project root, latest scan ID, evidence-manifest SHA-256, provider, model, and optimization profile; repair, reinstall, and managed removal may continue from the validated locked analysis according to their authenticated/recovery rules.

## Lock and rollback evidence

Read journals through `migrations::migrate_journal`. The journal records action, component, location scope, source path/size, reviewed resolution, source/result/backup hashes, staged and observed live hashes, expected and observed existence, and per-operation intent. Rollback must refuse ambiguous later user work, restore verified backups and lock state, and persist both the parent and child rollback records after the state transition. `rollback_source_path` identifies the parent backup that supplies restored bytes; the child `backup_path` identifies the inverse backup; completed child journals also record the lock state expected before an inverse action.

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
- immediately after a live file or metadata mutation but before the operation
  result is observed and journaled

Verify that recovery never creates a false success lock and rollback restores expected hashes.

The current Rust fault test iterates every stage and operation before/after boundary and asserts that no success lock is written; targeted regression tests also cover absent-root creation at apply, pre-apply staging discard, root removal, inverse recreation, later-user-content retention, skipped-file rollback, immutable ownership, remote approval, reanalysis scan binding, inverse rollback refusal after later file or lock edits, and the completed child inverse path. Native execution still requires the supported CI toolchains.

Maintenance regression coverage must also prove that Repair uses only the
locked revision, rejects a component absent from that revision, and that
Update is the path that can acquire it from a newly resolved source.

## Update this skill when

Update this skill when stages, operation records, journal transitions, backup policy, atomicity, conflict decisions, lock timing, recovery options, repair, reinstall, rollback, or removal behavior changes.

## Completion standard

Transaction work is complete only when the twelve stages, journal and
predecessor-lock evidence, backup/staging/apply boundaries, readiness gate,
rollback record, maintenance path, and fault scenarios agree; optional
external actions never turn into implicit core success.
