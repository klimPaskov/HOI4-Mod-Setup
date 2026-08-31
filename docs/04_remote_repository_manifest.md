# Remote repository and manifest design

## Purpose

The remote manifest is the installation contract between Agentic-HOI4-Modding and HOI4 Mod Setup. The single canonical discovery path is:

```text
hoi4-mod-setup.manifest.json
```

Compatibility is declared by the required `schema_version` inside this file.
Unsupported majors fail closed. The repository does not publish parallel
manifest filenames as update history.

## Latest mode

1. Query repository metadata.
2. Read the current default branch.
3. Resolve it to a commit SHA.
4. Fetch the manifest at that exact commit.
5. Parse the raw JSON and validate it against
   `docs/schemas/remote-manifest.schema.json` using Draft 2020-12 before typed
   deserialization. Then validate runtime trust-policy invariants, file
   evidence, and the declared generator revision.
6. Expand only selected component trees at the same commit.
7. Fetch selected files.
8. Verify SHA-256.
9. Record commit, manifest hash, manifest origin, platform, sizes, file hashes,
   the manifest's exact `wiki.required_pages` list, and the manifest-declared
   wiki snapshot/media/provenance/license metadata.

Latest mode becomes reproducible after installation because the lock records the exact commit.

The schema is closed at every declared object boundary. Unknown top-level or
nested policy fields fail before deserialization instead of being silently
discarded. Deterministic Rust validation remains responsible for cross-field
rules that JSON Schema cannot express.

## Pinned modes

Pinned commit accepts a full immutable commit and fetches everything from that revision. Pinned release resolves a release to immutable assets or a commit and records both when available. Mutable branch-only references are rejected as pinned installs.

The runtime rejects schema-declared wiki provenance/license values outside the
published enums, latest policies that do not require default-branch resolution
and commit recording, pinned modes disabled by the manifest, and required
signing policies because signature verification is not implemented in this
build. It records repository license evidence with the exact wiki metadata used
by the plan and lock; legacy locks without that field remain readable but are
shown as `unknown` until repaired.

## No clone

Use GitHub metadata, commit, tree, contents, raw, and release endpoints. Never run `git clone` for Agentic-HOI4-Modding. A repository-declared external dependency script may use Git for its own dependency only after dry-run disclosure and approval.

## Component contract

Each component defines:

- stable ID and display name
- category and optional state
- optional coding-environment ownership
- platforms
- source kind, path, includes, and excludes
- destination path and ownership
- dependencies
- tools and environment variables
- expected files and hashes
- conflicts and conflict policy
- validation
- update behavior
- capabilities and notes

The Agentic repository owns a generator and publication workflow for this
manifest. A relevant push enumerates only declared component source trees,
computes exact path/size/SHA-256 evidence from the pushed revision, validates
the result, runs every alternate-runtime synchronizer in drift-check mode,
verifies that `main` did not move underneath the run, and publishes
the refreshed manifest in a follow-up commit. New files inside `core.skills`
and `core.subagents` therefore require no app release. A genuinely new
component still needs an explicit source-owned component/profile declaration;
the app consumes compatible file-only declarations generically and never
invents them. A new or changed command-bearing component additionally requires
an app-owned allowlisted runtime adapter and therefore an app release.

At runtime Latest resolves the publication commit first and uses one immutable
revision for manifest and file downloads. New setup seeds the resolved default
profile. Update compares the installed manifest's default profile with the
newly resolved one and adds only newly published, provider-compatible,
platform-supported defaults, preserving earlier optional choices.

Coding-environment base components contain only runtime-neutral core agents.
An optional environment component is activated only when its declared optional
workflow dependencies are already in the selected closure. This lets one
Portrait Production or Super Events selection add only the projections for the
selected coding environments. Platform-specific MCP registrations are separate
optional environment components; a Windows `.cmd` registration must never be
declared by an all-platform component.

The current core profile includes `docs.mcp_integration`, a managed copy of the
repository's HOI4 Agent Tools capability, evidence, recovery, and
troubleshooting guide. It depends on `mcp.hoi4_agent_tools`, so the guide and
the reviewed MCP declaration remain bound to the same source revision.

The current portrait graph installs `workflow.portraits.core` plus
`workflow.portraits.router`, exactly one of the Cloud, Local, or RunPod provider
components, the bounded portrait subagent, and non-secret configuration. The
router is a dependency of each provider component; it is not synthesized by
the app or installed for Disabled projects.

The current Windows-only `workflow.3d` package includes the repository-owned
`blender_hoi4` production adapter, bounded worker, asset profiles, Meshy tool
contract, dependency record, bootstrap, and reviewed wrappers. Unrestricted
Blender Lab remains development-only; none of this evidence creates a macOS
route.

Its reviewed `3d.bootstrap` command runs automatically during transaction
post-install checks only after managed files validate. The manifest must match
the app-owned allowlisted command ID and fixed arguments; it also declares
network access, expected external writes, current-user privilege, and rollback
boundary. The app never executes arbitrary manifest commands, so changing the
3D command contract requires an app release. The runner verifies the installed
bootstrap bytes, injects `MESHY_API_KEY` only from the OS vault, and persists
`ready` only after an exit-zero verified-config run. Missing credentials or
prerequisites leave the optional workflow `incomplete` without blocking core
readiness.

The complete schema is `schemas/remote-manifest.schema.json`.

### `workflow.super_events`

The verified manifest defines `workflow.super_events` as an optional,
provider-neutral component for a complete reusable Super Events package. The
parent installs the single `hoi4-super-events` skill and three narrow research
subagents, then expands to hidden dependency components for
`interface/`, `common/`, `events/`, `localisation/`, `gfx/`, and
`docs/super_events/`. These dependencies install the working GUI,
GFX declarations, scripted GUI and registration effect, dynamic text and image
selectors, one console-test example, default localisation, DDS assets, and
editable Photoshop templates.

All runtime text is deterministically adapted to the confirmed project
namespace after its source bytes are verified. The upstream `hoi4ms_*` runtime
basenames are also renamed to the confirmed project prefix in the installed
text/interface destinations; source paths and checksums remain unchanged for
manifest evidence. Binary assets are copied without text or filename
adaptation. The package declares no tool, environment variable,
credential, external command, or health action, and audio remains an optional
later project addition with separate source and rights evidence. Every managed
component uses `replace_if_unmodified`, with obsolete-file removal and local
additions preserved according to the manifest.

The core skills selection excludes `hoi4-super-events/**`, while deterministic
adaptation removes the marked Super Events sections from the ordinary planning,
events, assets, text/audio, and subagent skills unless the workflow is selected. The core
subagent selection must exclude `hoi4_super_event_*.toml`, and every hidden
runtime dependency remains optional, so an install that declines the workflow
cannot receive the Super Events skill, runtime, research subagents, or conditional guidance accidentally. The adapted `AGENTS.md`
receives Super Events-specific guidance only when this component is selected
and its operation is approved. Selection, dependency expansion, file download,
hash verification, namespace adaptation, and lock evidence remain bound to the
same exact revision as the manifest.

## Source kinds

### File

One repository file maps to one destination or template transformer.

### Tree

A selected subtree expands at the resolved commit. Every resolved file receives a plan row and SHA-256. The installer still downloads only selected trees.

Disjoint tree components may share a standard managed destination directory such as `.agents/skills/` or `.codex/agents/`. Every selected concrete destination remains unique and is canonicalized before download; any selected overlap is rejected.

### Generated

The application creates a reviewed output such as a descriptor, adapted AGENTS, or preference state.

## Destination ownership

- `managed`: remote file, replace only when local matches installed hash
- `merged`: base and result recorded for future three-way or structured merge
- `generated`: recreated from reviewed state with preview
- `external`: outside project through an explicit adapter and backup

## Hash model

SHA-256 is the content-integrity source. Git blob SHA may be recorded as additional evidence. The plan stores source revision, source path, source hash, size, destination, local hash, previous base hash, and deterministic result hash when available. Interrupted blob reads retain only a bounded partial cache entry; a validated range retry may resume it, and no partial bytes enter staging.

## Path security

Reject absolute project destinations, parent traversal, reserved Windows names, Unicode identity collisions, links that escape the root, case collisions, alternate data streams, and two sources mapping to one destination.

## Platform support

A component supports `windows`, `macos`, or `all`. `all` requires verified command variants for both platforms. Current evidence supports platform-neutral text components, but the current `hoi4-agent-tools.cmd` and 3D wrappers are Windows-specific.

## Tools and environment

A tool declaration includes required state, version policy, version when pinned, user-visible commands, and health checks. A secret environment variable must use OS credential-vault storage and cannot request project-file storage.

## Validation

Built-in declarative validators include exists, SHA-256, JSON Schema, TOML parse, localisation BOM, directory coverage, approved command health check, and named application validators. Repository scripts remain high-impact external actions.

Command-bearing validation rules are copied into the typed installation plan as
high-risk external actions. The dry run shows the manifest target and approval
requirement; the core does not accept a renderer-supplied executable or command
line. The package-backed Windows MCP route is an explicit exception to static
Node hashes because the reviewed current-user Node LTS build can vary: the
manifest instead binds exact npm integrity, the full canonical installed
package tree, runtime entry, and tools. Rust requires a valid OpenJS Foundation
signature, captures the local Node hash, and rechecks it at spawn. The wrapper
is never executed.

The closed `validation.parameters` object admits the paired
`executable_sha256`/`executable_size`, `interpreter_sha256`/`interpreter_size`,
and `runtime_sha256`/`runtime_size` identity fields plus package name, version,
registry integrity, canonical package-tree SHA-256/file count, runtime entry,
required tools, bounded `arguments`, `network_access`, `expected_writes`,
`privilege`, and `rollback_boundary` action evidence. Draft 2020-12 validation
rejects any other parameter, while deterministic Rust validates each declared
identity before planning or execution.

If a manifest does not provide immutable executable identity evidence, the
action remains visible in the dry run but the health route is
`planned_unavailable`; the core never runs a same-named command found on `PATH`.
Installed readiness may detect that the exact configured command exists on the
reviewed PATH and display it as configured, but discovery alone never executes
or authorizes the command.
The current HOI4 MCP declaration provides the complete package-backed evidence
above. Missing or mismatched evidence blocks the route before process start.

## Update behavior

Each component declares replacement strategy, obsolete-file handling, local-addition policy, and repository-script ownership. Removed files are deleted only when the lock proves sole ownership and current content is unmodified.

## Published-manifest binding

The upstream manifest is now committed and published on the approved source
repository's `main` branch. Its `generated_for_revision` field records the
tracked source snapshot used to produce the evidence. That field may precede
the publication commit because a Git commit cannot contain its own final hash:
changing the field changes the commit hash again.

Evidence generation requires an exact lowercase 40-character revision and
reads that commit's raw blobs with `git ls-tree` and `git cat-file --batch`.
It never hashes worktree or `git archive` output, so line-ending conversion and
checkout attributes cannot change the declared bytes.

Latest and pinned resolution still fetch the manifest and every selected blob
from one exact resolved commit. The resolver validates that the manifest's
generation revision is an immutable commit-shaped value, and the download
layer independently verifies every selected file's declared size and SHA-256
against bytes from the resolved commit. A stale or incorrectly regenerated
manifest therefore fails closed at selective download; no mixed-revision
install is accepted. The lock records the resolved source revision, manifest
hash, manifest origin, and per-file hashes. The bundled manifest remains a
versioned offline bootstrap for already-supported packaged revisions, but it
is never substituted for a newly resolved remote commit.

## Compatibility

- unknown major schema blocks
- unknown optional component can be shown unsupported
- removed IDs remain recognized for migration and uninstall
- pinned commit cache is immutable after hash verification
- latest branch metadata is re-resolved for each source request; ETag caching is
  not claimed by the current implementation

The runtime validates `generated_for_revision` as immutable provenance and binds
the selected manifest and source files to the resolved commit. Dependency
resolution exposes reverse-dependent impact for update and removal review, and
an all-platform command declaration is represented for the current supported
platform instead of being silently dropped.
