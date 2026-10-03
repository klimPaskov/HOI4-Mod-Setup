# Public-readiness transaction filesystem handoff

Status: ancestor link redirection is partially mitigated and project-root
identity is now persisted and checked; the P1 transaction race finding remains
open and blocks a public release.

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
- Added platform-scoped directory identity tokens. Plans bind existing project
  roots or the existing parent of a new root; journals retain those identities
  and the identity of a created root. Recovery rejects identity drift, and
  apply rechecks the root before each operation.
- Existing-project scans record the opened root identity, and semantic review
  and plan construction reject a different directory at the same path.
- Windows `RootedDir` acquisition walks lexical components without
  canonicalizing through a possible junction and uses versioned 128-bit
  `FILE_ID_INFO` identities. Plan and journal schemas are version 1.1; legacy
  identity-less journals remain inspect-only.
- New project roots are created through the reviewed parent handle. An
  interrupted create before the root identity checkpoint is treated as
  ambiguous; rollback retains the empty root for inspection.
- Added tests that replace an existing project root and a reviewed parent at
  the same path, plus the crash-safe behavior for ambiguous new roots.
- Forward backup, apply, final verification, lock construction, and lock commit
  retain one reviewed project capability. Managed rollback file/lock operations
  and finalization's project-file and success-lock checks retain it as well.

## Verification

On 2026-10-03 the Windows all-feature suite passed 432 Rust tests with one
existing opt-in scan fixture ignored. The path tests preserve outside
sentinels during static link, ancestor junction, and staging-tree cases.
`cargo clippy` must be rerun against the final candidate.

## Remaining P1 work

1. Application-data, external destinations, created-root cleanup, Git,
   external actions, and readiness still have path-based boundaries. Retain
   and persist their identities for the complete transaction and recovery
   lifecycle. Checks before and after a path-based operation do not prevent
   a swap-away-and-back race.
2. Implemented on Windows for forward apply, managed delete, managed rollback restore and removal, the success-lock commit, and the rollback lock restore; see `public-readiness-transaction-quarantine.md`. The displaced leaf moves to a journaled same-directory quarantine through the retained parent handle, its bytes are verified before an exclusive placement, and every crash point recovers without losing user bytes. The Linux and macOS exclusive-rename routes are not yet compiled or run natively, so this item stays open until the native matrix in item 4 passes.
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
