# Update, repair, reinstall, rollback, and removal

## Dashboard

Show installed source mode and revision, last check, component states, local modification count, rollback points, optional workflow state, credential availability without values, and unresolved warnings.

When an existing-project scan finds a valid `.hoi4-mod-setup/install.lock.json`,
show that a managed setup is already present and offer **Repair or add
workflows**. The detection is read-only and uses only a bounded safe summary of
the lock: project ID, component IDs, 3D state, a non-secret 3D key-configured
bit, and portrait-interest state. It does not treat a guessed file list as an
installation record.

## Update

Latest mode resolves the current default branch. Pinned mode stays fixed until the user selects another commit or release.

The preview shows revision, manifest version, component additions and removals, dependency and command changes, platform changes, wiki coverage changes, conflicts, download size, and backup size.

For each file, compare installed result to current local and old source to new source. Reuse a prior merge choice only when the conflict signature is unchanged. Generated descriptor, external launcher, and thumbnail artifacts are reviewed independently, including when an existing project already has `descriptor.mod`.

Before an update plan is created, run a new bounded read-only scan. Show the resulting evidence manifest to the user, send only those approved text excerpts to the selected provider (the ChatGPT-authenticated Codex App Server for Codex), and require confirmation of the schema-constrained semantic proposals. The fresh core-session confirmation is passed into the update plan; an old lock record or renderer-supplied record cannot satisfy this update gate. If the scan, provider configuration, analysis, or confirmation fails, no update plan or transaction starts.

## Repair

Repair defaults to the locked revision. Check missing, corrupted, modified, parse-invalid, incomplete generated, MCP health, wiki coverage, and external dependency evidence. Healthy files are explicit no-op operations; missing files can be recreated, and a changed file becomes a reviewable replace/keep decision rather than an inferred repair. Modified files require review.

If the lock shows that `workflow.3d` was previously `not_selected`, the repair
screen offers the exact question **Do you want to set up the 3D models
workflow?** again. Selecting it expands the locked manifest's declared
dependencies, fetches the same exact locked revision, and plans only the new
component files through the normal transaction. A stored Meshy reference may be
carried forward only for that workflow; a missing key leaves 3D incomplete and
does not block the core setup. If 3D is already selected, the option is shown as
already part of the project rather than duplicating files. If its key is
missing, repair exposes the vault-only key field without writing the value to
the project. The LoRA/ComfyUI choice remains interest-only.

## Reinstall

Re-fetch selected components at the same or chosen revision. Preserve local modifications and safe merge decisions through the normal conflict engine.

## Rollback

Select a rollback record and preview source revision, restored or removed files, structured configuration, Git impact, optional states, and lock state. The current implementation performs a journaled, root-bound reversal using the transaction's verified backups and predecessor lock. Before restoration it creates a separate rollback transaction journal, links it from the parent journal, and backs up the live post-transaction bytes so the reversal has its own durable audit boundary. It records the reversal and refuses to remove explicit skips, merged content, or later user edits. When that child journal completes, the Ready screen exposes a confirmed “Restore rolled-back state” action that reuses the same guarded rollback path to restore the managed files and lock state recorded before rollback. It does not recreate Git or other external side effects; those remain an explicit review boundary.

## Removal

Use reverse dependencies. Default to deleting unmodified managed files, removing managed structured contributions, preserving modified or merged files, preserving local additions, keeping global tools, and removing only project credential references. Deleting the OS credential is a separate explicit vault action and requires the opaque Meshy reference; managed removal never performs it implicitly. The final removal report records transaction completion without claiming that the now-unconfigured project is AI-ready; the lock remains available for audit and rollback.

## Optional workflow maintenance

### 3D

Configure key, rerun preflight, rerun MCP health, reinstall repository files, rerun bootstrap, inspect dependency evidence, or remove the workflow.

### LoRA and ComfyUI

Version 1 allows changing or clearing the interest preference only.

## Lock refresh

Every successful maintenance transaction writes source revision, component states, per-file hashes, preserved ownership and skipped files, preserved choices, local modification records, and a new rollback record. The predecessor lock is backed up and restored if the maintenance transaction is rolled back. Configured remotes require explicit final dry-run approval; push and online repository creation remain outside the transaction.

## Signed-out or disconnected operations

Recovery, rollback, backup inspection, and managed removal remain available while signed out or disconnected. Repair and reinstall use the validated locked analysis after the selected provider is configured; update additionally requires the fresh reanalysis described above. An already approved interrupted transaction can resume without repeating provider analysis when its plan and hashes still validate.
