# Windows and macOS architecture

## Recommended stack

Use a Tauri desktop shell with a Rust core and a TypeScript React UI. Select supported stable framework versions at implementation start and lock them. The reasons are strong filesystem and process control, native packaging, compact distribution, native credential adapters, and a clean separation between declarative plans and mutation.

## Process boundaries

### UI

Owns wizard state, review screens, previews, progress, accessibility, and non-secret preferences. It cannot mutate project files directly.

### Rust core

Owns project identity, scanner, source resolver, manifest, dependency graph, downloads, hashes, merge, transactions, validators, credentials, MCP, Git, readiness, and recovery.

### Child-process runner

Receives an allowlisted executable and argument array. Enforces cwd, environment redaction, timeout, cancellation, output sanitation, and platform rules.

## Codex App Server bridge

The shared Rust core owns a `CodexBridge` adapter that starts `codex app-server`, frames stdio JSONL messages, performs initialize, tracks request IDs, consumes notifications, supervises the process, and exposes typed application events to Tauri.

The bridge owns capability detection, account state, browser and device-code login, logout, rate-limit checks, thread and turn lifecycle, read-only sandbox policy, output schema attachment, response validation, cancellation, and process restart.

The React layer never talks to the Codex process directly. It receives redacted typed state through Tauri commands and events. The target project is never a writable root for semantic analysis.

## Provider adapter boundary

`AiProviderAdapter` is a Rust-owned interface behind the same Tauri boundary.
It exposes provider profiles, local configuration status, scoped vault reads,
bounded analysis requests, response extraction, and schema validation. The
renderer never sends a secret in a project state object and never calls a
provider directly.

Codex delegates to the App Server bridge. Claude delegates to an Anthropic
messages envelope. Kimi, GLM, DeepSeek, local, and other configured profiles
use the OpenAI-compatible envelope. Hosted endpoints are explicit HTTPS user
configuration with redirects disabled; local endpoints are explicit loopback
HTTP configuration. The adapter does not invent provider login routes,
endpoints, model names, commands, or packages.

## Modules

```text
core/
  project/
  scanner/
  source/
  manifest/
  components/
  downloads/
  hashing/
  merge/
  transaction/
  validators/
  credentials/
  mcp/
  git/
  readiness/
  recovery/
  platform/
```

## Windows adapter

- canonical and long paths
- junction and reparse-point checks
- Credential Manager and DPAPI
- launcher descriptor external path
- `.exe`, `.cmd`, and PowerShell resolution
- antivirus and cloud-sync aware replacement
- Windows permissions and attributes

## macOS adapter

- canonical path and case-sensitivity checks
- symlink defense
- Keychain Services
- Application Support and cache paths
- app-bundle executable resolution
- quarantine and executable-permission reporting
- atomic replacement and file-coordination considerations

## HOI4 paths

Suggest common Steam and user-data locations, then require user confirmation. A suggestion is not proof.

## Launcher descriptor

The project descriptor stays inside the mod. The launcher descriptor is an explicit external destination with backup, hash, and validation.

## Platform-neutral core

Project model, manifest schema, component graph, scan evidence, hashing, merge decisions, lock, readiness, and UI should behave identically.

## Platform-specific components

A component can be unsupported without making the application unsupported. Current examples are `hoi4-agent-tools.cmd` and 3D `.cmd` wrappers. On macOS, show the limitation and do not invent a translation.

## Packaging

Production should use Windows code signing, macOS Developer ID signing and notarization, deterministic build metadata, and signed application-update packages. Application updates are separate from project workflow updates.

## Local data

### Windows

- settings and logs: `%APPDATA%/HOI4 Mod Setup/`
- caches and backups: `%LOCALAPPDATA%/HOI4 Mod Setup/`
- credentials: Windows Credential Manager

### macOS

- settings and backups: `~/Library/Application Support/HOI4 Mod Setup/`
- caches: `~/Library/Caches/HOI4 Mod Setup/`
- credentials: Keychain

## Open in Codex

Use a platform adapter that detects a supported Codex app or CLI, previews the exact project-opening action, and opens the selected root. If no opener is found, keep readiness pass and show the path plus manual instructions.

The Tauri command returns a typed non-error result for this opener-unavailable case. The React Ready screen announces the manual path without downgrading readiness; authentication, invalid-root, readiness, and process failures remain errors.

## Provider and Codex prerequisite resolution

For Codex, Windows and macOS resolve the official `codex` executable through an explicit configured path and a narrow allowlisted PATH lookup. The app verifies `app-server` support before login. Missing or incompatible Codex blocks Codex planning and offers official setup guidance. Non-Codex profiles require their user-supplied endpoint and vault reference. Do not invent a platform-specific installer command in application code.

## App Server process contract

Both platforms launch `codex app-server` as a supervised child process with stdio JSONL. The bridge performs initialize before account or thread calls, correlates request IDs, bounds message size, handles streamed notifications, redacts logs, and terminates the child cleanly. Browser login opens the returned URL with the platform shell. Device-code login uses the returned verification URL and code. No token file is read by the application.
