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

Treat transaction failure messages as untrusted output. Apply the shared
credential-shape redactor and a 2 KiB UTF-8-safe bound before journal
persistence, sanitize again when current or legacy journals are read, and keep
a bounded renderer redactor at the Recovery Details and direct transaction or
rollback error display boundaries. Cover unquoted key assignments such as
`client_secret=...`, quoted JSON-style secret fields, and prefixed provider
values. Tests must persist and read multibyte secret-shaped failures and prove
that the raw values are absent from the journal API and interface.

On Windows, resolve OS-owned verifier and browser executables from native
Windows directory APIs rather than `SystemRoot`. Authenticode checks compare
an exact reviewed certificate simple name, never a subject substring, and
re-hash the candidate across verification.

Version 1 has two distinct secret routes. The Meshy value lives in the OS
credential vault under an opaque reference and may be injected only as
`MESHY_API_KEY`. Reject other secret environment declarations and reject
credential-shaped keys or values before serializing plans, locks, journals,
diagnostics, or generated artifacts.
The verified 3D skill's exact documentation token
`msy_your_actual_key_here` is non-secret and may pass serialization checks.
Apply that exception through the shared bounded-token detector so plans,
journals, and flattened exports agree; longer values and every other `msy_`
shape remain blocked. Persist the documentation token unchanged rather than
silently rewriting selected source content.
The typed plan and generated project state may carry only the opaque stable
`credential://meshy_api_key/default` reference returned by the OS vault adapter,
only when `workflow.3d` is selected; validate the exact reference shape during
migration and continue accepting legacy app-generated UUID references. On
startup, read the stable reference; on Windows, boundedly rediscover the newest
valid legacy entry and reuse it in place without copying, rewriting, logging,
or deleting its value. Never accept a renderer-supplied secret or arbitrary
provider. The explicit delete route accepts only the stable or legacy Meshy
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

Known hosted provider account pages and checked-in defaults must come from
official provider documentation. The renderer may request only a fixed account
URL carried by the reviewed profile; Rust must still compare it against the
exact allowlist before invoking the system browser. Do not describe API-key
entry as OAuth or account login.

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
- The automatic 3D bootstrap receives `MESHY_API_KEY` only through
  `ScopedSecretEnvironment` loaded from the OS vault. Copy the hash-verified
  managed bootstrap to a private temporary script, run only the reviewed
  Python executable with `-I` and the reviewed argument array in the bound project root, cap time/output,
  redact evidence, and remove the private copy. Missing credentials or tools
  produce optional `incomplete`; changed script/action evidence fails closed.
  The script removes `MESHY_API_KEY` before every dependency child; Blender
  routes declare an empty credential environment. Never write the key to
  project config, plan, lock, journal, or process preview.
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
  Bind the child working directory to the retained `.git` handle: use `fchdir`
  immediately before exec on Unix and the non-delete-sharing handle plus
  canonical path on Windows. Do not pass a parent process `/proc/self/fd` or
  `/dev/fd` pathname to the child.
  Discover submodule paths by reading the bounded `.gitmodules` file directly;
  never start recursive submodule processes during a scan.
- Account-bearing Codex and secret-bearing optional workflow processes require
  a canonical, unlinked executable whose platform signature publisher matches
  the reviewed vendor immediately before spawn. A matching filename or stable
  hash alone does not establish provenance.
- The isolated Windows launcher environment restores only the non-secret
  drive/home routing variables needed by native applications (`SystemDrive`,
  `HOMEDRIVE`, and `HOMEPATH`); never replace their values with literal
  placeholders or add credential-bearing parent variables.
- Open in Codex may reuse only process-local readiness evidence bound to the
  canonical project root and exact current lock hash. Clear that evidence on
  app-controlled logout. A cached Codex executable remains usable only after
  its link boundary, OpenAI publisher, and SHA-256 are rechecked.
- On Unix, supervised children start in their own process group and timeout,
  restart, close, and drop paths terminate that entire group so descendants do
  not retain scoped environment or account access.
- Reviewed online Git actions bind the canonical root, named branch, clean
  worktree, exact HEAD, actual push URL, and GitHub CLI hash. Recheck all of
  them immediately before execution. Reject configured URL rewrites,
  `core.sshCommand`, `core.gitProxy`, `core.hooksPath`, submodules, active
  hooks, detached branches, and dirty trees. Public repository creation and
  the first push are separate approvals, and the result record is secret-free.
- A reviewed push supports only the exact approved HTTPS GitHub URL. Run it
  with repository-local configuration disabled through the effective
  `GIT_CONFIG` null-file override and fixed command-line transport settings;
  keep a behavioral child-process regression proving a local sentinel cannot
  be observed. Do not substitute the unsupported `GIT_CONFIG_LOCAL` variable.
- Initial app-owned Git staging reads every managed file through the retained
  no-follow project handle, requires the transaction-derived expected size and
  SHA-256, and sends those exact bytes to `git hash-object --stdin` before the
  index is updated. Git must never reopen the project pathname for this step.
- Readiness must parse installed skill frontmatter and subagent TOML, require
  explicit `fork_context=false`, reject link-containing agent trees, and avoid
  claiming the manifest-declared MCP wrapper is healthy when its PATH entry is
  a link, junction, or lacks immutable manifest identity evidence.
- MCP readiness must bind the target to the locked manifest/config, require
  registry integrity, the canonical full package-tree hash/count, runtime-entry
  hash/size, required tools, and an OpenJS-signed Node runtime. The wrapper is
  used only to locate the prefix and is never executed; Node's observed hash is
  rechecked at spawn with a cleared, non-secret environment. Read every
  package file through no-follow containment, require the canonical full-tree
  identity, materialize the verified bytes into a private tree, and execute
  only that private runtime entry;
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
- The installed Meshy MCP runtime uses the source-published exact npm lock and
  complete runtime-tree identity. Its Codex route must target the absolute
  installed HOI4 Mod Setup executable with only the fixed Meshy CLI marker;
  project wrappers, project launchers, and PATH Python must never receive the
  key. Each credential-bearing start rejects links and tree drift, copies and
  re-hashes the verified runtime, copies Node before exact simple-name
  publisher verification through the native Windows verifier, rechecks the
  private Node hash immediately before spawn, clears Node influence variables,
  and passes the key only to that exact private Node entry.
- A 3D health result is cached only as `ready` or `incomplete`, keyed by the canonical project root and a fingerprint of the locked workflow revision, manifest hash, and installed workflow file hashes. The cache stores no credential, command output, or provider response; a lock or Meshy-vault change or process restart invalidates the result and requires a new explicit health run.

## Source and update trust

- Application updates use one fixed HTTPS GitHub Release endpoint and the
  committed Tauri updater public key. The private key is release-environment
  only and signs final platform bytes after platform publisher signing.
- Background checks are non-blocking. After a newer signed version is
  surfaced, verified replacement and restart begin automatically; any
  metadata, download, signature, or replacement failure preserves the running
  app and exposes Retry.
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
- exact documented Meshy placeholder accepted while real and extended `msy_`
  values remain rejected from every persisted JSON boundary
- transaction failure redaction and 2 KiB UTF-8-safe bounds at journal
  persistence, legacy journal read, Recovery Details, and direct
  transaction/rollback error display boundaries, including quoted fields and
  unquoted secret assignments
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
