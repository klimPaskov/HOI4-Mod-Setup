---
name: hoi4-mod-setup-security
description: Use for credential handling, filesystem containment, archive extraction, command execution, logging, source trust, Git safety, support bundles, updater security, or security reviews.
---

# Security workflow

## Required sources

Read:

- `AGENTS.md`
- `SECURITY.md`
- `docs/13_security_model.md`
- `docs/04_remote_repository_manifest.md`
- `docs/14_transaction_rollback.md`
- relevant platform and process adapter code

## Security boundaries

The application handles:

- untrusted project files
- untrusted remote metadata until verified
- user-selected paths
- optional credentials
- external tools
- Git repositories
- transaction backups
- logs and support bundles

Treat every boundary as hostile until validated.

## Credentials

Store secrets in Windows Credential Manager or macOS Keychain. Store only an opaque reference in application state.

Never serialize a secret to project files, lock files, plans, manifests, logs, crash reports, screenshots, command previews, test fixtures, or Git.

Password drafts are renderer-memory-only and are cleared before the vault IPC
promise begins, on both success and failure. State-bearing IPC sanitizers still
blank the Meshy field as a second boundary.

Inject a secret only into an allowlisted process environment for the lifetime of that process. Redact known values and credential-shaped output before storage or display.

On Windows, resolve OS-owned verifier and browser executables from native
Windows directory APIs rather than `SystemRoot`. Authenticode checks compare
an exact reviewed certificate simple name, never a subject substring, and
re-hash the candidate across verification.

Version 1 has two distinct secret routes. The Meshy value lives in the OS
credential vault under an opaque reference and may be injected only as
`MESHY_API_KEY`. Reject other secret environment declarations and reject
credential-shaped keys or values before serializing plans, locks, journals,
diagnostics, or generated artifacts.
The typed plan and generated project state may carry only the opaque
`credential://meshy_api_key/<uuid>` reference returned by the OS vault adapter,
only when `workflow.3d` is selected; validate the exact reference shape during
migration and never accept a renderer-supplied secret or arbitrary provider.
The explicit delete route accepts only the platform's generated Meshy UUID
reference and clears the in-memory reference after the vault operation. A
  managed component removal never deletes an OS credential implicitly.
Renderer state calls must blank the Meshy password draft before Tauri IPC; only
the vault-backed opaque reference may enter planning state.

Non-Codex AI provider keys use a provider-keyed opaque vault reference. The
reference may carry only a non-secret provider scope; the core rejects a
reference scoped to another provider and rejects legacy unscoped references at
use time until the user reconnects. Codex keys are never collected: ChatGPT
authentication and token persistence belong to Codex App Server. Local models
may use a loopback endpoint without a key. Hosted or
custom endpoints must pass the core URL policy (HTTPS, no userinfo, query, or
fragment; loopback HTTP only for the local profile), and requests/responses are
bounded and schema-validated before any proposal is accepted.

## Filesystem

- Normalize and contain every path.
- Reject traversal, absolute destination, reserved names, case collisions, and invalid encodings.
- Enforce total path, segment, and depth limits before filesystem access.
- Defend against symlink and junction swaps between validation and apply.
- Flattened-source reads walk Unix ancestors through no-follow directory handles
  and verify the opened Windows file handle's final path remains under the
  canonical root. Keep adversarial swap tests as release evidence before
  describing the boundary as race-proof.
- Use the shared `is_link_metadata` boundary for filesystem metadata; on Windows it treats reparse points/junctions as links, not only `is_symlink()` results.
- On macOS, treat the OS-owned `/etc`, `/tmp`, and `/var` aliases into
  `/private` as path-prefix aliases, not project links; still reject every
  link or reparse point below the selected root. Canonicalize the selected
  root before containment checks so temporary and user paths work on both
  supported platforms.
- Use safe archive extraction with file count, size, ratio, depth, and path limits.
- Do not follow project links outside approved roots.
- Keep backup and staging permissions restrictive.

## Processes

- Use executable plus argument arrays.
- Do not build shell strings from user or manifest input.
- Allowlist executable identity and working roots.
- Open returned Codex login URLs and reviewed source or Ready-screen external
  links only through the HTTPS/allowlist-validated system-browser command using
  fixed OS-owned opener paths (`explorer.exe` on Windows or `/usr/bin/open` on
  macOS); never navigate an arbitrary renderer URL through a shell or PATH
  shim. The external-link command accepts only its fixed reviewed URL set.
- Do not treat a regular file found on `PATH` as independent executable trust.
  Every process identity must be hash-checked immediately before spawn; a
  manifest route without immutable executable evidence is unavailable. Codex
  still requires a core-owned or platform-verified executable identity before
  a production release can claim the login, analysis, or opener boundary.
- Preview environment variable names, never values.
- Provider/model, endpoint, network access, and environment names may appear in
  dry-run evidence; secret values and account metadata may not.
- Bound runtime, output, network, and expected writes.
- Do not elevate core setup.
- Treat tool health output as untrusted and redact it.
- Git executable discovery rejects linked PATH entries, and Git initialization or
  rollback rejects linked or junctioned `.git` metadata before invoking Git or
  removing metadata.
- Read-only Git inspection disables system/global configuration, optional
  locks, prompts, credential helpers, hooks, external diff/attribute behavior,
  replace objects, and file/ext transports. Before any Git child starts, parse
  the bounded local `.git/config` without includes and reject executable,
  transport, credential, URL-rewrite, filter, alias, or include settings.
  Discover submodule paths by reading the bounded `.gitmodules` file directly;
  never start recursive submodule processes during a scan.
- Account-bearing Codex and secret-bearing optional workflow processes require
  a canonical, unlinked executable whose platform signature publisher matches
  the reviewed vendor immediately before spawn. A matching filename or stable
  hash alone does not establish provenance.
- On Unix, supervised children start in their own process group and timeout,
  restart, close, and drop paths terminate that entire group so descendants do
  not retain scoped environment or account access.
- Reviewed online Git actions bind the canonical root, named branch, clean
  worktree, exact HEAD, actual push URL, and GitHub CLI hash. Recheck all of
  them immediately before execution. Reject configured URL rewrites,
  `core.sshCommand`, `core.gitProxy`, `core.hooksPath`, submodules, active
  hooks, detached branches, and dirty trees. Public repository creation and
  the first push are separate approvals, and the result record is secret-free.
- Readiness must parse installed skill frontmatter and subagent TOML, require
  explicit `fork_context=false`, reject link-containing agent trees, and avoid
  claiming the manifest-declared MCP wrapper is healthy when its PATH entry is
  a link, junction, or lacks immutable manifest identity evidence.
- MCP readiness must bind the target to the locked manifest/config, require
  manifest SHA-256 and size verification for the wrapper, command interpreter,
  and runtime before resolving PATH, and use a canonical `cmd.exe` plus wrapper
  path with a cleared, non-secret environment;
  the bounded probe may initialize and list tool metadata but must never call
  an MCP tool or serialize raw protocol output. Missing identity is
  `planned_unavailable`, not permission to execute a same-named command.
- Windows JSONL wrapper shutdown must terminate the reviewed process tree with
  the canonical system `taskkill.exe` route so a child Node server cannot
  survive a health timeout or protocol failure.
- The shared `ProcessSpec` timeout path uses the same canonical Windows
  process-tree termination for credential-bearing external checks; direct
  child kill is only the bounded fallback when the system tool is unavailable.
- The 3D route may inject `MESHY_API_KEY` only after the installed manifest is re-resolved at the lock revision, the bootstrap target is read through the no-follow core reader, and the Python executable is hash-checked immediately before spawn. Execute a private, hash-verified copy of the bootstrap, remove it after the supervised run, and fail without starting when cleanup or identity verification fails. A missing opaque reference must fail without starting the process; macOS must report the current Windows-only route as unsupported.
- A 3D health result is cached only as `ready` or `incomplete`, keyed by the canonical project root and a fingerprint of the locked workflow revision, manifest hash, and installed workflow file hashes. The cache stores no credential, command output, or provider response; a lock or Meshy-vault change or process restart invalidates the result and requires a new explicit health run.

## Source and update trust

- Application updates use one fixed HTTPS GitHub Release endpoint and the
  committed Tauri updater public key. The private key is release-environment
  only and signs final platform bytes after platform publisher signing.
- Background checks are non-blocking; installation requires explicit user
  action, and any metadata, download, or signature failure preserves the
  running app.
- Release curation stream-verifies each final updater artifact with the
  embedded public key so a stale or mismatched private key blocks publication.

- Resolve exact revisions.
- Verify SHA-256.
- Keep manifest and files on one revision.
- Reject unsupported manifest majors.
- Record redirects and final source identity.
- Require explicit evidence for external dependencies and platform commands.
- Separate application update trust from workflow component update trust.

## GitHub Actions

Use read-only default permissions. Grant write permission only to a release job that needs it. Do not expose secrets to fork pull requests. Use protected environments for release signing and notarization. Pin production third-party actions after review.

## Security tests

- traversal and encoded traversal
- symlink and junction race
- archive bomb and path escape
- command and environment injection
- secret in stdout, stderr, panic, and crash state
- malicious manifest and redirect
- compromised cache
- verified-cache path replacement between inspection and return
- unsigned or wrong-publisher executable substitution
- Unix descendant survival after supervised-process shutdown
- rollback data loss
- Git hook and config edge cases
- read-only Git environment isolation, hostile local-config rejection before
  spawn, and direct bounded `.gitmodules` parsing
- online Git approval expiry, changed HEAD/remote, GitHub CLI replacement, and
  no-action-before-approval
- updater metadata tampering
- support bundle redaction
- scoped Meshy injection into an approved child with secret-free stdout/stderr
- provider-keyed AI credential isolation, endpoint validation, bounded response
  handling, and no-secret flatten output

## Update this skill when

Update this skill when credential storage, redaction, path containment, archive rules, process policy, source trust, updater trust, Git safety, GitHub Actions permissions, or security test expectations change.

## AI provider and ChatGPT authentication rules

Codex App Server owns ChatGPT OAuth, token persistence, and refresh. The app uses managed browser login and device code only. It never reads Codex auth storage, implements an API-key fallback, accepts externally managed tokens, or persists full account identity, plan, usage, or rate-limit data. Treat approved analysis input as a disclosure surface and test redaction, root boundaries, and support bundles.

Existing-project provider evidence must be bound to the current core-owned
read-only scan. The approval store accepts only the exact core-derived
path/hash pair; renderer-authored excerpts, even with self-consistent hashes,
are rejected. Flattened reads remain link-aware and leaf-no-follow; do not call
them race-proof until handle-relative ancestor traversal and adversarial swap
tests exist on both supported platforms.
