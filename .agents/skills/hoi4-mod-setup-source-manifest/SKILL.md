---
name: hoi4-mod-setup-source-manifest
description: Use for GitHub source resolution, remote manifest design, selective downloads, component dependencies, checksums, cache behavior, wiki distribution, MCP declarations, or latest and pinned mode changes.
---

# Source, manifest, and selective download

## Required sources

Read:

- `AGENTS.md`
- `docs/04_remote_repository_manifest.md`
- `docs/05_wiki_installation.md`
- `docs/09_component_dependency_model.md`
- `docs/11_mcp_setup.md`
- `docs/31_ai_provider_profiles_and_chat_sources.md`
- `docs/schemas/remote-manifest.schema.json`
- `docs/source-manifest/hoi4-mod-setup.manifest.json`

Inspect the live Agentic HOI4 Modding repository when a path, package, command, platform declaration, or dependency may have changed. Do not rely on memory.

## Trust sequence

Latest mode:

1. Resolve the repository default branch.
2. Resolve that branch to one exact commit.
3. Fetch the manifest at that commit.
4. Parse the raw JSON as a value and validate it against the checked-in Draft
   2020-12 schema before typed deserialization; then validate the supported
   major version and runtime trust-policy invariants.
5. Expand selected component dependencies.
6. Resolve only manifest-declared files or bundles at the same commit.
7. Download into immutable cache or transaction staging.
8. Verify size and SHA-256.
9. Record source identity in plan and lock.

If unauthenticated GitHub REST returns a rate-limit-only 403/429, resolve
identity through the fixed repository's read-only Git
`info/refs?service=git-upload-pack` advertisement. Parse only bounded ref names
and commit IDs; pinned commits and releases additionally require commit and
release-page identity checks. This fallback never downloads Git objects, runs a
checkout, or searches for a local repository. Any unrelated 403 fails closed.

Resolution results may be reused only through the bounded in-memory cache: the
request key, resolved revision, branch, manifest bytes, and manifest evidence
stay together, with a maximum lifetime of 15 minutes (or a lower manifest TTL).
This is an optimization and never source identity.

Pinned commit uses the supplied commit for every step. Pinned release resolves and records both release identity and exact commit when available.

Latest is the new-setup and explicit Update-target default. Maintenance may
retarget a pinned lock only through the reviewed `updateSource` selection;
legacy callers that omit it retain the lock policy. Bind reanalysis's
`update_source` to the same resolved revision and manifest hash before any
component download. Reject a moved same-name release, but allow an explicitly
chosen different release, commit, or Latest. Repair/reinstall reject overrides.
All selected MCP actions use the incoming manifest, including Live Tools without
Agent Tools. Record reviewed package versions in the resulting lock; never
resolve an independent registry `latest` or silently change the source on repair.

## Hard rules

- Never clone the complete source repository through the application.
- Never use the Git ref-advertisement fallback for object downloads or a
  checkout; it is names-and-commit-IDs-only metadata.
- Never search the computer for a checkout.
- Never mix revisions.
- Never use a branch name as lock identity.
- Never trust archive paths before safe extraction checks.
- Never accept duplicate destinations or paths outside the target root.
- Never invent missing source, license, package, command, MCP, tool, environment, or platform metadata.
- Never hide unsupported optional components.
- Isolate downloads to the selected component IDs plus their manifest dependency
  closure. An unselected component's tree, files, commands, and hashes must
  not enter the plan, cache, staging area, or lock. `workflow.super_events`
  owns the user-visible selection and expands to optional hidden runtime
  components for its skill, interface, common scripts, event, localisation,
  GFX/templates, and guide only when selected.
- Ignore `.gitkeep` marker files even if a future selected source tree declares
  them; they never enter download, destination, staging, lock, or flattened
  output evidence.
- Treat each selected component's manifest-declared `expected_files` inventory
  as the public installation file set. Do not require or infer a recursive
  repository tree; every selected entry still needs declared path, size, and
  lowercase SHA-256 evidence.

- Reject an unresolved Git LFS pointer in `verify_download` even when its
  hash matches: manifest evidence hashes committed Git blobs, so a pointer
  passes the checksum but installs a placeholder. Map it to the dedicated
  "placeholder instead of a real file" plan error. The source generator
  (`scripts/generate_manifest_evidence.py`) fails closed on the same prefix.

## Manifest change workflow

1. Update schema and examples.
2. Add a migration or compatibility rule if the manifest model changes.
3. Validate component dependency cycles and reverse dependencies.
4. Test platform resolution separately from component selection.
5. Test hostile paths, redirect responses (which must be rejected), truncation,
   cache corruption, and hash mismatch.
6. Test default branch change between requests.
7. Test latest and pinned reproducibility.
8. Update source audit and docs.

The checked-in manifest fixture is bound to
`docs/source-audit/current_manifest_identity.json`: record its exact published
commit, generator revision, raw byte hash and size, Git blob, and release.
Keep historical repository inventories separate; do not rewrite their old
observations or use them to validate a newer manifest fixture. The source
repository still owns generation of every component's file evidence.

## Wiki rules

Install the manifest-declared wiki tree at `<mod_project>/paradox_wiki/`. Validate required page coverage, hashes, path containment, case collisions, media policy, and update behavior. Report missing formal provenance or license data exactly as missing.

## MCP and external tools

For a repository command whose target names Python, render its declared
`script_path` as the first reviewed argument. Keep the interpreter in the
process adapter; never execute it as a substitute for the installed script.
The app-owned allowlist, operation hash, source size and full command arguments
must still pass before execution. Cover this with a manifest-backed plan test
and a real scratch installation.
Multi-platform actions select the current supported platform regardless of
declaration order. Preserve unsupported declarations as unsupported evidence;
never translate a Windows command into a new route by inference.

MCP servers and external dependencies are components. Their command, arguments, tools, environment variable names, health checks, supported platforms, and update behavior come from verified repository evidence. A similar command on another platform is not support evidence.

For npm runtimes, verify that the exact published artifacts pin the complete
transitive runtime, not only the top-level version. Reproduce the declared
package-tree identity with a clean install into a disposable prefix. A tree
hash captured from mutable transitive resolution can become uninstallable;
fix it upstream with a bundled runtime or integrity-complete pinned closure.
Never bless the current machine's changed hash or relax installed-tree checks.

The HOI4 Agent Tools bootstrap owns a private per-platform npm toolchain, pinned
by SHA-512 inside its source-verified script. npm majors can realize different
optional-platform closures from the same shrinkwrap. Verify each supported
Node/platform combination without changing existing MCP tree hashes, isolate
npm configuration and cwd, and never replace the user's ambient npm executable.
Publish changed script evidence through the source-owned generator and consume
that immutable manifest commit; old pinned repairs must not silently substitute
a newer bootstrap.

## Repeatable MCP package verification

- Run `pnpm test:scratch-live` with an explicit committed source checkout to
  exercise real installations with Claude Code and Codex as primary in turn.
  It uses isolated profiles, verifies all selected skill projections and both
  clients' registrations, and starts Agent Tools and GPTImage for initialize
  and tools/list. Only setup analysis is a fixture. It makes no paid tool calls.
  Test other selected providers separately with their prerequisites available.

- Read exact package/version, integrity, complete tree hash/count, entry
  hash/size, and required routes from the canonical manifest's MCP health rule.
- For an unpublished candidate, use a reviewed local `npm pack` artifact and
  set `HOI4_MCP_LOCAL_TARBALL` to its absolute path when running
  `node scripts/test_live_mcp.mjs`. The script checks exact SHA-512, stages the
  verified bytes, and skips only the registry integrity lookup. Full installed
  tree, entry, optional npm lock, negotiated version/protocol, and tool checks
  still run. Never infer package identity from the filename.
- Run `node --test scripts/mcp_local_tarball.node-test.mjs` for the opt-in
  boundary and Rust MCP/source/commands tests for route and manifest evidence.
- The registry smoke bounds a cold Windows npm installation at ten minutes;
  short npm metadata operations retain their two-minute limit. Timeout errors
  name the stage. On a timeout, inspect the stage and retry from the script's
  fresh isolated prefix; never change measured tree or entry evidence to pass.
  The live initialize/tools-list probe also allows two minutes for a cold
  verified runtime and reads its client version from `package.json`.
- Keep default registry smoke fail-closed for an unpublished version. Before
  release, replace local-pack integrity with npm `dist.integrity`, recapture a
  clean published-package tree and entry, regenerate source-owned evidence,
  copy the committed manifest blob exactly, and pass the default smoke test.
- Copy source manifests through `git cat-file` at the immutable publication
  commit. Compute both audit digest fields from that same blob, preserving LF
  bytes; never substitute checkout bytes or edit generated evidence by hand.
- Keep bundled source JSON artifacts marked `-text` in `.gitattributes`.
  Checkout or index newline conversion must never change audited upstream
  bytes; verify the committed blob and worktree against the same audit digest.
- Current comparison behavior belongs to `hoi4.tech_inspect` mode `compare`.
  Removed route names cannot substitute for current inspectors in health tests.

## Current implementation boundaries

- The application consumes the single canonical `hoi4-mod-setup.manifest.json`
  route from the approved GitHub source at one verified commit. The manifest's
  required `schema_version` field owns compatibility; unsupported majors fail
  closed without creating parallel filename routes. The local evidence generator is
  `scripts/generate_manifest_evidence.py`; it accepts only an explicitly
  supplied source root and must not discover checkouts.
- Manifest generation requires an exact lowercase 40-character revision and
  reads bytes from that revision with `git ls-tree` plus `git cat-file
  --batch`. Never hash worktree or `git archive` output because checkout
  attributes and line-ending conversion can change bytes. The generator fails
  when a declared path is absent from the selected commit.
- Apply the same immutable-blob rule to source-audit manifest digests. Record
  the published manifest's Git bytes, not a source checkout's converted bytes,
  and keep the bundled LF bytes identical. The source contract test compares
  both audit digest fields with the included manifest before release.
- The explicitly configured local source validates each raw route's immutable
  commit and reads with `git show <commit>:<path>`; it does not re-resolve every
  raw file through `rev-parse`. Source verification uses an isolated cache and
  bounded prefetch, and fails when the checkout is not supplied.
- `HttpSourceClient::resolve_commit` uses GitHub REST first and falls back only
  for a rate-limit-only 403/429 to the fixed repository's bounded
  `info/refs?service=git-upload-pack` advertisement. The parser binds `HEAD`,
  the advertised default branch, and peeled release tags, while commit-page
  checks confirm pinned commit/release identity. It never invokes clone,
  fetch, object download, or checkout, and rejects another repository,
  service, redirect, or unrelated 403.
- Every HTTP request uses a no-redirect client. The URL allowlist binds API and
  raw routes to the fixed `SOURCE_OWNER`/`SOURCE_NAME`; raw content requires an
  exact 40-character commit segment before the path. Do not follow a redirect
  or treat its destination as source identity; request route and resolved
  revision must remain explicit for every manifest/blob fetch.
- Runtime parsing compiles and applies
  `docs/schemas/remote-manifest.schema.json` as Draft 2020-12 before
  deserializing `RemoteManifest`. Unknown fields, including nested policy
  fields, fail closed; handwritten validation remains the second layer for
  cross-field and trust-policy invariants.
- A non-generated component is not downloadable unless every selected file has
  lowercase SHA-256 and size evidence. Public selection uses
  `select_manifest_files` and each component's `expected_files`, not a
  recursive repository tree; outside, duplicate, missing, or undeclared
  evidence fails before any blob is fetched. The published live-source
  manifest records the tracked snapshot used to generate its evidence; the
  runtime binds the manifest and all selected blobs to the one resolved commit
  and rechecks those hashes.
- `select_component_files` walks only the supported selected-component closure,
  applies each component's include/exclude rules, and rejects missing evidence
  or destination collisions before any selected blob is fetched. This is
  selective manifest isolation, not a filtered full-repository clone.
- Multiple tree components may target the same managed directory only so disjoint selected-only packages can share the canonical `.claude/skills/` or `.claude/agents/` roots, or generated client roots such as `.agents/skills/` and `.codex/agents/`. The selected-file pass still canonicalizes every concrete destination and rejects any overlap before download or staging.
- A selected component may have a deterministic project adaptation layer after source verification. Keep the upstream source path and checksum evidence unchanged, adapt only the reviewed destination/result bytes, and reject collisions among adapted destinations. For Super Events, the verified `hoi4ms_*` runtime source basenames are written as the confirmed project-prefix basenames; binary GFX/PSD assets keep their manifest destinations.
- Marker-based text adaptation first checks for its marker pair in the raw bytes; source blobs without those markers, including binary assets, pass through unchanged and are never decoded as UTF-8.
- An immutable install requires `generated_for_revision` in the manifest; the runtime schema and validator reject a manifest that cannot prove which commit produced its evidence.
- Verified blobs are cached under the application data cache by `<revision>/<sha256>` and are accepted only after size (when declared) and SHA-256 revalidation. A corrupt cache entry is discarded and fetched again.
- Complete source resolutions have a separate bounded memory cache keyed by the
  serialized source request. It expires entries no later than 15 minutes,
  retains exact revision/branch/manifest evidence, and never mixes revisions;
  the hash-addressed blob cache remains independently verified.
- Cache reads bind the path to one no-follow file handle, read a bounded byte
  buffer once, and verify that exact buffer before returning it. Never hash one
  path lookup and then reopen the path to return different bytes.
- Verified blob downloads retain only a bounded `.part` cache entry after an interrupted read; a retry may use a validated HTTP range, and the hash-addressed cache is promoted only after the complete size and SHA-256 checks pass. Partial bytes are never staged.
- Manifest paths must use canonical slash-separated spelling (with an optional directory trailing slash); collision keys use Unicode NFC plus case folding so composed and decomposed macOS names cannot map to two managed destinations.
- The runtime rejects manifests whose wiki provenance/license status is outside the schema enums, whose latest policy does not require default-branch resolution plus recorded commit evidence, or whose signing policy requires verification that the build does not implement. Pinned commit/release modes are checked against the manifest's explicit allow flags.
- Release tags are resolved through typed GitHub objects, including annotated-tag dereferencing, and pinned revisions are verified as commit objects before manifest or file access.
- The MCP component is optional, verified on Windows and Linux, and provider-neutral; it depends on its verified bootstrap rather than `codex.config`. The setup-assistant choice does not change component selection or generated Codex TOML. Installed `codex.config` and MCP declarations independently control their later-development integrations. macOS retains an explicit unsupported state and never receives a substitute command. Its package-backed route requires exact npm integrity, canonical full-package-tree SHA-256/file count, runtime-entry identity, and required tools. The wrapper locates the current-user npm prefix but is never executed. On Windows require an OpenJS Foundation-signed Node executable; Linux requires a root-owned, non-writable, dpkg-verified native tool. Capture its local SHA-256 and recheck it at spawn. Linux uses explicit platform_targets and platform_pins, the XDG current-user npm prefix, and only the two source-pinned contained npm symlinks; all other links fail closed. Any missing evidence blocks execution. The offline wiki is always rooted at `paradox_wiki/`; the plan and lock copy the exact manifest `wiki.required_pages` list plus snapshot/media/provenance/license metadata for the resolved revision, and readiness blocks legacy locks that lack that evidence instead of using a newer bundle.
- The MCP health probe requires exact protocol negotiation, an advertised
  server version matching the reviewed package version, a `tools` capability,
  and a `tools/list` result containing every declared
  route. It executes only a private copy materialized from the verified
  package-tree bytes, so a global-package mutation after verification cannot
  change the entry that runs. Keep package inspection bounded to 32 MiB per
  file, 256 MiB total, and 10,000 files. A hidden npm lock is additional
  integrity evidence when npm emits it; full manifest integrity and exact
  package-tree identity remain required regardless.
- The selected Windows 3D component may publish its exact npm lock and runtime
  identity dynamically, but the credential-bearing Codex route is rendered by
  the app as the absolute installed HOI4 Mod Setup executable plus the one
  fixed Meshy CLI marker. Never preserve or generate a project batch/Python
  credential launcher from source content.
- `generated_for_revision` is required immutable provenance but may precede the publication commit because a manifest cannot contain the final hash of the commit that contains it. The resolver consumes the remote manifest at the resolved commit, and selective download fails closed when its per-file size or SHA-256 evidence does not match that same commit. A bundled bootstrap is never substituted for a newly resolved remote commit.
- For unpublished package validation, `HOI4_MOD_SETUP_SOURCE_REPOSITORY` must
  name an absolute committed Agentic HOI4 Modding checkout. The local source
  adapter resolves its default branch and exact commits through Git, and the
  ignored `local_source_serves_every_declared_file` test uses a temporary
  `HttpSourceClient::with_cache_root` so verification does not depend on an
  ambient application cache.
- The Agentic source owns `scripts/generate_manifest_evidence.py` and
  `.github/workflows/publish-hoi4-mod-setup-manifest.yml`. Relevant source
  changes regenerate exact path/size/SHA-256 evidence for declared component
  trees, validate generator and 3D contracts, verify the pushed `main` revision
  is still current, and publish the manifest in a follow-up commit. Never edit
  generated file evidence by hand or make the app search a local checkout.
- Treat component/profile IDs as manifest data, not an app enum. New setup uses
  the resolved default profile. Update compares the installed manifest profile
  with the newly resolved one and adds only new, provider-compatible,
  platform-supported defaults; it preserves prior optional deselection. New
  files under declared skill/subagent trees flow through automatically, while
  a new component still requires an explicit source-owned declaration.
- Treat image-generation provider and model catalogs the same way: the
  manifest owns provider IDs, model lists, route component IDs, capabilities,
  and credential fields. Newly published catalog entries are consumed from the
  exact manifest revision without adding per-model application enums or schema
  migrations; Rust dependency expansion and route validation remain the final
  authority.
- Image-generation and live-testing provider `supported_coding_environments`
  are client-ID patterns in format 6, never a fixed enum. The source lists
  only clients whose project MCP file accepts a working directory
  (`MCP_ROUTE_CLIENTS`), and the format 6 candidate fixture test requires
  that list to equal the registry's `image_and_live_routes` clients.
- Pin app fixtures and remote acceptance only to a source commit whose
  manifest `generated_for_revision` is its parent and which changes nothing but
  the manifest. Source validation compares evidence with the generated
  revision, so a later commit that edits an installed file still passes there
  but fails the app's download hash check. Take the regenerated manifest from
  that commit's `hoi4-mod-setup-manifest-preview` artifact.
- Coding-client packages are also manifest data. Components tagged with
  `coding_environment` form the complete closure for exactly one primary and
  any additional environment selected by the user. Keep environment packages
  composable with workflow profiles, and include native instructions, settings,
  MCP configuration, maps, and generated agent projections with complete file
  evidence. A future component under an existing supported environment must be
  consumable from its exact manifest revision without an app release.
- Keep every non-optional coding-environment component free of optional
  workflow projections. Represent a runtime/workflow intersection as an
  optional `coding_environment` component whose optional dependencies name the
  canonical selected workflow components. The app activates it only when all
  of those optional dependencies are in the selected closure. Use the same
  pattern for platform-specific MCP registrations; never place a Windows
  `.cmd` command in an all-platform source component.
- Manifest format 6 adds per-client MCP declarations and shared components
  (`docs/36_coding_environment_catalog.md`). Each environment declares
  `mcp_support` (`project`, `manual`, or `none`); a `project` route names the
  owning `component_id`, its exact `path`, and a closed `format`
  (`json_mcp_servers`, `vscode_servers`, `local_command_array`,
  `toml_mcp_servers`). `validate_environment_mcp_declarations` rejects a route
  whose component belongs to another client or does not install that path.
  The app offers a format 6 client only when the `NATIVE_CLIENTS` registry in
  `src-tauri/src/coding_environment.rs` has a matching path and format, or when
  the route is manual or absent; anything else is listed as unavailable with
  a reason, and `component_ids` refuses it with an `InvalidInput` message that
  the UI shows unchanged. Format 5 and older manifests offer only the five
  clients they shipped.
- `shared_coding_environments` lets one component serve several clients from
  the same path (the `.agents/skills` copy serves Codex, Gemini CLI,
  Antigravity, Kilo Code, and Devin Desktop). It requires an owning
  `coding_environment`, names only other declared clients, and needs format 6.
  Use `coding_environment::serves_environment` wherever selection or
  maintenance asks which components belong to a client; never declare a second
  component for the same destination. Cross-client `dependencies` alone are
  not enough, because maintenance compares unexpanded per-client component
  lists against the lock.
- Adding a client with an existing MCP format needs a source declaration and a
  registry row (path, format, layout, markers, scan folders, documentation
  URL). A manual or absent route needs only the source declaration. A new MCP
  format needs app code in `crates/hoi4-setup-core/src/native_mcp.rs` and therefore an app
  release. Bumping the manifest major makes older releases refuse the
  canonical manifest, so merge a format bump in the source only together with
  the app release that reads it.
- The source repository owns the client table in
  `scripts/coding_environment_data.py`; the evidence generator applies it and
  `validate_published_manifests.py` keeps the table and generated declarations
  exact. A source pull request cannot publish evidence: its validation job
  uploads a `hoi4-mod-setup-manifest-preview` artifact built by the owned
  generator for the head commit, and only that output may be committed as the
  branch's manifest.
- The canonical authoring source is `CLAUDE.md`, `.claude/skills/**`, and
  `.claude/agents/*.md`. Source-side synchronizers generate the adapted
  `AGENTS.md` plus every other client's agent projections; `--check`
  must fail on missing, stale, extra, or drifted projections before a manifest
  is published.
- Expected file evidence carries both SHA-256 and byte size into selected-file records, operations, locks, and readiness. Command-bearing validation rules become plan-visible, approval-bound external actions; their target, platform, and risk come from the verified manifest rather than renderer input. All-platform command declarations are bound to the current supported platform so they are not silently dropped.
- The closed validation-parameter schema admits paired executable,
  interpreter, and runtime SHA-256/size identity fields plus bounded arguments,
  network access, expected writes, privilege, and rollback-boundary evidence.
  Schema validation rejects unknown keys; deterministic Rust rejects an
  incomplete identity pair or an invalid lowercase SHA-256/size before any
  action can be planned.
- Selected canonical `.claude/agents/*.md` bytes are verified before
  deterministic projection. Generated Codex `.codex/agents/*.toml` bytes are
  adapted at staging to add the project-required `fork_context=false` spawn
  rule when absent and reject an explicit true declaration; the operation keeps
  canonical source SHA-256 evidence distinct from the adapted projection hash.
- The verified `core.agents` template is project-adapted only after download.
  Remove its complete template-only `## Placeholder Guide` section before the
  adapted bytes enter staging, conflict review, or the flattened Chat export.
  Preserve the following real H2 instruction section and continue to reject
  unresolved project placeholders.
- Dependency resolution exposes a deterministic reverse-dependency map for update/removal impact review; provider constraints are applied after forward dependency expansion.
- The published core profile includes `docs.mcp_integration`, a managed copy of
  the repository's HOI4 Agent Tools capability, evidence, recovery, and
  troubleshooting guide. It depends on `mcp.hoi4_agent_tools`; documentation
  never substitutes for immutable executable or platform evidence.
- The optional Windows 3D component includes the repository-owned
  `blender_hoi4` production adapter, worker, asset profiles, Meshy tool
  contract, and wrapper alongside the bootstrap evidence. Keep unrestricted
  Blender Lab development-only, derive every installed path from the verified
  manifest, require declared `uv`, and do not infer a macOS route. The
  manifest command must match the app-owned allowlisted action ID and fixed
  arguments and must declare network access, expected external writes,
  current-user privilege, and the rollback boundary. The app may run only that
  recognized reviewed action after verifying the installed script bytes; a
  changed command contract requires an app release.
- The published component graph includes the optional complete portrait
  contract, `workflow.portraits.router`, and one provider-specific skill
  component for each supported route. Each enabled provider depends on the
  router; select exactly one provider skill and let dependency expansion add
  the router at the resolved revision. Disabled adaptation removes the entire
  portrait component closure and marked ComfyUI guidance. Never synthesize
  components from the retired `workflow.lora_comfyui_interest` value; legacy
  lock values are ignored and removed by the next verified transaction.
- The optional Super Events parent installs one additional `hoi4-super-events` skill, three narrow research subagents, and six hidden runtime components.
  The verified ordinary planning, events, assets, text/audio, and subagent
  skills contain bounded marked sections that deterministic adaptation retains
  only when the workflow is selected; unselected installs strip those sections.
  Verify source bytes first, then deterministically replace `[MOD_PREFIX]` and
  `[MOD_NAME]` only in its interface, common-script, event, and localisation
  text components, and rename those runtime text/interface destinations from
  `hoi4ms_*` to the confirmed project prefix. Reject invalid namespaces,
  unresolved placeholders, and adapted destination collisions.
  Never text-adapt its binary GFX/PSD component.
- External-action dry runs carry reviewed arguments, cwd, environment names, manifest-declared expected writes, network/privilege evidence, and rollback boundary. Missing manifest declarations remain `not_declared`; they are not inferred from a repository script.

## Required tests

- the one canonical `hoi4-mod-setup.manifest.json` discovery path is used and
  no parallel versioned manifest filename is bundled or requested
- branch to commit resolution and publication-commit manifest binding
- commit immutability
- manifest major rejection, including a newer major reported as needing an app
  update before any schema is applied
- format 6 MCP declarations: a project route without a registration, a manual
  route with one, a registration owned by another client or not installing its
  path, an unknown format, a non-HTTPS documentation link, and invalid shared
  clients; format 5 rejects every format 6 field
- availability gating: unknown or mismatched project routes, platform gaps, and
  format 5 manifests never offer an unshipped client
- authoritative schema rejection for unknown top-level and nested fields before
  typed deserialization
- schema acceptance for all six executable/interpreter/runtime identity fields
  and the bounded action-evidence fields, paired-field runtime enforcement, and
  rejection of unknown validation keys
- dependency cycle
- selected file set only
- rate-limit-only 403/429 fallback to the fixed repository's read-only ref
  advertisement, including default-branch and peeled-release binding; reject
  unrelated 403s, other repositories, other Git services, redirects, object
  downloads, and checkout behavior
- resolution-cache expiry at 15 minutes, lower manifest TTL, request-key
  isolation, exact revision/branch/manifest evidence, and no cross-revision
  cache hit
- `expected_files` inventory selection without a recursive tree prerequisite,
  including missing, duplicate, outside-component, size, SHA-256, and
  undeclared-tree rejection
- selected-component isolation, including the complete hidden
  `workflow.super_events` dependency closure when selected and none of its
  skill, runtime, binary, or guide files when unselected
- checksum mismatch
- Git-blob generation under a worktree with different line endings
- source-audit digests match the exact bundled manifest bytes
- cache path replacement after a verified handle is opened
- partial download and resume
- redirect rejection and API/raw host-owner-repository policy
- archive traversal and extraction limits
- cache corruption
- wiki page coverage
- unsupported platform state
- source-backed fixtures preserve Windows/Linux MCP declarations and Windows-only 3D declarations
- external-action declaration and approval binding
- manifest provenance/update/signing policy rejection
- manifest-owned image provider/model catalog expansion without adding app
  schema enums or per-model persisted state; selected route component closure
  remains dependency-validated

## Update this skill when

Update this skill when the manifest schema, source API, cache layout, checksum policy, wiki distribution, component graph, MCP declaration, or latest and pinned resolution changes.

## Completion standard

Source work is complete only when one exact revision is bound to the manifest,
selected files, sizes, hashes, platform evidence, required wiki-page list, and
the manifest's wiki provenance/media metadata;
unsupported routes remain explicit; and the relevant hostile-source and
external-action tests pass.

The live-testing catalog binds the reviewed source revision, exact native Linux
game version, platforms and route. Register only selected coding clients;
Clausewitz stays a to-do until Linux readiness is explicitly confirmed. Keep
LiveTools bootstrap registration disabled because the app owns project
registrations.

New-project UI follows the resolved manifest's default profile. An explicit
Latest choice still resolves the default branch to one exact commit, while
existing projects retain their reviewed source choice.
