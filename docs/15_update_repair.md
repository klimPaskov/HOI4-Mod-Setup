# Update, repair, reinstall, rollback, and removal

## Dashboard

Show installed source mode and revision, last check, component states, local modification count, rollback points, optional workflow state, credential availability without values, and unresolved warnings.

When an existing-project scan finds a valid `.hoi4-mod-setup/install.lock.json`,
show that a managed setup is already present and offer **Repair or add
workflows**. The detection is read-only and uses only a bounded safe summary of
the lock: project ID, component IDs, `workflow.3d` state, the non-secret 3D
key-configured bit, and `workflow.super_events` state. It does not treat a
guessed file list as an
installation record.


## ChatGPT source package

Superseded (2026-10-18): the app no longer packages ChatGPT sources or prepares a flattened folder. The packager is the optional `runtime.chatgpt_sources` manifest component, a `.tools` script the user checks on Components and runs. See `docs/33_chatgpt_source_packaging.md`.

## Update

Latest mode resolves the current default branch. New setup and the Update
target selector default to Latest. An existing pinned project can explicitly
select Latest, another full commit, or another release in Update source. The
installed policy and bytes stay unchanged until the new plan is reviewed and
applied; an omitted target from a legacy caller retains the recorded policy.
The same release tag may not move to another commit. Repair and reinstall
cannot accept an Update source override and remain bound to the installed
revision.

Maintenance reanalysis and planning resolve the same explicit target. Changing
it clears the UI's review and plan; Rust rejects a confirmed analysis whose
revision or manifest hash differs from the resolved target, before component
downloads. Selected MCP bootstrap, health, version, integrity, and package-tree
evidence all come from that target manifest, including standalone Live Tools.
The resulting lock records the reviewed package version rather than carrying
forward the predecessor's MCP version. Registry `latest` tags never substitute
for manifest evidence. New command routes still require an app-owned adapter.

The preview shows revision, manifest version, component additions and removals, dependency and command changes, platform changes, wiki coverage changes, conflicts, download size, and backup size.

For each file, compare installed result to current local and old source to new source.
A file that already holds the incoming bytes is a no-op: it raises no conflict and
is not rewritten. A file the user edited whose incoming bytes equal what the last
install wrote has nothing new to merge; the edit is kept (`keep_reviewed_edit`)
without asking again at every update. A merged file the new source no longer
ships is kept silently while it still matches the lock. Repeating an update with
nothing new therefore lists no conflicts. Reuse a prior merge choice only when the conflict signature is unchanged. Generated descriptor, external launcher, and thumbnail artifacts are reviewed independently, including when an existing project already has `descriptor.mod`.

Before an update plan is created, run a new bounded read-only scan. Show the resulting evidence manifest to the user, send only those approved text excerpts to the selected provider (the ChatGPT-authenticated Codex App Server for Codex), and require confirmation of the schema-constrained semantic proposals. The fresh core-session confirmation is passed into the update plan; an old lock record or renderer-supplied record cannot satisfy this update gate. If the scan, provider configuration, analysis, or confirmation fails, no update plan or transaction starts.

Update may add `workflow.super_events` when the target manifest declares the
component, even if the lock records it as `not_selected`. The preview shows the
component, its `core.skills` dependency, the exact managed destination, and the
file/conflict impact. The addition remains provider-neutral and has no
credential or external-action step.

Super Events source files use stable `hoi4ms_*` names for manifest evidence,
but installation and maintenance write the confirmed project-prefix names.
Update removes an unchanged legacy `hoi4ms_*` destination as obsolete; a
modified legacy file remains a reviewable conflict.

## Repair

Repair defaults to the locked revision. Check missing, corrupted, modified, parse-invalid, incomplete generated, MCP health, wiki coverage, and external dependency evidence. Healthy files are explicit no-op operations; missing files can be recreated, and a changed file becomes a reviewable replace/keep decision rather than an inferred repair. Modified files require review.

When a workflow selection refreshes instructions or client configuration,
reconcile it with the existing maintenance operation for that destination.
There is exactly one operation per canonical destination, with a stable ID
across overlapping component batches. Repeated identical dependencies are
no-ops; conflicting ownership, source evidence, case aliases, removals, or
already reviewed choices fail closed. Kept local instructions still require
review, even when their bytes match the last installed lock.

If the lock shows that `workflow.3d` was previously `not_selected`, the repair
screen offers the **3D models workflow** title again. Selecting it expands the locked manifest's declared
dependencies, fetches the same exact locked revision, and plans only the new
component files through the normal transaction. A stored Meshy reference may be
carried forward only for that workflow; a missing key leaves 3D incomplete and
does not block the core setup. If 3D is already selected, the option is shown as
already part of the project rather than duplicating files. If its key is
missing, repair exposes the vault-only key field without writing the value to
the project.

If the lock shows `workflow.super_events` as `not_selected`, Repair shows the
**Super Events workflow** title only after
resolving the manifest at the locked immutable revision. Repair may add the
component and its `core.skills` dependency when that revision declares it. If
the locked source does not declare it, Repair remains unchanged and directs the
user to Update; it must not read a newer manifest as a repair source. An already
selected Super Events workflow is checked for missing, modified, or corrupted
managed files without duplicating its tree, and its missing or incomplete state
never blocks core readiness.

## Reinstall

Re-fetch selected components at the same or chosen revision. Preserve local modifications and safe merge decisions through the normal conflict engine.

## Rollback

Select a rollback record and preview source revision, restored or removed files, structured configuration, Git impact, optional states, and lock state. The current implementation performs a journaled, root-bound reversal using the transaction's verified backups and predecessor lock. Before restoration it creates a separate rollback transaction journal, links it from the parent journal, and backs up the live post-transaction bytes so the reversal has its own durable audit boundary. It records the reversal and refuses to remove explicit skips, merged content, or later user edits. When that child journal completes, the Ready screen exposes a confirmed “Restore rolled-back state” action that reuses the same guarded rollback path to restore the managed files and lock state recorded before rollback. It does not recreate Git or other external side effects; those remain an explicit review boundary.

## Removal

Use reverse dependencies. Default to deleting unmodified managed files, removing managed structured contributions, preserving modified or merged files, preserving local additions, keeping the mod's own files (descriptors, launcher descriptor, thumbnail, README, and documents added through the app, which leave management but stay on disk), keeping global tools, and removing only project credential references. Deleting the OS credential is a separate explicit vault action and requires the opaque Meshy reference; managed removal never performs it implicitly. The final removal report records transaction completion without claiming that the now-unconfigured project is AI-ready; the lock remains available for audit and rollback.

## Optional workflow maintenance

### 3D

Configure key, rerun preflight, rerun MCP health, reinstall repository files, rerun bootstrap, inspect dependency evidence, or remove the workflow.

### Super Events

Inspect the manifest-bound skill tree, restore missing or corrupted unmodified
files, and review local changes through the normal conflict engine. There is no
credential or external health action. A later addition follows the Update or
same-locked-revision Repair rule above. Lock refresh preserves the workflow
state and its managed file ownership; it does not add a credential reference.

## Lock refresh

Every successful maintenance transaction writes source revision, component states, per-file hashes, preserved ownership and skipped files, preserved choices, local modification records, and a new rollback record. The predecessor lock is backed up and restored if the maintenance transaction is rolled back. Configured remotes require explicit final dry-run approval; push and online repository creation remain outside the transaction.

After a maintenance plan is prepared, the interface opens the normal dry-run review immediately, or conflict review when a choice is still required. Repair, reinstall, update, and removal never leave a completed plan hidden on the maintenance action screen.

## Signed-out or disconnected operations

Recovery, rollback, backup inspection, and managed removal remain available while signed out or disconnected. Repair and reinstall use the validated locked analysis after the selected provider is configured; update additionally requires the fresh reanalysis described above. An already approved interrupted transaction can resume without repeating provider analysis when its plan and hashes still validate.
