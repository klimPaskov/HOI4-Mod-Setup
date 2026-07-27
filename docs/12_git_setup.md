# Git setup

## Modes

### Initialize

Verify that no existing Git root controls the folder. Create `.git` only during apply. Merge `.gitignore`, set the selected initial branch, stage only the approved set, and make an initial commit only when selected.

### Preserve

Keep history, branches, remotes, hooks, attributes, submodules, and worktree configuration. Do not switch branch or stage unrelated changes.

### Skip

Add no Git operation to the plan.

## `.gitignore`

Preserve existing order and comments where practical. Add managed rules in a marked block:

```gitignore
# BEGIN HOI4 Mod Setup managed rules
.hoi4-mod-setup/backups/
.hoi4-mod-setup/cache/
.tools/3d_pipeline/vendor/
# END HOI4 Mod Setup managed rules
```

Secrets never belong in the project, so ignore rules are defense in depth.

## Branch and commit

Default new repositories to `main` while allowing another valid name. Do not rename an existing branch without a separate action.

Preview initial commit contents. The project lock should normally be committed because it defines reproducibility and contains no secret. Transaction backups and caches remain ignored.

## Remote

Allow optional remote name and URL. Validate and preview the exact action. Do not authenticate, create an online repository, or push during normal setup.

## Push policy

Never push automatically. A future publish action must show remote, branch, commits, force state, and credentials method. Force push is outside the version 1 setup flow.

## Rollback

Remove a transaction-created `.git` only when no later user commit exists. Never delete an existing repository. Restore pre-setup files through the normal backup.

## Dirty tree

A dirty tree is a warning rather than an automatic block. Recommend a commit or backup and list touched files that already have Git modifications.

## Readiness

Report repository present or intentionally skipped, branch, clean or dirty, remote selected or not, initial commit result, ignored managed files, and tracked secret-like paths.

## Distinction from the application source repository

The rules above describe Git actions that HOI4 Mod Setup may perform inside a user's mod project.

Development of HOI4 Mod Setup itself follows `docs/26_open_source_github_workflow.md`, `CONTRIBUTING.md`, and `RELEASING.md`. The application source repository uses protected `main`, pull requests, CI, CODEOWNERS, dependency updates, and tag-based releases. Those maintainer rules must never be applied automatically to a user's mod project.
