# Public-readiness transaction filesystem handoff

Status: ancestor link redirection is partially mitigated; the P1 transaction
race finding remains open and blocks a public release.

## Implemented in the current candidate

- Added `src-tauri/src/safe_fs.rs` with a project-owned `RootedDir` facade.
- Unix opens components with no-follow `openat` flags and performs reads,
  writes, append, rename, delete, and directory enumeration through retained
  directory descriptors.
- Windows opens and retains each ancestor handle and rejects raw reparse-point
  attributes. Deletion and new-target rename use opened handles; replacing an
  existing file still calls `ReplaceFileW` while its retained parent chain is
  open. Read-only handles permit delete sharing; mutation roots do not.
- Routed atomic JSON writes, file hashing, backup copies, staging writes,
  executable metadata, checkpoint append/replay, managed delete, and staging
  discard through the rooted filesystem layer.
- Added a transaction source regression that rejects ambient `std::fs` reads
  and mutations from production transaction code.
- Added native Windows link/junction checks for final leaves, ancestor swaps,
  and recursive staging removal. The same tests use symlinks on Unix.

## Verification

The Windows suite passed 422 Rust tests with one existing opt-in scan fixture
ignored. The new path tests preserve outside sentinels during static link,
ancestor junction, and staging-tree cases. `cargo clippy` passed before the
last filesystem refinements and must be rerun against the final candidate.

## Remaining P1 work

1. A transaction opens rooted paths per operation rather than retaining one
   project, app-data, transaction, backup, staging, and external-descriptor
   identity through the complete transaction and recovery lifecycle. Persist
   root identities in the journal and reject drift before apply, lock commit,
   and recovery.
2. A regular destination can still be changed after its precondition hash and
   before replacement or deletion. Quarantine the displaced leaf, verify its
   bytes after namespace movement, and persist the quarantine name/hash and
   commit checkpoint so interruptions recover without losing user data.
3. The successful local path-swap tests prove ancestor containment for selected
   cases; they do not cover every transaction stage or concurrent final-leaf
   edit. Add deterministic barriers for app-root, transaction, backup, staging,
   project, launcher-parent, lock, journal, apply, rollback, and staging discard
   swaps, plus crash points around sync and namespace changes.
4. The new native tests ran on Windows only. Run the same matrix on macOS and
   include the supported case-sensitive and case-insensitive filesystem routes
   before closing the finding.

`safe_join`, canonicalization, `symlink_metadata`, and `path_has_link_component`
remain useful for lexical checks and diagnostics. They do not authorize a
mutation. Do not describe the transaction boundary as race-proof until the
remaining root-identity, same-leaf, recovery, and cross-platform gates pass.
