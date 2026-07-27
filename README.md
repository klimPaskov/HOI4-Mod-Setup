# HOI4 Mod Setup

HOI4 Mod Setup prepares a new or existing Hearts of Iron IV mod for development in Codex. It creates the launcher files, project structure, Codex instructions, selected workflows, and local validation needed to begin work without manual setup.

> **Status:** in development. Public builds will be published through GitHub Releases after the transaction, recovery, security, and platform gates pass.

## What you need

- Windows or macOS
- Hearts of Iron IV installed or a chosen local mod workspace
- a ChatGPT account with Codex access
- the official Codex client installed with App Server support
- internet access while signing in and downloading selected workflow components

HOI4 Mod Setup uses the Codex access included with the signed-in ChatGPT account. It does not ask for an OpenAI API key.

## New mods

Describe the mod in plain language. Codex proposes the project identity and setup, including:

- mod display name
- stable project ID
- script prefix and namespace
- short project description
- descriptor tags
- initial folder profile
- project-specific `AGENTS.md`
- recommended skills, subagents, and components

Every proposal is editable. Deterministic validation checks paths, identifier syntax, collisions, descriptor structure, and file safety before anything is written.

After approval, the app creates and validates:

- `<mod_project>/descriptor.mod`
- `<HOI4 user mod directory>/<project_id>.mod`
- `<mod_project>/thumbnail.png`
- the selected HOI4 folder scaffold
- `AGENTS.md`
- selected `.agents/skills/`
- selected `.codex/agents/`
- `.codex/config.toml`
- `<mod_project>/paradox_wiki/`
- selected documentation, scripts, validators, and templates
- `.hoi4-mod-setup/installation-lock.json`

The generated thumbnail is a replaceable local placeholder. Updates never overwrite a replacement silently.

## Existing mods

The app first runs a bounded read-only scanner over the selected project and approved companion paths. It detects descriptors, launcher registration, folder structure, Git state, identifiers, namespaces, naming patterns, localisation conventions, documentation, skills, subagents, Codex configuration, MCP configuration, and conflicts.

Codex then interprets the approved scan evidence and proposes project-specific conventions and installation choices. Findings remain separated as **Detected**, **Suggested by Codex**, and **Confirmed**.

## ChatGPT sign-in

The app opens the official ChatGPT sign-in flow through the local Codex App Server. Codex manages and refreshes its own authentication. HOI4 Mod Setup does not read, copy, store, or log ChatGPT tokens.

A browser flow is used by default. A device-code flow is available when the browser callback cannot complete. Signing out removes the active Codex session through the same official interface.

Setup analysis requires an authenticated ChatGPT session. Local recovery and rollback remain available when signed out.

## Safe installation and updates

No project file changes before dry-run approval. Every installation uses preflight, exact source resolution, selective download, checksum verification, dry-run review, backup, staging, validation, apply, post-install checks, readiness, and a rollback record.

Modified files receive a comparison and an explicit keep, replace, merge, rename, or skip choice. The same project can later be updated, repaired, reinstalled, rolled back, or have managed components removed. Updates also run a fresh read-only Codex semantic review over approved scan evidence before an update plan is created.

## Optional workflows

### 3D models

The optional 3D workflow requires `MESHY_API_KEY`. The key stays in Windows Credential Manager or macOS Keychain and is injected only into the process that needs it. A missing key leaves the 3D workflow incomplete without blocking the normal mod setup.

### LoRA and ComfyUI portraits

Version 1 records interest only. It does not install or modify ComfyUI, models, LoRAs, Python environments, GPU software, or drivers.

## Privacy and security

- no telemetry in version 1
- no OpenAI API key field
- ChatGPT tokens remain owned by Codex
- approved text excerpts only are sent for semantic analysis
- secrets, binaries, Git objects, and credential stores are excluded
- downloads resolve to an exact source revision and are checked with SHA-256
- external commands appear in the dry run before execution
- user-modified files are never replaced silently

## Help, security, and contributing

Use [GitHub Issues](../../issues) for reproducible bugs and feature requests. Remove private paths, project content, and credentials before posting evidence.

Report vulnerabilities through [SECURITY.md](SECURITY.md). Contributor setup and Git instructions are in [CONTRIBUTING.md](CONTRIBUTING.md) and [DEVELOPMENT.md](DEVELOPMENT.md).

## License

A formal open-source license must be selected before the first public source release. The current decision record is in [LICENSE_SELECTION.md](LICENSE_SELECTION.md).
