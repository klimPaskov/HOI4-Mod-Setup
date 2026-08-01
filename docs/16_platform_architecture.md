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

## Tauri command responsiveness

Every Tauri desktop command must use async/thread-pool dispatch. Filesystem,
network, Git, provider, child-process, hashing, and other potentially blocking
waits run off the desktop event loop; a command handler must not perform those
waits synchronously on the event-loop thread. Progress, cancellation, and
completion return through typed state or correlated events without changing the
ownership boundary.

Keep a regression test with blocking fake filesystem, network, Git, and
provider adapters. While each representative command is held in its wait, the
test must prove that the desktop event loop continues to service a probe and
that cancellation remains observable. A command that blocks that probe fails
the regression even when its eventual result is correct.

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
messages envelope. Kimi, GLM, DeepSeek, local, and the bounded `custom` profile
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

## HOI4 user mod directory resolution

Resolve the platform's user `Documents` location through the native known-folder
or user-directory API, not by concatenating `%USERPROFILE%\Documents`, `~`, or a
guessed OneDrive/iCloud path. The native result honors redirected Documents
locations. Append `Paradox Interactive/Hearts of Iron IV/mod` using the
platform separator, show the resolved absolute path as evidence, and validate
it before it is used. Do not search the whole computer or infer this path from
the Steam installation.

If the resolved directory is missing, inaccessible, or not the location the
user wants, allow an explicit path override and show the override in the dry
run. Resolution and scanning never create it. An external launcher descriptor
is read or written only after the user has visibly confirmed its reviewed path.

## New-project path defaults

After deterministic validation of `project_id`, populate the new-project root
and launcher descriptor paths. With the default parent, the root is
`<resolved HOI4 user mod directory>/<project_id>` and the external descriptor
is `<resolved HOI4 user mod directory>/<project_id>.mod`. If the user already
selected another project parent, use that parent for the root while retaining
the resolved HOI4 `mod` directory for the external descriptor. Both fields are
editable overrides. Changing the ID updates only fields that are still
auto-filled; a manually edited path is preserved and revalidated. No project
root or descriptor is created before apply.

## Launcher descriptor

The project descriptor stays inside the mod. The launcher descriptor is an
explicit external destination with backup, hash, and validation. For an
existing selected mod, automatic discovery is bounded to direct descriptor
candidates in the selected root's immediate parent; it never searches sibling
trees or the computer. The user must confirm a candidate before the scanner
reads it.

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

## Safe external GitHub links

The version 1 Ready-screen portrait link is a fixed HTTPS URL:
`https://github.com/klimPaskov/comfyui-hoi4-portraits`. It is opened through
the typed system-browser action, never through a shell or a URL supplied by a
provider, manifest, project file, or scan result. The link is informational and
does not claim that the external workflow is installed or ready.

## Provider and Codex prerequisite resolution

For Codex, Windows and macOS resolve the official `codex` executable through an explicit configured path and a narrow allowlisted PATH lookup. The app verifies `app-server` support before login. Missing or incompatible Codex blocks Codex planning and offers official setup guidance. Non-Codex profiles require their user-supplied endpoint and vault reference. Do not invent a platform-specific installer command in application code.

## App Server process contract

Both platforms launch `codex app-server` as a supervised child process with stdio JSONL. The bridge performs initialize before account or thread calls, correlates request IDs, bounds message size, handles streamed notifications, redacts logs, and terminates the child cleanly. Browser login opens the returned URL with the platform shell. Device-code login uses the returned verification URL and code. No token file is read by the application.
