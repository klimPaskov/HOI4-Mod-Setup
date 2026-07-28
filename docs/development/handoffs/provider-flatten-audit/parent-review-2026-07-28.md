# Parent follow-up review — provider, flatten, transaction, and release surfaces

Date: 2026-07-28

This follow-up records changes made after the bounded auditor snapshots in this
directory. It is not a release sign-off.

## Verified since the snapshots

- Codex-only ChatGPT source flattening remains optional and last in the setup.
  The output includes adapted `AGENTS.md`, generated `README.md`, all selected
  skills as `<skill>.md`, selected subagents, and explicitly named extras. The
  core rechecks bounds, links, Unicode/case collisions, UTF-8, secret-shaped
  paths/content, and reviewed conflict decisions at the mutation boundary.
- Provider selection remains provider-neutral with Codex default, explicit
  hosted endpoints and OS-vault keys, loopback-only local endpoints, and a
  shared schema-validated analysis response. The provider, model, endpoint
  fingerprint, and optimization profile are bound to confirmed analysis and
  maintenance plans without serializing secrets or account metadata.
- Journal operations now carry source path/size, reviewed resolution, staged
  SHA-256, and separate live after-state fields. Success-lock bytes and
  predecessor-lock state are hash-bound for finalization, ordinary rollback,
  inverse rollback, and pre-apply resume. Managed removal has a green
  all-skip test and clears all optional workflow credential references from the
  resulting lock while leaving the OS vault untouched.
- Native release metadata checks now bind a release artifact to checked-out
  `HEAD`, `GITHUB_SHA`, and the tag target. A local unsigned package remains
  inspection evidence only.

## Remaining gates

- The current source manifest lacks immutable wrapper/interpreter/runtime
  identity evidence for the Windows MCP route; it stays `planned_unavailable`.
- Flattening uses no-follow leaf and ancestor checks with bounded enumeration,
  but native handle-relative ancestor containment and coordinated Windows
  junction-race evidence are not implemented.
- Native disk-full, file-lock, permission-loss, journal-write, cancellation,
  and network-failure adapters remain release-gate work.
- macOS native packaging/E2E, signing/notarization, clean-machine evidence,
  license selection, upstream manifest publication, and a public GitHub source
  release remain blocked. No public repository or installer is claimed.

## Current automated result

- Rust all-feature suite: 162 passed, 0 failed.
- Frontend unit suite: 19 passed, 0 failed.
- Typecheck, lint, repository/template validation, secret scan, formatting,
  clippy, release script syntax checks, and `git diff --check`: passed in the
  current worktree.
