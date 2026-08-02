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

Pinned commit uses the supplied commit for every step. Pinned release resolves and records both release identity and exact commit when available.

## Hard rules

- Never clone the complete source repository through the application.
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

## Manifest change workflow

1. Update schema and examples.
2. Add a migration or compatibility rule if the manifest model changes.
3. Validate component dependency cycles and reverse dependencies.
4. Test platform resolution separately from component selection.
5. Test hostile paths, redirects, truncation, cache corruption, and hash mismatch.
6. Test default branch change between requests.
7. Test latest and pinned reproducibility.
8. Update source audit and docs.

## Wiki rules

Install the manifest-declared wiki tree at `<mod_project>/paradox_wiki/`. Validate required page coverage, hashes, path containment, case collisions, media policy, and update behavior. Report missing formal provenance or license data exactly as missing.

## MCP and external tools

MCP servers and external dependencies are components. Their command, arguments, tools, environment variable names, health checks, supported platforms, and update behavior come from verified repository evidence. A similar command on another platform is not support evidence.

## Current implementation boundaries

- The application consumes `hoi4-mod-setup.manifest.json` from the approved GitHub source at one verified commit. The local evidence generator is `scripts/generate_manifest_evidence.py`; it accepts only an explicitly supplied source root and must not discover checkouts.
- Manifest generation requires an exact lowercase 40-character revision and
  reads bytes from that revision with `git ls-tree` plus `git cat-file
  --batch`. Never hash worktree or `git archive` output because checkout
  attributes and line-ending conversion can change bytes. The generator fails
  when a declared path is absent from the selected commit.
- Runtime parsing compiles and applies
  `docs/schemas/remote-manifest.schema.json` as Draft 2020-12 before
  deserializing `RemoteManifest`. Unknown fields, including nested policy
  fields, fail closed; handwritten validation remains the second layer for
  cross-field and trust-policy invariants.
- A non-generated component is not downloadable unless every selected file has lowercase SHA-256 and size evidence. The published live-source manifest records the tracked snapshot used to generate its evidence; the runtime binds the manifest and all selected blobs to the one resolved commit and rechecks those hashes.
- `select_component_files` walks only the supported selected-component closure,
  applies each component's include/exclude rules, and rejects missing evidence
  or destination collisions before any selected blob is fetched. This is
  selective manifest isolation, not a filtered full-repository clone.
- Multiple tree components may target the same managed directory only so disjoint selected-only packages can share standard `.agents/skills/` or `.codex/agents/` roots. The selected-file pass still canonicalizes every concrete destination and rejects any overlap before download or staging.
- An immutable install requires `generated_for_revision` in the manifest; the runtime schema and validator reject a manifest that cannot prove which commit produced its evidence.
- Verified blobs are cached under the application data cache by `<revision>/<sha256>` and are accepted only after size (when declared) and SHA-256 revalidation. A corrupt cache entry is discarded and fetched again.
- Cache reads bind the path to one no-follow file handle, read a bounded byte
  buffer once, and verify that exact buffer before returning it. Never hash one
  path lookup and then reopen the path to return different bytes.
- Verified blob downloads retain only a bounded `.part` cache entry after an interrupted read; a retry may use a validated HTTP range, and the hash-addressed cache is promoted only after the complete size and SHA-256 checks pass. Partial bytes are never staged.
- Manifest paths must use canonical slash-separated spelling (with an optional directory trailing slash); collision keys use Unicode NFC plus case folding so composed and decomposed macOS names cannot map to two managed destinations.
- The runtime rejects manifests whose wiki provenance/license status is outside the schema enums, whose latest policy does not require default-branch resolution plus recorded commit evidence, or whose signing policy requires verification that the build does not implement. Pinned commit/release modes are checked against the manifest's explicit allow flags.
- Release tags are resolved through typed GitHub objects, including annotated-tag dereferencing, and pinned revisions are verified as commit objects before manifest or file access.
- The MCP component is optional and Windows-only; selecting it changes the structurally generated Codex TOML, while macOS retains an explicit unsupported state and never receives a substitute command. A wrapper action requires immutable executable, command-interpreter, and runtime hash/size evidence; if any is absent, no same-named `PATH` command is executed. Installed readiness may recognize that the exact declared command already exists on the reviewed PATH and report it as configured, but this discovery does not authorize or run it. The offline wiki is always rooted at `paradox_wiki/`; the plan and lock copy the exact manifest `wiki.required_pages` list plus snapshot/media/provenance/license metadata for the resolved revision, and readiness blocks legacy locks that lack that evidence instead of using a newer bundle.
- `generated_for_revision` is required immutable provenance but may precede the publication commit because a manifest cannot contain the final hash of the commit that contains it. The resolver consumes the remote manifest at the resolved commit, and selective download fails closed when its per-file size or SHA-256 evidence does not match that same commit. A bundled bootstrap is never substituted for a newly resolved remote commit.
- Expected file evidence carries both SHA-256 and byte size into selected-file records, operations, locks, and readiness. Command-bearing validation rules become plan-visible, approval-bound external actions; their target, platform, and risk come from the verified manifest rather than renderer input. All-platform command declarations are bound to the current supported platform so they are not silently dropped.
- Selected `.codex/agents/*.toml` bytes are verified before deterministic
  adaptation. Staging adds the project-required `fork_context=false` spawn
  rule to developer instructions when absent and rejects an explicit true
  declaration; the operation keeps source SHA-256 evidence distinct from the
  adapted result SHA-256.
- The verified `core.agents` template is project-adapted only after download.
  Remove its complete template-only `## Placeholder Guide` section before the
  adapted bytes enter staging, conflict review, or the flattened Chat export.
  Preserve the following real H2 instruction section and continue to reject
  unresolved project placeholders.
- Dependency resolution exposes a deterministic reverse-dependency map for update/removal impact review; provider constraints are applied after forward dependency expansion.
- The published component graph has no LoRA/ComfyUI interest component. Do not
  synthesize one from older manifests or project state; legacy lock values are
  ignored and removed by the next verified transaction.
- The optional Super Events parent expands to six hidden runtime components.
  Verify source bytes first, then deterministically replace `[MOD_PREFIX]` and
  `[MOD_NAME]` only in its interface, common-script, event, and localisation
  text components. Reject invalid namespaces and unresolved placeholders.
  Never text-adapt its binary GFX/PSD component.
- External-action dry runs carry reviewed arguments, cwd, environment names, manifest-declared expected writes, network/privilege evidence, and rollback boundary. Missing manifest declarations remain `not_declared`; they are not inferred from a repository script.

## Required tests

- branch to commit resolution and publication-commit manifest binding
- commit immutability
- manifest major rejection
- authoritative schema rejection for unknown top-level and nested fields before
  typed deserialization
- dependency cycle
- selected file set only
- selected-component isolation, including the complete hidden
  `workflow.super_events` dependency closure when selected and none of its
  skill, runtime, binary, or guide files when unselected
- checksum mismatch
- Git-blob generation under a worktree with different line endings
- cache path replacement after a verified handle is opened
- partial download and resume
- redirect and host policy
- archive traversal and extraction limits
- cache corruption
- wiki page coverage
- unsupported platform state
- external-action declaration and approval binding
- manifest provenance/update/signing policy rejection

## Update this skill when

Update this skill when the manifest schema, source API, cache layout, checksum policy, wiki distribution, component graph, MCP declaration, or latest and pinned resolution changes.

## Completion standard

Source work is complete only when one exact revision is bound to the manifest,
selected files, sizes, hashes, platform evidence, required wiki-page list, and
the manifest's wiki provenance/media metadata;
unsupported routes remain explicit; and the relevant hostile-source and
external-action tests pass.
