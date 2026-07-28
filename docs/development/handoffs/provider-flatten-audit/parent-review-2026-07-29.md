# Parent review: flatten containment follow-up

The flatten reader was hardened after the 2026-07-28 audit snapshot.

- Unix/macOS opens the canonical project root and walks each relative
  directory component with `openat`, `O_NOFOLLOW`, `O_DIRECTORY`, and
  close-on-exec flags before opening the final file.
- Windows opens the leaf with `FILE_FLAG_OPEN_REPARSE_POINT` and checks
  `GetFinalPathNameByHandleW` before and after the read against the canonical
  root. A path-based ancestor substitution therefore fails closed when the
  opened handle resolves outside the root.
- 3D and private health-check reads use the same root-bound reader.
- A cross-platform regular-file regression test and the new `flatten` fuzz
  target compile on the Windows host.

Evidence run:

```text
cargo fmt --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml --all-features
cargo check --manifest-path fuzz/Cargo.toml --bins
```

Remaining evidence gap: coordinated concurrent macOS symlink and Windows
junction/reparse swaps have not been executed on both native runners. The
implementation remains link-aware and fail-closed, but no release claim calls
it race-proof until those adversarial tests pass.
