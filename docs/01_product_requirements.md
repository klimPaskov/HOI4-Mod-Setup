# Product requirements

## Product statement

**HOI4 Mod Setup** is a Windows and macOS desktop application that prepares a Hearts of Iron IV mod project for agentic development with a user-selected AI provider. Codex is the default. It creates a new launcher-ready mod from a guided brief or imports an existing project through an evidence-backed read-only scan. It installs a selected workflow package from `klimPaskov/Agentic-HOI4-Modding` without cloning the complete source repository. Structural analysis is deterministic. Required semantic analysis uses the selected provider adapter and produces reviewable proposals after deterministic evidence is collected.

## Primary outcome

A successful core setup leaves the selected mod project with:

- a valid internal `descriptor.mod` inside the project
- a valid external launcher `<project_id>.mod` in the confirmed HOI4 user mod directory
- a deterministic, valid, replaceable `thumbnail.png` placeholder
- a readable profile-specific initial mod folder structure
- launcher discoverability without manual file creation or descriptor editing
- an adapted `AGENTS.md`
- selected generic HOI4 skills and helper files
- selected generic Codex subagents
- Codex configuration
- repository-declared MCP configuration
- selected scripts, validators, templates, and documentation
- an offline wiki under `<mod_project>/paradox_wiki/`
- optional Git initialization or preservation
- a project installation lock
- a readiness report
- **Open in Codex** enabled when every blocking requirement passes

## Target users

### New mod author

Needs the app to turn a normal-language idea into a safe initial project without requiring knowledge of every HOI4 folder or Codex file.

### Existing mod maintainer

Needs a read-only scan, small review steps, preservation of local conventions, conflict-aware merges, and reversible installation.

### Advanced workflow maintainer

Needs pinned installs, optional 3D support, exact provenance, update and repair, and full external-command visibility.

## Product principles

1. Selective source use.
2. Read before write.
3. Evidence over inference.
4. No silent overwrite.
5. Exact source identity.
6. Secrets stay outside the project.
7. Optional means non-blocking.
8. Package names and commands come from verified repository evidence.
9. Every apply is reversible.
10. Placeholder and unsupported states remain honest.
11. Deterministic facts and Codex proposals are never presented as the same evidence class.
12. A new project is not ready until both descriptors and the thumbnail pass validation.

## Planning provider requirement

The first setup screen selects Codex, Claude, Kimi, GLM, DeepSeek, a local
model, or another bounded provider profile. Codex uses the official local Codex
App Server and ChatGPT-managed browser or device-code authentication. Hosted
non-Codex profiles use a user-supplied endpoint and an API key stored in the OS
credential vault. Local models use an explicit loopback HTTP endpoint. The
application does not invent provider URLs, OAuth routes, package names,
commands, model names, MCP servers, or platform support.

All semantic fields use the selected provider profile:

- normalized project description
- display name proposal
- project ID proposal
- script prefix and namespace proposal
- descriptor tag proposal
- initial folder profile proposal
- `AGENTS.md` adaptation
- skill and subagent recommendations
- existing-project purpose and convention analysis
- semantic conflict explanation

The deterministic core validates syntax, collisions, paths, hashes, descriptors, PNG files, encodings, Git state, manifest rules, and transaction safety. The user confirms each proposal before rendering.

The Codex integration is `codex app-server` over stdio JSONL. Authentication
uses the App Server managed ChatGPT browser flow with device-code fallback;
Codex owns tokens and refresh. Non-Codex credentials are read only from the OS
vault for the scoped provider request. No ChatGPT token, provider key, account
identity, or credential value is stored in the target project or installation
lock. All provider responses validate against the same `codex-analysis`
schema, and Rust remains the authority for deterministic facts and writes.

## Functional requirements

### Project selection

The welcome screen offers **Create a new mod** and **Import an existing project**. Recent projects are application-local. The app never searches the whole computer for a source repository or mod project.

### New project creation

Collect and review:

- selected provider configuration or ChatGPT sign-in through Codex App Server
  and required provider semantic analysis
- natural-language mod description
- display name
- stable project ID
- project folder
- supported game version
- initial version and tags
- internal `descriptor.mod` preview
- external launcher `<project_id>.mod` preview and destination
- generated `thumbnail.png` preview and placeholder style
- initial folder profile
- source mode
- selected components
- optional workflows
- MCP and credential choices
- Git choices

No file is created before dry-run approval. After approval, the transaction creates the project root, internal descriptor, external launcher descriptor, thumbnail placeholder, selected folder profile, and selected workflow components. The new mod must appear in the HOI4 launcher and load as an empty or scaffolded local mod without manual file creation, assuming the selected HOI4 user directory is valid.

#### Launcher-ready artifact contract

- `descriptor.mod` lives inside the project root.
- `<project_id>.mod` lives in the user-confirmed Hearts of Iron IV `mod` directory.
- The launcher descriptor contains the canonical absolute project path using platform-correct escaping.
- The descriptors share the reviewed name, version, supported game version, tags, and picture reference where applicable.
- The app does not fabricate `remote_file_id` or another Workshop identity.
- `thumbnail.png` is rendered deterministically from a bundled template, previewed before apply, decoded after staging, and tracked by hash.
- The placeholder remains managed until the user modifies or replaces it. Updates and repairs never overwrite a modified thumbnail silently.
- A thumbnail conflict offers Keep, Replace, Rename, or Skip. A valid existing thumbnail can satisfy readiness.
- The selected profile creates real directories such as `events/`, `common/`, `localisation/english/`, `gfx/`, `interface/`, and `docs/` where appropriate. The profile does not create map or history surfaces unless selected.

### Existing project import

The user selects one root. The scanner detects:

- folder structure
- both descriptors
- Git state
- IDs and namespaces
- file and folder naming patterns
- localisation naming and encoding
- documentation
- skills and helper tools
- subagent TOMLs
- `AGENTS.md`
- Codex configuration
- MCP definitions
- absolute paths and project-specific examples
- conflicts with incoming components
- launcher descriptor registrations and thumbnail state

Findings are grouped into reviewable steps and can be accepted, edited, rejected, or deferred when the value is not needed for generation. Deterministic findings remain authoritative for observable facts.

### Required provider semantic analysis

A valid analysis from the selected provider is required before Create, Import,
Update, or Repair can produce an installation plan. Codex uses the official
local App Server and ChatGPT-managed login. Other hosted providers require an
explicit HTTPS endpoint and OS-vault key; local requires an explicit loopback
HTTP endpoint. Do not create unverified provider login routes or silently
switch providers.

- complete the selected provider connection or Codex App Server handshake;
- use the chosen model and optimization profile;
- Preview the exact structured findings and text excerpts before each turn.
- Exclude binaries, secrets, credential stores, Git objects, unapproved paths, and unrelated content.
- Require output that validates against `schemas/codex-analysis.schema.json`.
- Use the selected provider for project identity, description, namespace and
  prefix proposals, tags, folder profile, AGENTS adaptation, component
  selection, convention interpretation, and semantic conflict explanations.
- Keep paths, hashes, descriptor validity, PNG validity, encodings, Git state, identifier syntax, collisions, manifest checks, and transaction safety under deterministic Rust ownership.
- Label values as `Detected`, `Suggested by Codex`, or `Confirmed`.
- Require deterministic validation and user confirmation before file rendering.
- Preserve the draft when sign-in, usage, process, or schema validation fails. Start no transaction.

Recovery, rollback, backup inspection, and managed removal remain available
while signed out or disconnected.

### Remote source resolution

Use GitHub API and raw endpoints. Never clone Agentic-HOI4-Modding.

Latest mode resolves the default branch to an exact commit, fetches the manifest at that commit, and records it. Pinned mode accepts an exact commit or immutable release identity.

### Component selection

Each component shows:

- source and destination
- selected state
- dependencies
- platform support
- required tools and environment variables
- estimated size
- conflict state
- validation rules
- update behavior

Required dependencies are selected automatically and remain visible.

### Offline wiki

Install under `<mod_project>/paradox_wiki/`. Verify path containment, every declared hash, required page coverage, media policy, snapshot marker when declared, and link integrity. Show source and licensing evidence without invention.

### Optional 3D workflow

Ask exactly:

**Do you want to set up the 3D models workflow?**

When yes:

- explain Meshy.ai and possible provider cost
- store the key in Windows Credential Manager or macOS Keychain
- record only an opaque credential reference
- inject the key as `MESHY_API_KEY` into approved processes
- validate presence and non-empty state without printing it
- install only repository-declared 3D components
- use repository-declared packages, versions, commands, adapters, and health checks
- include the workflow in dry-run and readiness

A missing key leaves the optional workflow incomplete and does not block core setup.

### LoRA and ComfyUI placeholder

Ask exactly:

**Do you want to set up LoRAs and ComfyUI for portrait generation?**

Version 1 records interest only. It creates no download, install, model, Python, ComfyUI, LoRA, GPU, or driver operation. Readiness reports `planned_unavailable` and never claims success.

### MCP setup

Show source, capabilities, platform support, tools, environment variables, command preview without secrets, installation state, health result, and update policy. Never invent an unsupported platform command.

### Git setup

Support initialize, preserve, or skip. Support `.gitignore` merge, branch selection, optional initial commit, and optional remote. Never create an online repository or push without a separate explicit approval.

### Transactional installation

All installs and updates use:

1. preflight
2. repository source resolution
3. selective download
4. checksum verification
5. dry-run review
6. backup
7. staging
8. validation
9. apply
10. post-install checks
11. readiness report
12. rollback record

### Maintenance

Support update, repair, reinstall, rollback, managed removal, optional-workflow changes, and later Meshy credential configuration.

### Readiness

Check the internal descriptor, external launcher descriptor, external destination, descriptor agreement, thumbnail existence and decoding, selected structure, AGENTS, skills, subagents, selected provider instructions, MCP, wiki integrity and page coverage, Git, environment variables, hashes, conflicts, external dependencies, 3D state, and LoRA placeholder state. Codex authentication or selected-provider configuration and confirmed analysis are blocking core checks.

Open in Codex is enabled only when blocking core checks pass.

## Non-functional requirements

### Reliability

- interrupted setup can resume or roll back
- operation-level apply is idempotent
- optional failures cannot corrupt core setup
- retry does not duplicate descriptors, Git rules, or MCP tables

### Performance

- first scan findings within one second for a typical mod
- progress streams for large projects
- bounded hashing and parser concurrency
- cancellable work
- immutable download cache

### Security

- no secret in logs, files, previews, manifests, locks, crash reports, or analytics
- path traversal and link escape protection
- exact commit and SHA-256 verification
- external commands only from verified sources
- no administrator or root requirement for core setup

### Privacy

No telemetry in version 1. Recent projects and scan findings stay local.

### Interface clarity and accessibility

The wizard uses seven grouped phases instead of one step per screen. Each screen has one focal task, one title, no more than one supporting sentence, and no more than two visible content regions by default. Evidence, file lists, hashes, logs, dependency graphs, and advanced controls use progressive disclosure. The interface does not explain ordinary controls or repeat the same fact in several places.

Full keyboard operation, visible focus, WCAG 2.2 AA contrast, screen-reader semantics, reduced motion, text plus icon statuses, and usable 200 percent scaling are required.

## Out of scope in version 1

- editing HOI4 gameplay content
- installing Steam or HOI4
- whole-computer mod discovery
- Steam Workshop publishing
- automatic online Git repository creation or push
- automated LoRA or ComfyUI installation
- invented macOS MCP or 3D commands

## Open-source application repository

HOI4 Mod Setup itself is developed as a public GitHub project.

The source repository must include:

- a user-facing root `README.md`
- a real open-source `LICENSE` before public release
- contributor, development, security, conduct, and release documentation
- issue forms and pull request template
- CODEOWNERS for sensitive surfaces
- protected `main` through a GitHub ruleset
- CI for planning integrity, frontend, Rust, security, and supported platforms
- Dependabot configuration for npm, Cargo, and GitHub Actions
- tag-based draft release automation
- root `AGENTS.md`
- maintained repo-local skills under `.agents/skills/`
- bounded subagents under `.codex/agents/`

Contributor-only build and governance information must stay out of the user-facing README.

Repo-local skills are living implementation memory. A pull request that changes a repeated workflow, command, path, invariant, schema, platform rule, validation step, or recovery method updates the owning skill in the same change.

## Core authentication readiness

A setup session is core-ready only when the selected provider is configured (or a compatible Codex App Server is initialized with active ChatGPT-managed authentication), required semantic analysis validates against the current schema, and every required proposal is confirmed. Optional 3D, LoRA, and Codex-only flattened Chat-source states remain independent.
