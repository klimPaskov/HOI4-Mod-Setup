# Product requirements

## Product statement

**HOI4 Mod Setup** is a Windows and macOS desktop application that prepares a Hearts of Iron IV mod project for provider-neutral agentic development. The user selects a setup assistant for bounded semantic analysis; Codex is the default. That choice does not select or restrict the AI client used later for development. The app creates a new launcher-ready mod from a guided brief or imports an existing project through an evidence-backed read-only scan. It installs a selected workflow package from `klimPaskov/Agentic-HOI4-Modding` without cloning the complete source repository. Structural analysis is deterministic. Required semantic analysis uses the selected setup-provider adapter and produces reviewable proposals after deterministic evidence is collected.

## Primary outcome

A successful core setup leaves the selected mod project with:

- a valid internal `descriptor.mod` inside the project
- a valid external launcher `<project_id>.mod` in the confirmed HOI4 user mod directory
- a deterministic, valid, replaceable 600×600 solid-black `thumbnail.png`
- a readable profile-specific initial mod folder structure
- launcher discoverability without manual file creation or descriptor editing
- an adapted `AGENTS.md` with template-only placeholder guidance removed
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
13. New-project identity conventions are generated from the mod name and brief; manual review is for edits, ambiguity, or external paths.

## Setup assistant requirement

The first setup screen selects Codex, Claude, Kimi, GLM, DeepSeek, a local
model, or a bounded custom profile as the setup assistant. Codex uses the official local Codex
App Server and ChatGPT-managed browser or device-code authentication. Hosted
non-Codex profiles use a user-supplied endpoint and an API key stored in the OS
credential vault. Local models use an explicit loopback HTTP endpoint. The
application does not invent provider URLs, OAuth routes, package names,
commands, model names, MCP servers, or platform support.

The first screen fetches the selected provider's available model catalog after
authentication, exposes the selected model's supported reasoning levels from
Light through Max, and persists both choices. Codex defaults to Luna at xhigh;
DeepSeek defaults to its Flash model. A failed or empty catalog refresh keeps
the checked-in verified model selectable for Codex and known hosted providers;
Local and Custom keep an editable model field and add live endpoint results as
suggestions. These values are retained as analysis
provenance in app-managed state, plans, and locks; they are not written into
generated project guidance as a future development preference.

All semantic proposals use the selected setup profile:

- normalized project description
- display name proposal
- project ID proposal
- script prefix and namespace proposal
- descriptor tag proposal restricted to the current official Hearts of Iron IV Workshop categories
- initial folder profile proposal
- project-specific `AGENTS.md` adaptation without provider/model attribution
- skill and subagent recommendations
- existing-project purpose and convention analysis
- semantic conflict explanation

The deterministic core validates syntax, collisions, paths, hashes, descriptors, PNG files, encodings, Git state, manifest rules, and transaction safety. The user confirms each proposal before rendering provider-neutral project bytes. Changing the setup assistant never adds or removes a development-client component.

The Codex integration is `codex app-server` over stdio JSONL. Authentication
uses the App Server managed ChatGPT browser flow with device-code fallback;
Codex owns tokens and refresh. Non-Codex credentials are read only from the OS
vault for the scoped provider request. No ChatGPT token, provider key, account
identity, or credential value is stored in the target project or installation
lock. All provider responses validate against the same `codex-analysis`
schema, and Rust remains the authority for deterministic facts and writes.

## Coding environments

Coding-client selection is a separate setup step from the setup assistant and
from workflow profiles such as Core or Core plus 3D. Exactly one primary client
is required (Codex by default); any supported clients may be added alongside it:
Codex, Claude Code, Cursor, Qoder, and OpenCode. Changing the primary client
automatically removes it from the additional list.

The resolved source manifest supplies a complete native package for each
selected client. Codex keeps `.codex/agents/*.toml` as the canonical subagent
source; Claude Code, Cursor, Qoder, and OpenCode projections are generated from
those TOMLs and source-side synchronization checks reject drift. Runtime-neutral
files (`AGENTS.md`, `.agents/skills/`, canonical subagents, `.tools/sync/`,
shared docs, and selected workflows) are installed for every selection.

The primary/additional selection is persisted in project state, plans, locks,
and journals. Existing-project detection reports installed clients, defaults to
Codex when no recorded primary exists, and never silently removes a client.
Deselection previews all additions, updates, preserved conflicts, and removals;
only unchanged managed files are removed, while modified or unmanaged files are
kept.

## Functional requirements

### Project selection

The welcome screen offers **Create a new mod** and **Import an existing project**. Recent projects are application-local. The app never searches the whole computer for a source repository or mod project.

### New project creation

Ask for and review:

- setup-assistant configuration or ChatGPT sign-in through Codex App Server
  and required provider semantic analysis
- mod name and natural-language description
- generated display name, stable project ID, script prefix, namespace, descriptor
  tags, and initial folder profile
- project folder
- auto-filled project root derived from the validated project ID and selected parent
- supported game version
- initial version and tags
- internal `descriptor.mod` preview
- auto-filled external launcher `<project_id>.mod` preview and destination
- editable overrides for the project root and launcher descriptor destination
- generated `thumbnail.png` preview and placeholder style
- initial folder profile
- source mode
- selected components
- optional workflows
- MCP and credential choices
- Git choices

No file is created before dry-run approval. At apply, the transaction creates the selected project root—or exactly one reviewed leaf when the root was absent—then creates the internal descriptor, external launcher descriptor, thumbnail placeholder, selected folder profile, and selected workflow components. The new mod must appear in the HOI4 launcher and load as an empty or scaffolded local mod without manual file creation, assuming the selected HOI4 user directory is valid.

The app fills generated identity fields from the mod name and description before
the identity screen opens. After the project ID passes deterministic validation,
it fills the project root and external launcher descriptor path from that ID and
the resolved HOI4 user mod directory. The selected provider may suggest reviewed
alternatives, but the deterministic defaults remain until the user edits or
confirms them. Every generated value remains
editable, but the user is not asked to invent a project ID, prefix, namespace,
tags, folder list, or launcher filename from scratch. Changing the ID updates
only paths that remain auto-filled; an explicit override is preserved and
revalidated.

#### Launcher-ready artifact contract

- `descriptor.mod` lives inside the project root.
- `<project_id>.mod` lives in the user-confirmed Hearts of Iron IV `mod` directory.
- The default `mod` directory comes from the native platform `Documents`
  location, so Windows Documents redirection and supported macOS Documents
  redirection are honored; the user may explicitly override it.
- The launcher descriptor contains the canonical absolute project path using platform-correct escaping.
- The descriptors share the reviewed name, version, supported game version, tags, and picture reference where applicable.
- Script prefixes and primary namespaces are scripting conventions stored in setup metadata and project guidance. They are never written as `script_prefix` or `namespace` keys in either descriptor.
- The app does not fabricate `remote_file_id` or another Workshop identity.
- `thumbnail.png` is rendered deterministically from a bundled template, previewed before apply, decoded after staging, and tracked by hash.
- The placeholder remains managed until the user modifies or replaces it. Updates and repairs never overwrite a modified thumbnail silently.
- A thumbnail conflict offers Keep, Replace, Rename, or Skip. A valid existing thumbnail can satisfy readiness.
- The selected profile creates real directories such as `events/`, `common/`, `localisation/english/`, `gfx/`, `interface/`, and `docs/` where appropriate. The profile does not create map or history surfaces unless selected.

### Existing project import

The user selects one root. The scanner detects:

- both descriptors
- Git state
- agentic setup structure and approved documentation
- skills and helper tools
- subagent TOMLs
- `AGENTS.md`
- Codex configuration
- MCP definitions
- absolute paths and project-specific examples
- conflicts with incoming components
- launcher descriptor registrations and thumbnail state

The scanner does not inventory ordinary HOI4 gameplay, localisation, media,
generated documentation corpora, or root data dumps. Namespace, prefix,
naming, localisation, and folder-profile ideas are provider suggestions derived
only from the approved agentic evidence and remain subject to deterministic
validation and user confirmation.

Before the scan starts, the app performs a bounded, read-only discovery of
direct `*.mod` candidates in the selected root's immediate parent. It shows the
candidate path and the path-match evidence, then requires a visible choice to
confirm that descriptor, scan without an external descriptor, or cancel. It
does not search sibling trees or the whole computer. Only a confirmed external
descriptor path may be read by the scan.

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
- Use the selected setup assistant for project identity, description, namespace and
  prefix proposals, tags, folder profile, provider-neutral AGENTS adaptation,
  component recommendations, convention interpretation, and semantic conflict explanations.
- Keep paths, hashes, descriptor validity, PNG validity, encodings, Git state, identifier syntax, collisions, manifest checks, and transaction safety under deterministic Rust ownership.
- Label values as `Detected`, `Suggested by <selected setup assistant>`, or `Confirmed`.
- Require deterministic validation and user confirmation before file rendering.
- Preserve the draft when sign-in, usage, process, or schema validation fails. Start no transaction.

Recovery, rollback, backup inspection, and managed removal remain available
while signed out or disconnected.

### Remote source resolution

Use GitHub API and raw endpoints. Never clone Agentic-HOI4-Modding. Resolve the
single canonical `hoi4-mod-setup.manifest.json` path; its required
`schema_version` owns compatibility, and parallel filename aliases are not
published as update history.

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

The resolved manifest, not an app-binary component enum, owns the current
catalog. In Latest mode, changed or newly added files under declared skill and
subagent component trees are installed from the new exact revision after hash
verification. Compatible newly declared default-profile components are
selected for new setups and added during Update without reselecting optional
components that the user previously declined. Compatible optional workflow
components appear from their manifest metadata and remain explicit choices.

### Offline wiki

Install under `<mod_project>/paradox_wiki/`. Verify path containment, every declared hash, required page coverage, media policy, snapshot marker when declared, and link integrity. Show source and licensing evidence without invention.

### Optional 3D workflow

Show the title:

**3D models workflow**

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

### Optional Super Events workflow

On the same Optional workflows screen, show immediately after the 3D title:

**Super Events workflow**

Super Events is a provider-neutral optional component with no credential,
environment variable, external command, or provider-specific readiness gate.
When selected, `workflow.super_events` is resolved from the verified manifest at
the one installation revision and selectively installs the generic managed tree
at `.agents/skills/hoi4-super-events/`. Its only declared dependency is
`core.skills`. When declined, the tree is not downloaded or installed and the
adapted `AGENTS.md` receives no Super Events-specific guidance. The optional
state is visible in readiness and is carried through the managed lock and the
read-only existing-project scan.

Update may add the component from the selected target manifest. Repair may add
it only when the same immutable locked source revision declares it; otherwise
the maintenance action is Update. Missing or incomplete Super Events content
never blocks core readiness.

### ComfyUI HOI4 portrait workflow

Generic projects may select Cloud, Local, RunPod, or Disabled on the Optional workflows screen. The selection is persisted in project state, the installation plan and lock, the scan summary, readiness, settings, Update, and Repair. Enabled projects receive the complete portrait-production contract, the explicit provider router, only the selected provider skill, a bounded subagent, non-secret configuration, and the exact upstream revision recorded in `docs/32_comfyui_portrait_pipeline.md`. Disabled projects receive source-based portrait handling and no ComfyUI-specific files, marker sections, MCP configuration, or instructions.

The expanded portrait row states the minimum local recommendation as 16 GB of VRAM and 25 GB of storage. These are honest resource guidance, not a blocking core readiness requirement.

Cloud registers the official Comfy Cloud MCP route and may remain incomplete while authorization or subscription is deferred. Local discovery is bounded, loopback-only, and reports hardware, workflow, model, and Hugging Face state. RunPod records its URL/workspace and shows canonical setup and browser-control guidance without claiming readiness until the page and workflow are found. All provider credentials remain outside project files and locks.

Portrait sourcing remains separate from production. ComfyUI production applies only to sourced or grounded portraits. Non-sourced fictional or impossible portraits use native ImageGen and never enter the ComfyUI workflow. Durable source PNG/TXT pairs use the runtime basename, person-only prompts begin with `hoi4_portrait,`, and temporary provider failures produce an honest source-based DDS placeholder with a pending replacement state. See `docs/32_comfyui_portrait_pipeline.md` for the complete contract.

### MCP setup

Show source, capabilities, tools, environment variables, command preview without secrets, installation state, health result, and update policy. Platform compatibility is evaluated internally; ordinary screens show only a concise unavailable state when a selected route cannot run. Never invent an unsupported platform command.

MCP component selection, bootstrap, package verification, and app-owned health
checks are provider-neutral. `codex.config` is a development-client integration,
not a setup-assistant dependency, and may be installed regardless of which
provider performed setup analysis.

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

Support update, repair, reinstall, rollback, managed removal, optional-workflow changes, later Meshy credential configuration, and later Super Events addition.

### ChatGPT source package export

When an existing project scan finds an `AGENTS.md`, flattened skill, or
subagent source, the scan results and maintenance view offer a ChatGPT
project-source export;
an installation lock and complete core component set are not required. The
export page defaults to the platform Downloads folder and selects the detected
`AGENTS.md`, `README.md`, every direct skill flattened to
`<skill>.md`, and every direct TOML subagent. Other Markdown files immediately
under the mod root are listed as optional unchecked entries. The package is a
ZIP written outside the project; it uses validated absolute paths, rejects
links, traversal, collisions, oversized or secret-shaped content, refuses
overwrite, and leaves the project unchanged.

### Readiness

Check the internal descriptor, external launcher descriptor, external destination, descriptor agreement, thumbnail existence and decoding, selected structure, provider-neutral AGENTS, skills, subagents, setup-analysis provenance, independently selected client integrations, MCP, wiki integrity and page coverage, Git, environment variables, hashes, conflicts, external dependencies, and optional `workflow.3d`, `workflow.super_events`, and portrait-provider states. Setup-assistant configuration and confirmed analysis are blocking core checks; optional workflow states are not.

Open in Codex is enabled when blocking core checks pass and the Codex project
integration is installed, regardless of which setup assistant produced the
confirmed analysis.

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

Every Tauri desktop command uses async/thread-pool dispatch for filesystem,
network, Git, and provider waits. No such wait may block the desktop event
loop. A regression test must hold each representative wait open and verify that
the event loop remains responsive while the command is pending.

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
- tag-based release automation that publishes only after platform verification
- root `AGENTS.md`
- maintained repo-local skills under `.agents/skills/`
- bounded subagents under `.codex/agents/`

Contributor-only build and governance information must stay out of the user-facing README.

Repo-local skills are living implementation memory. A pull request that changes a repeated workflow, command, path, invariant, schema, platform rule, validation step, or recovery method updates the owning skill in the same change.

## Core authentication readiness

A setup session is core-ready only when the selected setup assistant is configured (or a compatible Codex App Server is initialized with active ChatGPT-managed authentication), required semantic analysis validates against the current schema, and every required proposal is confirmed. Optional 3D and flattened Chat-source states remain independent development choices.
