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
- `schemas/remote-manifest.schema.json`
- `examples/repository-manifest.example.json`

Inspect the live Agentic HOI4 Modding repository when a path, package, command, platform declaration, or dependency may have changed. Do not rely on memory.

## Trust sequence

Latest mode:

1. Resolve the repository default branch.
2. Resolve that branch to one exact commit.
3. Fetch the manifest at that commit.
4. Validate manifest schema and supported major version.
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
- A non-generated component is not downloadable unless every selected file has lowercase SHA-256 and size evidence. The local live source manifest is a publication staging artifact until it is committed at a self-consistent upstream revision.
- An immutable install requires `generated_for_revision` in the manifest; the runtime schema and validator reject a manifest that cannot prove which commit produced its evidence.
- Verified blobs are cached under the application data cache by `<revision>/<sha256>` and are accepted only after size (when declared) and SHA-256 revalidation. A corrupt cache entry is discarded and fetched again.
- Release tags are resolved through typed GitHub objects, including annotated-tag dereferencing, and pinned revisions are verified as commit objects before manifest or file access.
- The MCP component is optional and Windows-only; selecting it changes the structurally generated Codex TOML, while macOS retains an explicit unsupported state and never receives a substitute command. The offline wiki is always rooted at `paradox_wiki/`; the plan and lock copy the exact manifest `wiki.required_pages` list for the resolved revision, and readiness blocks legacy locks that lack that evidence instead of using a newer bundle.
- If the remote manifest's `generated_for_revision` is stale, the resolver may use the bundled manifest only when its evidence is generated for the same resolved commit; the plan and lock record `manifest_origin: bundled_revision_bootstrap`, and a mixed-revision install is rejected.
- Expected file evidence carries both SHA-256 and byte size into selected-file records, operations, locks, and readiness. Command-bearing validation rules become plan-visible, approval-bound external actions; their target, platform, and risk come from the verified manifest rather than renderer input.
- External-action dry runs carry reviewed arguments, cwd, environment names, manifest-declared expected writes, network/privilege evidence, and rollback boundary. Missing manifest declarations remain `not_declared`; they are not inferred from a repository script.

## Required tests

- branch to commit resolution
- commit immutability
- manifest major rejection
- dependency cycle
- selected file set only
- checksum mismatch
- partial download and resume
- redirect and host policy
- archive traversal and extraction limits
- cache corruption
- wiki page coverage
- unsupported platform state
- external-action declaration and approval binding

## Update this skill when

Update this skill when the manifest schema, source API, cache layout, checksum policy, wiki distribution, component graph, MCP declaration, or latest and pinned resolution changes.

## Completion standard

Source work is complete only when one exact revision is bound to the manifest,
selected files, sizes, hashes, platform evidence, and required wiki-page list;
unsupported routes remain explicit; and the relevant hostile-source and
external-action tests pass.
