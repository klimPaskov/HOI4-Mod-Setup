# HOI4 Mod Setup

HOI4 Mod Setup is a Windows and macOS desktop wizard that prepares a new or
existing Hearts of Iron IV mod for agentic development. It creates launcher
files, project structure, verified workflow files, and a readiness report
without silently replacing user work.

> **Status:** in development. Public installers will be linked from GitHub
> Releases only after the security, recovery, signing, and native platform
> gates pass.

## Install

The public source repository is [klimPaskov/HOI4-Mod-Setup](https://github.com/klimPaskov/HOI4-Mod-Setup).
Public binary releases will be linked from its GitHub Releases page: a signed
Windows `.exe` installer and a signed/notarized macOS `.dmg`. Until the first
release is published, contributors can use the setup in
[DEVELOPMENT.md](DEVELOPMENT.md). Do not download installers from unverified
mirrors.

## Choose a planning provider

Codex is the default. The first screen lets you choose:

- Codex through the official local Codex App Server and ChatGPT sign-in;
- Claude through an Anthropic-compatible endpoint and an API key in the OS
  credential vault;
- Kimi, GLM, or DeepSeek through a user-supplied OpenAI-compatible HTTPS
  endpoint and vault key;
- a local model through a user-supplied loopback HTTP endpoint; or
- another OpenAI-compatible provider through its explicit HTTPS endpoint and
  vault key.

The selected model and provider profile shape the analysis prompt, adapted
`AGENTS.md`, project `README.md`, and maintenance review. The app does not
invent provider URLs, OAuth routes, commands, packages, or model names. Rust
validates every provider response against the same review schema before the
user can approve a plan.

## Create or import a mod

For a new project, describe the mod in plain language. The selected provider
proposes an editable identity, project ID, namespaces, tags, folder profile,
instructions, skills, subagents, and components. For an existing project, a
bounded read-only scan records evidence for descriptors, launcher state,
structure, Git, identifiers, naming, localisation, docs, skills, subagents,
Codex/MCP files, paths, and conflicts before semantic review.

After review and dry-run approval, the app can create and validate:

- `descriptor.mod`, the launcher descriptor, and a replaceable `thumbnail.png`;
- the selected HOI4 folder scaffold;
- adapted `AGENTS.md`, `README.md`, skills, subagents, Codex/MCP files;
- selected scripts, validators, templates, docs, and
  `<mod_project>/paradox_wiki/`; and
- `.hoi4-mod-setup/installation-lock.json` after final verification.

## Optional ChatGPT sources folder

When Codex is selected, the Git phase offers an optional final checkbox to
prepare `<mod_project>/chatgpt_project_sources/`. It includes the adapted
`AGENTS.md`, created `README.md`, every skill as `<skill>.md`, every selected
subagent, and only the additional project-relative files the user names. The
normal source trees remain intact. The app checks containment, links, hashes,
collisions, file sizes, and secret-shaped content through the transaction.

The final screen only recommends: **After setup, start planning using ChatGPT
"Chat".** No upload, conversation, or planning action starts automatically.

## Safe installation and updates

Every mutation uses preflight, exact source resolution, selective download,
SHA-256 verification, dry-run review, backup, staging, validation, apply,
post-install checks, readiness, and a rollback record. Modified files receive
base/local/incoming comparison and only valid keep, replace, merge, rename, or
skip choices. Update, repair, reinstall, rollback, and managed removal remain
available with honest incomplete or unsupported states.

## Optional workflows

The app asks exactly **Do you want to set up the 3D models workflow?** A Meshy
key is stored in Windows Credential Manager or macOS Keychain and injected only
as `MESHY_API_KEY`. A missing key leaves 3D incomplete without blocking core
setup; Windows-oriented repository steps are not translated into invented
macOS commands.

It also asks exactly **Do you want to set up LoRAs and ComfyUI for portrait
generation?** Version 1 records interest only. It does not install or modify
ComfyUI, models, LoRAs, Python, GPU software, or drivers.

## Privacy and security

The app has no telemetry in version 1. Provider keys and ChatGPT tokens never
enter project files, plans, locks, logs, or screenshots. Only approved evidence
is sent for semantic analysis. Source files are resolved to one exact manifest
revision and SHA-256 checked. External actions appear in the dry run, and
user-modified files are never silently replaced.

Contributor setup, security reporting, and release maintenance are in
[CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md),
[DEVELOPMENT.md](DEVELOPMENT.md), and [RELEASING.md](RELEASING.md).

## License

HOI4 Mod Setup is released under the [Apache License 2.0](LICENSE).
