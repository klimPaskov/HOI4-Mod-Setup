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

Inject a secret only into an allowlisted process environment for the lifetime of that process. Redact known values and credential-shaped output before storage or display.

Version 1 has one supported secret route: the Meshy value lives in the OS
credential vault under an opaque reference and may be injected only as
`MESHY_API_KEY`. Reject other secret environment declarations and reject
credential-shaped keys or values before serializing plans, locks, journals,
diagnostics, or generated artifacts.
The typed plan and generated project state may carry only the opaque
`credential://` reference returned by the OS vault adapter, and only for the
manifest-declared `MESHY_API_KEY`; validate the reference shape during
migration and never accept a renderer-supplied secret or arbitrary provider.
The explicit delete route accepts only the platform's generated Meshy UUID
reference and clears the in-memory reference after the vault operation. A
managed component removal never deletes an OS credential implicitly.

## Filesystem

- Normalize and contain every path.
- Reject traversal, absolute destination, reserved names, case collisions, and invalid encodings.
- Enforce total path, segment, and depth limits before filesystem access.
- Defend against symlink and junction swaps between validation and apply.
- Use the shared `is_link_metadata` boundary for filesystem metadata; on Windows it treats reparse points/junctions as links, not only `is_symlink()` results.
- Use safe archive extraction with file count, size, ratio, depth, and path limits.
- Do not follow project links outside approved roots.
- Keep backup and staging permissions restrictive.

## Processes

- Use executable plus argument arrays.
- Do not build shell strings from user or manifest input.
- Allowlist executable identity and working roots.
- Open Codex login URLs only through the HTTPS-validated system-browser command using fixed OS-owned opener paths (`explorer.exe` on Windows or `/usr/bin/open` on macOS); never navigate an arbitrary renderer URL through a shell or PATH shim.
- Do not accept PATH script shims as the Codex executable route; use one
  canonical core-owned executable identity for login, analysis, and Open in Codex.
- Preview environment variable names, never values.
- Bound runtime, output, network, and expected writes.
- Do not elevate core setup.
- Treat tool health output as untrusted and redact it.
- Git executable discovery rejects linked PATH entries, and Git initialization or
  rollback rejects linked or junctioned `.git` metadata before invoking Git or
  removing metadata.
- Readiness must parse installed skill frontmatter and subagent TOML, require
  explicit `fork_context=false`, reject link-containing agent trees, and avoid
  claiming the manifest-declared MCP wrapper is healthy when its PATH entry is
  a link or junction.
- MCP readiness must bind the target to the locked manifest/config and use a
  canonical `cmd.exe` plus wrapper path with a cleared, non-secret environment;
  the bounded probe may initialize and list tool metadata but must never call
  an MCP tool or serialize raw protocol output.
- Windows JSONL wrapper shutdown must terminate the reviewed process tree with
  the canonical system `taskkill.exe` route so a child Node server cannot
  survive a health timeout or protocol failure.
- The shared `ProcessSpec` timeout path uses the same canonical Windows
  process-tree termination for credential-bearing external checks; direct
  child kill is only the bounded fallback when the system tool is unavailable.
- The 3D route may inject `MESHY_API_KEY` only after the installed manifest is re-resolved at the lock revision, the bootstrap target is a hash- and size-verified project file, and the Python executable is canonicalized from the approved PATH. A missing opaque reference must fail without starting the process; macOS must report the current Windows-only route as unsupported.
- A 3D health result is cached only as `ready` or `incomplete`, keyed by the canonical project root and a fingerprint of the locked workflow revision, manifest hash, and installed workflow file hashes. The cache stores no credential, command output, or provider response; a lock or Meshy-vault change or process restart invalidates the result and requires a new explicit health run.

## Source and update trust

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
- rollback data loss
- Git hook and config edge cases
- updater metadata tampering
- support bundle redaction
- scoped Meshy injection into an approved child with secret-free stdout/stderr

## Update this skill when

Update this skill when credential storage, redaction, path containment, archive rules, process policy, source trust, updater trust, Git safety, GitHub Actions permissions, or security test expectations change.

## ChatGPT authentication rules

Codex App Server owns ChatGPT OAuth, token persistence, and refresh. The app uses managed browser login and device code only. It never reads Codex auth storage, implements an API-key fallback, accepts externally managed tokens, or persists full account identity, plan, usage, or rate-limit data. Treat approved analysis input as a disclosure surface and test redaction, root boundaries, and support bundles.

Existing-project Codex evidence must be bound to the current core-owned
read-only scan. Never trust renderer-created excerpts solely because their
hashes are internally consistent.
