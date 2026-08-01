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
Server; hosted non-Codex adapters use an explicit user endpoint and a
provider-keyed OS-vault credential; local adapters use loopback HTTP. Hosted
endpoints require HTTPS, contain no userinfo, use no redirects, and have
bounded response bodies. Local endpoints cannot leave loopback. Provider
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

## External link security

Application-generated browser links are not provider or project content. The
version 1 Ready screen may expose only the fixed HTTPS URL
`https://github.com/klimPaskov/comfyui-hoi4-portraits`. Validate the exact
scheme, host, owner, and repository path before invoking the typed
system-browser action. Do not construct it from a provider response, manifest,
scan result, project file, or user text; do not pass it through a shell. The
link is informational and cannot satisfy installation, health, or readiness.

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

Secret values do not enter logs through debug formatting, are not copied automatically, do not enter crash reports, are not stored or hashed, are not shown in previews, and are injected only into the approved request header or verified Meshy child process. State-bearing renderer calls blank the Meshy password draft before Tauri IPC; planning receives only the vault-backed opaque reference. The optional flattened Chat export rejects secret-shaped paths and content before staging. `workflow.super_events` has no credential or environment requirement and never creates a vault reference; its unselected state must not add Super Events-specific `AGENTS.md` guidance.

## Command execution

Represent commands as executable plus argument array. Do not build a shell string for ordinary execution. A checked repository wrapper is an executable artifact with a verified hash. A wrapper route is executable only when the manifest also supplies size and SHA-256 evidence for each core-resolved interpreter and runtime dependency; verify those identities immediately before spawn. Display arguments in a safely escaped form with secrets removed.

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

Structured logs contain transaction, stage, component, operation, approved relative path, hashes, duration, status, and sanitized error. They exclude secrets, raw environments, full user documents, and provider responses that may contain credentials.

## Privacy

No telemetry in version 1. Update checks contact only the selected source after a project is opened or configured. Recent projects and scan findings stay local.

## Security tests

Traversal, symlink races, Windows junction escape, case collision, reserved names, huge files, malformed TOML, command injection, environment injection, secret redaction, crash reporting, interrupted apply, compromised manifest fixtures, and hostile local Git configuration rejected before process start.

## Provider input disclosure

Every existing-project provider request has a visible input manifest. The user can remove files or excerpts before transmission. Exclude binaries, `.git/`, credential stores, environment files, secrets, provider caches, large generated assets, and any path outside approved roots. Hash the approved input and validate the response schema before showing proposals.

## Application updates

Application updates use a fixed HTTPS GitHub Release metadata endpoint and a
Tauri updater public key embedded in the app. The private updater key exists
only in the protected release environment and signs final package bytes after
platform signing. A metadata, platform, version, URL, download, or signature
failure leaves the running version unchanged and does not block mod setup.
Installation requires an explicit user action.
