# Update, repair, reinstall, rollback, and removal

## Dashboard

Show installed source mode and revision, last check, component states, local modification count, rollback points, optional workflow state, credential availability without values, and unresolved warnings.

## Update

Latest mode resolves the current default branch. Pinned mode stays fixed until the user selects another commit or release.

The preview shows revision, manifest version, component additions and removals, dependency and command changes, platform changes, wiki coverage changes, conflicts, download size, and backup size.

For each file, compare installed result to current local and old source to new source. Reuse a prior merge choice only when the conflict signature is unchanged. Generated descriptor, external launcher, and thumbnail artifacts are reviewed independently, including when an existing project already has `descriptor.mod`.

Before an update plan is created, run a new bounded read-only scan. Show the resulting evidence manifest to the user, send only those approved text excerpts to the ChatGPT-authenticated Codex App Server, and require confirmation of the schema-constrained semantic proposals. The fresh core-session confirmation is passed into the update plan; an old lock record or renderer-supplied record cannot satisfy this update gate. If the scan, authentication, analysis, or confirmation fails, no update plan or transaction starts.

## Repair

Repair defaults to the locked revision. Check missing, corrupted, modified, parse-invalid, incomplete generated, MCP health, wiki coverage, and external dependency evidence. Healthy files are explicit no-op operations; missing files can be recreated, and a changed file becomes a reviewable replace/keep decision rather than an inferred repair. Modified files require review.

## Reinstall

Re-fetch selected components at the same or chosen revision. Preserve local modifications and safe merge decisions through the normal conflict engine.

## Rollback

Select a rollback record and preview source revision, restored or removed files, structured configuration, Git impact, optional states, and lock state. The current implementation performs a journaled, root-bound reversal using the transaction's verified backups and predecessor lock. It records the reversal and refuses to remove explicit skips, merged content, or later user edits. A separate rollback-as-new-transaction backup is not yet implemented and remains a release blocker for the broader rollback contract.

## Removal

Use reverse dependencies. Default to deleting unmodified managed files, removing managed structured contributions, preserving modified or merged files, preserving local additions, keeping global tools, and removing only project credential references. Deleting the OS credential is a separate explicit vault action and requires the opaque Meshy reference; managed removal never performs it implicitly. The final removal report records transaction completion without claiming that the now-unconfigured project is Codex-ready; the lock remains available for audit and rollback.

## Optional workflow maintenance

### 3D

Configure key, rerun preflight, rerun MCP health, reinstall repository files, rerun bootstrap, inspect dependency evidence, or remove the workflow.

### LoRA and ComfyUI

Version 1 allows changing or clearing the interest preference only.

## Lock refresh

Every successful maintenance transaction writes source revision, component states, per-file hashes, preserved ownership and skipped files, preserved choices, local modification records, and a new rollback record. The predecessor lock is backed up and restored if the maintenance transaction is rolled back. Configured remotes require explicit final dry-run approval; push and online repository creation remain outside the transaction.

## Signed-out operations

Recovery, rollback, backup inspection, and managed removal remain available while signed out. Repair and reinstall use the validated locked analysis after ChatGPT authentication; update additionally requires the fresh reanalysis described above. An already approved interrupted transaction can resume without repeating Codex analysis when its plan and hashes still validate.
