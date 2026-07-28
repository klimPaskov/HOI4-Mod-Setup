# HOI4 Mod Setup

HOI4 Mod Setup is a Windows and macOS desktop wizard that prepares a new or
existing Hearts of Iron IV mod for agentic development. It creates launcher
files, project structure, verified workflow files, and a readiness report
without silently replacing user work.

> **Status:** the source repository is public. Development previews are
> available from GitHub Releases; stable installers will be linked only after
> the security, recovery, signing, and native platform gates pass.

## Install

The public source repository is [klimPaskov/HOI4-Mod-Setup](https://github.com/klimPaskov/HOI4-Mod-Setup).
For the easiest installation, use its [GitHub Releases](https://github.com/klimPaskov/HOI4-Mod-Setup/releases) page:

- Windows: download the `.exe` installer and run it.
- macOS: download the `.dmg`, open it, and move the app to Applications.

GitHub Releases use explicit platform names so the download is easy to spot:
`HOI4-Mod-Setup-windows-x64-setup.exe`, `HOI4-Mod-Setup-macos-arm64.dmg`
(Apple silicon), and `HOI4-Mod-Setup-macos-x64.dmg` (Intel). The release notes
also show each package's SHA-256 and verification manifest.

Development previews are source-built test packages. Windows or macOS may
show a platform security warning because stable publisher signing and
notarization are separate release gates. Before installing a preview, verify
the included provenance and SHA-256 files against the public source commit.
Stable releases will be clearly labelled when those gates are complete. Do
not download installers from unverified mirrors.

## Screenshots

These screenshots are captured from the current wizard UI. They show the main
setup path, the provider-neutral configuration surface, and the Codex-only
flattened Chat sources option.

<table>
  <tr>
    <td><img src="docs/screenshots/01-welcome.png" alt="HOI4 Mod Setup welcome screen with new and existing mod choices" width="480"></td>
    <td><img src="docs/screenshots/02-description.png" alt="Natural-language mod description screen" width="480"></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/03-identity.png" alt="Project identity and Hearts of Iron IV descriptor screen" width="480"></td>
    <td><img src="docs/screenshots/04-components.png" alt="Verified component selection screen" width="480"></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/05-integrations.png" alt="Optional 3D models and LoRA workflow choices" width="480"></td>
    <td><img src="docs/screenshots/06-mcp-credentials.png" alt="MCP and credential review screen" width="480"></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/07-git-chat-sources.png" alt="Git setup with the optional flattened ChatGPT project sources checkbox" width="480"></td>
    <td><img src="docs/screenshots/08-flattened-chat-sources.png" alt="Expanded flattened ChatGPT project sources option with additional files field" width="480"></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/09-dry-run.png" alt="Dry-run review showing planned changes and preflight checks" width="480"></td>
    <td><img src="docs/screenshots/10-provider-selection.png" alt="Claude provider selection with model, endpoint, and vault-key controls" width="480"></td>
  </tr>
</table>

The browser preview intentionally shows safe unavailable states when a native
Tauri process or remote manifest is not present. Native package verification
and desktop launch smoke run on the Windows and macOS GitHub Actions runners.

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

For a new project, enter a mod name and describe it in plain language. The app
immediately fills an editable project ID, script prefix, namespace, descriptor
tags, and starter folders; the selected provider can refine those suggestions
before review. You do not need to invent naming conventions from scratch. For
an existing project, a
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
