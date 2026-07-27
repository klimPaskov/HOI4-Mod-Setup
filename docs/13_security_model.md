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

## ChatGPT and Codex authentication boundary

The application delegates ChatGPT authentication to the official Codex App Server managed flow. It does not own OAuth tokens and does not read Codex credential storage.

Core rules:

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

## Secrets

Secret values do not enter logs through debug formatting, are not copied automatically, do not enter crash reports, are not stored or hashed, are not shown in previews, and are injected only into approved child processes.

## Command execution

Represent commands as executable plus argument array. Do not build a shell string for ordinary execution. A checked repository wrapper is an executable artifact with a verified hash. Display arguments in a safely escaped form with secrets removed.

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

Traversal, symlink races, Windows junction escape, case collision, reserved names, huge files, malformed TOML, command injection, environment injection, secret redaction, crash reporting, interrupted apply, and compromised manifest fixtures.

## Codex input disclosure

Every existing-project Codex request has a visible input manifest. The user can remove files or excerpts before transmission. Exclude binaries, `.git/`, credential stores, environment files, secrets, provider caches, large generated assets, and any path outside approved roots. Hash the approved input and validate the response schema before showing proposals.
