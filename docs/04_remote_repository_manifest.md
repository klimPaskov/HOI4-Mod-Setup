# Remote repository and manifest design

## Purpose

The remote manifest is the installation contract between Agentic-HOI4-Modding and HOI4 Mod Setup. The proposed discovery path is:

```text
hoi4-mod-setup.manifest.json
```

A release may later provide the same manifest and a compact file index as signed assets.

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

The complete schema is `schemas/remote-manifest.schema.json`.

### `workflow.super_events`

The verified manifest defines `workflow.super_events` as an optional,
provider-neutral component for a complete reusable Super Events package. The
parent installs the selected-only `hoi4-super-events*` skill packages and three
narrow research subagents, then expands to hidden dependency components for
`interface/`, `common/`, `events/`, `localisation/`, `gfx/`, and
`docs/super_events/`. These dependencies install the working GUI,
GFX declarations, scripted GUI and registration effect, dynamic text and image
selectors, one console-test example, default localisation, DDS assets, and
editable Photoshop templates.

All runtime text is deterministically adapted to the confirmed project
namespace after its source bytes are verified. Binary assets are copied without
text adaptation. The package declares no tool, environment variable,
credential, external command, or health action, and audio remains an optional
later project addition with separate source and rights evidence. Every managed
component uses `replace_if_unmodified`, with obsolete-file removal and local
additions preserved according to the manifest.

The core skills selection must exclude `hoi4-super-events*/**`, the core
subagent selection must exclude `hoi4_super_event_*.toml`, and every hidden
runtime dependency remains optional, so an install that declines the workflow
cannot receive any Super Events file accidentally. The adapted `AGENTS.md`
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
line. After installation, an optional workflow check re-resolves the manifest at
the lock revision and verifies the installed target's SHA-256 and size before
starting its manifest-derived route. A wrapper route also requires immutable
size and SHA-256 evidence for every command interpreter and runtime dependency
that the core resolves (currently `cmd.exe` and Node on the Windows MCP route)
and rechecks the interpreter identity immediately before spawn.

If a manifest does not provide immutable executable identity evidence, the
action remains visible in the dry run but the health route is
`planned_unavailable`; the core never runs a same-named command found on `PATH`.
The current HOI4 MCP declaration is in this honest state because the inspected
source provides no executable, interpreter, or runtime hash/size evidence,
package identity, or version evidence.

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
