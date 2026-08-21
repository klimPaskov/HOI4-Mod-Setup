# Security model

## Threats

- path traversal
- malicious symlink or junction
- command injection
- compromised remote content
- secret leakage
- unsafe package installation
- privilege escalation
- overwrite of user work
- parser resource exhaustion
- dependency confusion
- mutable source references

## Trust boundaries

### Local project

Untrusted input. Parsers must be bounded. Project files cannot define commands.

### Remote manifest

Trusted only after exact revision resolution, schema validation, integrity checks, and policy validation. It can request only supported declarative actions.

### Repository scripts

Executable content. Always shown as high-impact external actions with source path and hash. Run only after approval.

### OS credential vault

Trusted secret boundary. Project data receives opaque references only.

### External tools

Git, Node, Python, Blender, Codex, and MCP servers run as separate allowlisted processes.

### AI provider adapters

The renderer calls typed Rust commands only. Codex uses the official local App
Server; known hosted non-Codex adapters use a checked-in, officially verified
HTTPS default and a provider-keyed OS-vault credential; custom adapters use an
explicit user address; local adapters use loopback HTTP. Fixed provider account
links pass a Rust allowlist before the system browser opens. Hosted addresses
require HTTPS, contain no userinfo, use no redirects, and have bounded response
bodies. Local addresses cannot leave loopback. Provider
responses are untrusted text until the common schema and deterministic
evidence checks pass.

## Download security

- HTTPS only
- exact repository and revision
- redirect and host limits
- content-length limits
- SHA-256 before staging
- immutable cache by source and hash
- executable permission only when declared

## Path security

Normalize Unicode and separators, reject absolute managed destinations and parent traversal, resolve links, verify final parent containment, detect case collisions, reject Windows device names and alternate data streams, and block archive-link escapes.

## Portrait provider security

The portrait workflow is optional for generic projects and mandatory only in
Chaos Redux. The project stores provider, route, status, exact upstream
repository, branch, commit, and workflow metadata, but never an API key,
access token, password, cookie, RunPod credential, account identity, or usage
metadata. Cloud MCP registration is a fixed product-reviewed endpoint;
provider authentication remains in the provider's secure flow. Local health
checks accept only HTTP loopback URLs and bounded ComfyUI roots. RunPod URLs
must be HTTPS and are never treated as ready until the page and workflow are
observed.

The local workflow installer downloads only the two authorized current UI
workflow files and the two pinned adaptive-crop custom-node files at the
pinned commit, disables redirects, verifies SHA-256 before staging, refuses
modified existing files, and applies through an exact transaction staging
directory. Model downloads and Python dependency installation remain separate
Hugging-Face-gated actions; `HF_TOKEN` is only a presence hint and is never
read into project state, logs, previews, plans, locks, or React state.

The fixed canonical repository link may still be opened through the typed
system-browser bridge, but it is supplemental guidance and never evidence of
provider readiness.

## ChatGPT, Codex, and provider credential boundary

The application delegates ChatGPT authentication to the official Codex App Server managed flow. It does not own OAuth tokens and does not read Codex credential storage.

Codex rules:

- no OpenAI API key field
- no API-key fallback for core analysis
- no externally managed ChatGPT token mode
- no account email, account ID, plan, usage, or rate limits in project files or locks
- no authentication values in command arguments
- no raw App Server protocol logs in production
- redact browser URLs and device codes from diagnostics
- use local stdio transport and no network listener
- terminate the child process cleanly
- keep semantic analysis read-only

Live account metadata returned by `account/read` is transient UI state.

Non-Codex rules:

- keys are saved only to Windows Credential Manager or macOS Keychain;
- the in-memory reference map is keyed by provider and the key value never
  crosses into React state, project state, plans, locks, logs, or screenshots;
- the endpoint and model are user-selected non-secret configuration;
- a provider switch clears stale analysis and cannot reuse another provider's
  reference; and
- the first provider request is a bounded, schema-validated capability check.

## Secrets

Secret values do not enter logs through debug formatting, are not copied automatically, do not enter crash reports, are not stored or hashed, are not shown in previews, and are injected only into the approved request header or the isolated, hash-verified Meshy bootstrap/route. The bootstrap removes the key before every dependency child; Blender, Git, uv, npm setup, and downloaded setup code never receive it. Blender routes declare no Meshy environment. State-bearing renderer calls blank the Meshy password draft before Tauri IPC; planning receives only the vault-backed opaque reference. The optional flattened Chat export rejects secret-shaped paths and content before staging. `workflow.super_events` has no credential or environment requirement and never creates a vault reference; its unselected state must not add Super Events-specific `AGENTS.md` guidance.

## Command execution

Represent commands as executable plus argument array. Do not build a shell string for ordinary execution. A checked repository wrapper is a verified managed artifact. The HOI4 Agent Tools wrapper is never executed: it locates a current-user npm prefix whose exact full package tree is copied through no-follow reads into a private tree before an OpenJS-signed Node runtime starts the private entry. Meshy never uses a project credential-bearing wrapper: Codex invokes the absolute installed HOI4 Mod Setup executable with one fixed CLI marker. That app-owned boundary revalidates and privately copies the complete lockfile runtime, copies Node before exact native publisher verification, re-hashes both private identities immediately before spawn, clears Node influence variables, and only then passes `MESHY_API_KEY` to the exact private entry. Display arguments in a safely escaped form with secrets removed. Desktop-supervised children do not request an interactive terminal; Windows launches use `CREATE_NO_WINDOW` so the app remains independent of a visible console.

Read-only Git inspection runs with system and global configuration disabled,
optional locks and prompts off, an empty credential helper, inert hook and
attribute paths, no external diff, no replace objects, and file/ext transports
disabled. Before spawning Git, the core parses the bounded local `.git/config`
without includes and rejects executable aliases, hooks, filters, credential or
transport helpers, URL rewrites, and other external-process settings. Submodule
paths come from a bounded direct `.gitmodules` read; scanning does not recurse
through submodule Git processes. This isolation does not make a PATH-discovered
Git executable independently trusted.

## Privilege

Core setup runs as the current user and does not request administrator or root privileges. A dependency action that requires elevation is separately disclosed and approved. Core setup remains usable without it.

## Codex security settings

Incoming values such as `danger-full-access` and `approval_policy = "never"` are security-sensitive. Show them as explicit choices and record the result.

## Supply chain

- exact commit
- per-file SHA-256
- package source and version policy
- visible global installs
- observed external dependency versions and hashes
- future signed manifest support

## Logging

Structured logs contain transaction, stage, component, operation, approved relative path, hashes, duration, status, and sanitized error. They exclude secrets, raw environments, full user documents, and provider responses that may contain credentials. Transaction failure messages are credential-shape redacted, including quoted JSON fields and unquoted key assignments such as `client_secret=...`, and bounded to 2 KiB on a UTF-8 boundary before journal persistence. They are sanitized again when a current or legacy journal is read and defensively redacted before Recovery Details or a transaction/rollback command error displays them.

## Privacy

No telemetry in version 1. Update checks contact only the selected source after a project is opened or configured. Recent projects and scan findings stay local.

## Security tests

Traversal, symlink races, Windows junction escape, case collision, reserved names, huge files, malformed TOML, command injection, environment injection, secret redaction, crash reporting, interrupted apply, compromised manifest fixtures, and hostile local Git configuration rejected before process start. Recovery tests persist quoted, unquoted-assignment, prefixed, and multibyte credential-shaped transaction errors, read an unsanitized legacy equivalent, and assert that neither the journal API nor any transaction error surface exposes the raw value.

## Provider input disclosure

Every existing-project provider request has a visible input manifest. The user can remove files or excerpts before transmission. Exclude binaries, `.git/`, credential stores, environment files, secrets, provider caches, large generated assets, and any path outside approved roots. Hash the approved input and validate the response schema before showing proposals.

## Application updates

Application updates use a fixed HTTPS GitHub Release metadata endpoint and a
Tauri updater public key embedded in the app. The private updater key exists
only in the protected release environment and signs final package bytes after
platform signing. A metadata, platform, version, URL, download, or signature
failure leaves the running version unchanged and does not block mod setup.
When a newer signed version is found on startup, the app begins the verified
download, replacement, and restart automatically. A failure leaves the running
version usable and permits a retry; signature verification cannot be bypassed.
Release curation stream-verifies every final update artifact with the embedded
public key before metadata is published, catching stale or mismatched signing
keys before users can receive an unusable update.

## ChatGPT source package export

The export command accepts only a validated existing project root, a validated
existing external directory, and a fresh file-ID selection returned by Rust.
It re-reads each regular file without following links, checks UTF-8, size,
secret-shaped content, archive-relative names, and collisions, then writes a
new uncompressed ZIP through a create-new temporary file and atomic rename.
The destination is never the project root, an existing archive is never
overwritten, no credentials are read, and a failed export removes only its
temporary output. It does not require an installation lock; eligibility comes
from the bounded source scan, while credentials remain outside the project.
