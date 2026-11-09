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

Latest is the default for new setups and for the explicit existing-project
Update target. Commit and release pinning remain optional. The installed lock
changes only after reviewed apply; Repair uses the recorded immutable source.

1. Query repository metadata.
2. Read the current default branch.
3. Resolve it to a commit SHA.
4. Fetch the manifest at that exact commit.
5. Parse the raw JSON and validate it against
   `docs/schemas/remote-manifest.schema.json` using Draft 2020-12 before typed
   deserialization. Then validate runtime trust-policy invariants, file
   evidence, and the declared generator revision.
6. Expand only the selected component closure. Public installation uses each
   selected component's manifest-declared `expected_files` inventory; it does
   not need a recursive repository tree to infer files.
7. Fetch selected files.
8. Verify each declared size and SHA-256.
9. Record commit, manifest hash, manifest origin, platform, sizes, file hashes,
   the manifest's exact `wiki.required_pages` list, and the manifest-declared
   wiki snapshot/media/provenance/license metadata.

Latest mode becomes reproducible after installation because the lock records the
exact commit. A bounded in-memory resolution cache may reuse a complete
resolution for at most 15 minutes (or the manifest's lower declared TTL), but a
cache entry remains bound to the request, resolved revision, manifest bytes,
and branch. It is an optimization, not a source of identity.

When the unauthenticated GitHub REST quota returns a rate-limit-only 403/429,
the fixed public repository may resolve identity from the read-only Git HTTP
`info/refs?service=git-upload-pack` advertisement. This fallback receives ref
names and commit IDs only; it never clones, downloads Git objects, or searches
the computer. Pinned commits and releases still require commit/release-page
identity confirmation. An unrelated 403 fails closed.

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

Use GitHub metadata, commit, raw, and release endpoints for public
installation. A recursive tree endpoint may remain available to developer-only
local-source fixtures and compatibility tests, but it is not the public file
inventory or an authorization to fetch a whole repository. Never run `git
clone` for Agentic-HOI4-Modding. A repository-declared external dependency
script may use Git for its own dependency only after dry-run disclosure and
approval.

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

A selected tree component is still a manifest concept, but its published
`expected_files` list is the installation inventory. Every listed file receives
a plan row and SHA-256; the installer downloads only those declared blobs. A
recursive tree listing is not required for public resolution and cannot add
undeclared files to a plan.

#### One archive per install

When many declared files are not yet in the verified blob cache, the desktop
app first downloads the exact commit once from
`https://codeload.github.com/klimPaskov/Agentic-HOI4-Modding/tar.gz/<commit>`
instead of about 1,700 single-file requests. The download streams to a partial
file under the cache (resumed with a `Range` request when the server allows it,
with the same bounded retries as other source requests, and capped at 1 GiB
compressed and 4 GiB declared). The extractor never uses an archive path on the
filesystem: it looks each entry up by its exact manifest path, accepts only
regular files whose size and SHA-256 match the manifest, and stores them in the
same hash-addressed cache. Links, special files, undeclared paths and mismatched
content are skipped. Anything the archive did not supply, and every file when the
archive fails or is refused, is fetched by the per-file route as before, so a
bad archive can only cost time. A developer's local source checkout never uses
the archive. The web edition is unchanged.

Disjoint tree components may share a standard managed destination directory such as `.agents/skills/` or `.codex/agents/`. Every selected concrete destination remains unique and is canonicalized before download; any selected overlap is rejected.

### Generated

The application creates a reviewed output such as a descriptor, adapted AGENTS, or preference state.

### Release binary image provider

An image provider whose `route.kind` is `release_binary` installs one program
from a per-platform GitHub release instead of repository files. The route
declares `release_binary` beside the usual route fields (`auth_method` is
`chatgpt_login`, `component_id` is `imagegen.hoi4_imagegen`, `server_id` is
`hoi4_imagegen`, `package_version` equals the release version):

```json
"release_binary": {
  "version": "0.2.0",
  "install_root": ".tools/image_generation/hoi4-imagegen",
  "targets": {
    "x86_64-pc-windows-msvc": {
      "url": "https://github.com/klimPaskov/hoi4-imagegen-mcp/releases/download/v0.2.0/hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc.zip",
      "sha256": "<64 lowercase hex>",
      "size": 12345678,
      "archive": "zip",
      "entries": {
        "mcp": "hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc/hoi4-imagegen-mcp.exe",
        "cli": "hoi4-imagegen-v0.2.0-x86_64-pc-windows-msvc/hoi4-imagegen.exe"
      }
    }
  }
}
```

Targets are `x86_64-pc-windows-msvc` (`zip`), `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` and
`aarch64-unknown-linux-gnu` (`tar.gz`). A target may be omitted; its platform
is then reported as unsupported. Validation is strict and the app derives the
only acceptable `url` from the version and target, so the repository must be
`klimPaskov/hoi4-imagegen-mcp`, the host `github.com`, the path a release
download of `v<version>`, with no query, port or credentials. The component
`imagegen.hoi4_imagegen` is a `generated` component with no `expected_files`;
it exists so selection, dependency and readiness handling treat the provider
like any other image component. Both format 5 and format 6 schemas accept the
route.

The app downloads only the declared URL (following at most four redirects to
`objects.githubusercontent.com` or `release-assets.githubusercontent.com`),
checks size and SHA-256 before it opens the archive, refuses an archive with
any traversal, absolute, link, encrypted or oversized entry, extracts only the
two declared entries, and installs them in
`.tools/image_generation/hoi4-imagegen/<version>/` (executable on macOS and
Linux). It then runs `hoi4-imagegen --version` and requires the declared
version. An update installs the new version beside the old one, retargets the
client entries, and removes the old version only after the check passed and
only while its files still hash to what the lock recorded. The retired
`local_package` GPTImage route (`imagegen.gptimage`, `hoi4_image_gptimage`) is
never installed; an older manifest that lists it is accepted and its provider
is ignored with a message.

#### Backends of the hoi4-imagegen route

The image step offers hoi4-imagegen and, beside it, "None, I make art
myself". The backend list comes from the route, so a backend the program gains
appears in Setup without an app release. The route may carry an optional
`backends` array (schema formats 4 to 6):

```json
"backends": [
  { "id": "chatgpt_login", "display_name": "ChatGPT sign-in", "default": true,
    "replaces_providers": ["codex_native"] },
  { "id": "openai", "display_name": "OpenAI", "credential_field": "openai_api_key",
    "replaces_providers": ["openai_api"] },
  { "id": "comfyui", "display_name": "ComfyUI", "credential_field": "comfyui_api_key",
    "settings": [
      { "key": "providers.comfyui.endpoint", "kind": "url", "required": true,
        "default": "http://127.0.0.1:8188" },
      { "key": "providers.comfyui.workflow", "kind": "project_path", "required": false }
    ],
    "replaces_providers": ["comfyui_local"] }
]
```

- `id` is the lower-case name hoi4-imagegen accepts for the backend. Setup
  records the choice as `route_settings.hoi4_imagegen.backend` and validates it
  against this list.
- `default` marks the backend used when none is chosen (exactly one; the
  ChatGPT sign-in). It is listed first.
- `credential_field` names an entry of the route's `credential_fields`. Its
  key is stored in the operating-system credential store and reaches only the
  hoi4-imagegen process. A backend without one (the ChatGPT sign-in) takes no key.
- `settings` lists values the backend reads from the project's
  `.hoi4-imagegen/config.toml`. `key` is the dotted TOML path (it starts with
  `providers.`, so `policy`, `profiles` and `templates` are never touched) and
  `kind` is `url` (https anywhere, or http on this computer, with no user
  name), `project_path` (a `.json` path inside the project) or `text` (letters,
  digits and `. _ : / -`, such as a model name). A `required` setting without a
  `default` must be filled in before setup continues. Setup asks for
  each one beside the backend and writes it there.
- `live_verified: false` marks a backend not yet verified against the real
  service; the picker shows an "untested" hint.
- `replaces_providers` lists older provider rows the backend covers. Update and
  repair offer to move a project that records one of them, and the stored key
  of a row with one key moves to `credential_field` without being shown.

A package that omits `backends` still works: Setup derives the list from
`credential_fields` (one backend per key, the ChatGPT sign-in as the default),
and maps an older provider onto the backend whose key is read from the same
environment variable. A credential field must be a lower-case id that, in upper
case, is a variable name ending in `_KEY` or `_TOKEN` (not an Anthropic,
Meshy, Claude or Codex name), so a package can name a new backend's
key without being able to name any other variable.

When the user picks a backend, Setup merges it into the project's
`.hoi4-imagegen/config.toml` (a reviewed, `generated` file; `merged` when the
project already had one): it sets `[defaults] provider = "<id>"` and the
chosen backend's `settings`, and keeps every other table and key (profiles,
policy, templates). It writes nothing when the project records no backend
choice, because hoi4-imagegen then picks the first ready provider itself, and
an explicit choice of the default is written as `provider = "chatgpt_login"` so
an earlier choice cannot linger. If the backend is later cleared or
hoi4-imagegen is deselected, the file is removed only while it still matches
what Setup wrote.

An older provider that no backend covers yet keeps working exactly as the
project recorded it; Update and repair never remove or change it.

## Destination ownership

- `managed`: remote file, replace only when local matches installed hash
- `merged`: base and result recorded for future three-way or structured merge
- `generated`: recreated from reviewed state with preview
- `external`: outside project through an explicit adapter and backup

## Hash model

A downloaded file whose bytes are an unresolved Git LFS pointer (they begin with `version https://git-lfs.github.com/spec/v1`) is rejected during download verification even when its SHA-256 matches the evidence, because the evidence then describes the pointer instead of the declared file. The source manifest generator fails closed on the same condition.

SHA-256 is the content-integrity source. Git blob SHA may be recorded as additional evidence. The plan stores source revision, source path, source hash, size, destination, local hash, previous base hash, and deterministic result hash when available. Interrupted blob reads retain only a bounded partial cache entry; a validated range retry may resume it, and no partial bytes enter staging.

## Path security

Reject absolute project destinations, parent traversal, reserved Windows names, Unicode identity collisions, links that escape the root, case collisions, alternate data streams, and two sources mapping to one destination.

## Platform support

Manifest major 5 adds `linux` to `windows`, `macos`, and `all`. `all` covers all supported platforms; command routes still require explicit platform evidence. HOI4 Agent Tools declares platform-specific launcher names and immutable package-tree pins for Windows and Linux. macOS keeps its unsupported runtime state; 3D remains Windows-only. Older manifest majors 2, 3, and 4 remain readable. Linux core routes must use the declared native launcher and current-user XDG npm prefix, never an inferred `.cmd` translation.

Manifest major 7 adds an optional `macos` entry to the HOI4 Agent Tools health action's `platform_targets` (`hoi4-agent-tools`) and `platform_pins`. Format 6 forbids additional keys there, so a format 6 reader could not accept them. On macOS the source bootstrap installs a private official Node.js at `~/Library/Application Support/hoi4-mod-setup/npm/node/bin/node`. The app accepts it only with the Node.js Foundation Developer ID signature (Team ID `HX7739G8FX`). Client registrations start that runtime with the package entry under the same prefix, as on Linux. Older majors 2 to 6 remain readable.

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

- unknown major schema blocks; a major newer than the app reads is reported as
  needing an app update before any schema is applied
- format 6 adds `mcp_support`, `mcp_registration`, and `documentation_url` to
  each coding environment and `shared_coding_environments` to components (see
  `docs/36_coding_environment_catalog.md`); format 5 schemas reject them, and
  a format 5 manifest offers only the five clients it shipped
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
