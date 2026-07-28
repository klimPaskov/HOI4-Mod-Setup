# Source, provider provenance, and flattened Chat audit

## Verdict

**Fail for the bounded source/provider/flatten gate.** Fresh source resolution is substantially fail-closed: latest resolves the default branch to a full commit before manifest access, pinned commits and releases resolve to commit objects, manifest and selected blobs use that one revision, selected files require manifest SHA-256 and size evidence, and the plan/lock retain source evidence. The current bundled wiki declaration is also internally complete and honestly records unknown provenance/license state.

The gate still fails because final conflict outcomes do not drive the flattened view, non-Codex dependency expansion can restore Codex-only components, provider endpoint identity is not bound to the confirmed analysis, and the MCP health route can execute an unpinned PATH command without package/hash identity. Additional manifest-example and validation gaps are listed below.

## Scope and source revision

- Workspace: `HOI4 Mod Setup`, branch `codex/bootstrap-hoi4-mod-setup`
- Workspace HEAD: `ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70`
- Final audit snapshot: `2026-07-28T11:50:26+03:00`
- Working tree: already modified by concurrent work; this audit did not revert or edit product files.
- Bundled workflow-source revision: `27128a7b311d728a959afff7238a9aeeb9987f2b`
- Bundled manifest SHA-256: `cddb7ece7235d033888d85508455c255ffe320f0f28bc924999e8f4ddd1c19b5`
- Inspection was limited to the current workspace. No network source, full repository clone, or local workflow checkout discovery was used.

Key final-snapshot hashes:

| File | SHA-256 |
| --- | --- |
| `src-tauri/src/source.rs` | `9030a1424b921df36648f172daf7ee97e132f041798fc24650f63376ff7f99cd` |
| `src-tauri/src/ai.rs` | `b01027dd092bda01f112c64fadbfd5f5413ce34e66342356f2beade30ecf3429` |
| `src-tauri/src/flatten.rs` | `cabba2f1cc9cb59983ea177b5881864bc4464bf5b59f0aa1f18c2490f72f4217` |
| `src-tauri/src/commands.rs` | `d804cd1710e89e27f05cec4354b682d7c816608ec4ab4b52a1fc82790f9e2748` |

## Audited files and evidence

Primary implementation:

- `src-tauri/src/source.rs`: `HttpSourceClient`, `resolve_source`, `validate_manifest`, dependency expansion, platform support, file selection, download verification, immutable cache
- `src-tauri/src/ai.rs`: provider registry, endpoint validation, provider request, analysis record construction
- `src-tauri/src/flatten.rs`: flat input/output mapping, containment, limits, collision handling
- `src-tauri/src/commands.rs`: selected components, plan construction, maintenance, conflicts, plan/lock-facing provenance, external actions
- Supporting bounded evidence in `src-tauri/src/models.rs`, `src-tauri/src/mcp.rs`, and lock construction/readiness identifiers in `src-tauri/src/transaction.rs` and `src-tauri/src/readiness.rs`

Contracts and examples:

- `schemas/remote-manifest.schema.json`
- `schemas/installation-plan.schema.json`
- `schemas/installation-lock.schema.json`
- `schemas/project-state.schema.json`
- `schemas/codex-analysis.schema.json`
- Corresponding examples, especially `repository-manifest.example.json`, `installation-plan.example.json`, `installation-lock.example.json`, `project-state.example.json`, and `codex-analysis.example.json`
- `source-manifest/hoi4-mod-setup.manifest.json`
- `scripts/generate_manifest_evidence.py`
- `scripts/validate_repository_templates.py`

Documentation and mirrors:

- `docs/04_remote_repository_manifest.md`
- Directly relevant `docs/05_wiki_installation.md`, `docs/09_component_dependency_model.md`, `docs/11_mcp_setup.md`, and `docs/31_ai_provider_profiles_and_chat_sources.md`
- `README.md`
- Relevant `repository-template/` mirrors

`README.md`, `AGENTS.md`, `GOAL_PROMPT.md`, `docs/31_ai_provider_profiles_and_chat_sources.md`, the repository validator, and the Codex integration skill were byte-identical to their checked-in repository-template mirrors at the final snapshot.

## Findings by severity

No critical finding was identified in this bounded audit.

### High — Flattened outputs are derived before final conflict outcomes

`commands.rs::build_plan` retains every verified incoming file in `prepared`, including modified files whose operation is `Skip` or unresolved (`src-tauri/src/commands.rs:2332-2399`). It then calls `flatten::build_artifacts` before the user finishes conflict review (`src-tauri/src/commands.rs:2523-2530`). `flatten.rs::build_artifacts` treats every prepared byte sequence as the source of truth (`src-tauri/src/flatten.rs:25-47`).

`commands.rs::resolve_installation_conflict` later removes or changes the original prepared operation for keep, merge, or rename, but does not regenerate its already-created `codex.chat_flatten` artifacts (`src-tauri/src/commands.rs:3384-3505`). The maintenance path has the same ordering: it prepares incoming bytes even for skip operations and builds the flat set from them (`src-tauri/src/commands.rs:3123-3198`).

Result: preserving a locally modified `AGENTS.md`, skill, subagent, or README can still produce a new flat copy containing incoming repository bytes instead of the accepted project bytes. This contradicts the mapping and transaction claims in `docs/31_ai_provider_profiles_and_chat_sources.md:55-70`.

### High — Non-Codex selection can regain Codex-only dependencies

`commands.rs::selected_ids` removes only a directly selected `codex.config` when the provider is not Codex (`src-tauri/src/commands.rs:1587-1610`). `build_plan` subsequently calls the provider-neutral `source.rs::expand_components`, which recursively restores dependencies (`src-tauri/src/commands.rs:2240-2247`; `src-tauri/src/source.rs:841-889`).

The bundled manifest declares:

- `mcp.hoi4_agent_tools -> codex.config` (`source-manifest/hoi4-mod-setup.manifest.json`, component `mcp.hoi4_agent_tools`)
- `workflow.3d -> codex.config` (`source-manifest/hoi4-mod-setup.manifest.json`, component `workflow.3d`)

Therefore a non-Codex state selecting MCP or 3D can reinstall `.codex/config.toml`. On macOS, the optional Windows MCP is correctly marked unsupported, but its platform-neutral `codex.config` dependency remains selected. Provider-specific closure is not fail-closed after dependency expansion.

### High — The provider endpoint used for analysis is not bound to analysis provenance

`ai.rs::analyze` sends the request to `AiProviderConfig.endpoint` but records only provider, model, and optimization profile in `CodexAnalysisRecord` (`src-tauri/src/ai.rs:219-286`; `src-tauri/src/models.rs:1047-1075`). `commands.rs::codex_analysis_from_state` checks provider/model/profile but has no endpoint identity to compare (`src-tauri/src/commands.rs:2100-2152`). `build_plan` independently copies the current renderer state endpoint into the plan.

A non-Codex analysis may consequently be created and confirmed against endpoint A, followed by a state change to valid endpoint B; the plan and lock then attribute the confirmed proposals to B without reanalysis. Plan and lock schemas expose nullable `ai_endpoint` but do not require it even when `ai_provider` is hosted or local (`schemas/installation-plan.schema.json:7-54`; `schemas/installation-lock.schema.json:7-49`).

This breaks exact provider-profile provenance. Codex correctly records no model identity from App Server; the separate plan `ai_model` value should therefore also be described as selected configuration, not observed App Server evidence.

### High — MCP command identity is not pinned to manifest file/package evidence

The bundled `mcp.hoi4_agent_tools` component is `generated`, has no `expected_files`, names the bare command `hoi4-agent-tools.cmd`, and uses `version_policy: manifest` without a pinned package/version/hash. `commands.rs::manifest_external_actions` labels the target `repository_script` even though it is not a selected repository blob (`src-tauri/src/commands.rs:1613-1670`).

`mcp.rs::manifest_target` validates only the bare command name, while `mcp.rs::initialize_health` resolves it from PATH and starts it (`src-tauri/src/mcp.rs:34-65`, `src-tauri/src/mcp.rs:97-137`). Initialize and `tools/list` validation require bounded, structurally valid fields but do not compare server name, version, tool identity, or executable SHA-256 to source evidence (`src-tauri/src/mcp.rs:146-203`).

The current evidence does support these declarations: Windows only; tools `node` and `hoi4-agent-tools.cmd`; no environment variables; command health target `hoi4-agent-tools.cmd`; MCP initialize plus optional `tools/list`; and the listed focus/event/GUI/map/technology/probability capabilities. It does **not** support an installation package, immutable executable identity, expected server version, macOS command, or source-file hash. The current executable route should remain unsupported rather than treating any same-named PATH server as the source-declared server. This also disagrees with `docs/04_remote_repository_manifest.md:96-101`, which says the installed target is hash/size verified before start.

### Medium — `validate_manifest` does not enforce its supplied revision

`source.rs::validate_manifest` validates both the supplied revision and `generated_for_revision`, but never compares them (`src-tauri/src/source.rs:513-566`). Fresh `resolve_source` performs a separate exact equality check and is safe (`src-tauri/src/source.rs:461-486`).

The gap remains reachable through `commands.rs::resolve_installed_manifest`: the bundled-manifest branch verifies the manifest hash and then calls `parse_manifest(..., Some(lock.source.revision))`, relying on the missing equality check (`src-tauri/src/commands.rs:1062-1070`). A mismatched or altered lock can therefore bind the bundled manifest to a different revision until a later file hash happens to fail.

### Medium — The repository manifest example is schema-valid but runtime-invalid

`examples/repository-manifest.example.json` has empty `expected_files` for non-generated `core.skills`, `core.subagents`, `codex.config`, `wiki.snapshot`, and `workflow.3d`. Draft 2020-12 validation reports zero errors because the schema does not conditionally require evidence for non-generated sources. Runtime `source.rs::validate_manifest` rejects those components (`src-tauri/src/source.rs:600-606`).

The example also differs materially from the bundled contract, including an older generator revision, an asserted wiki snapshot marker, only eight components, and required rather than optional MCP. It cannot serve as a runnable source-contract example.

### Medium — Reverse dependencies are not modeled

Cycle validation and forward topological dependency expansion are present (`src-tauri/src/source.rs:807-889`), but no reverse-dependency graph or dependent-impact evidence is returned in source models, plan schemas, examples, or commands. This omits the reverse-dependency step required by `docs/09_component_dependency_model.md` and leaves deselection/removal impact implicit.

### Medium — `all` command platform declarations can be silently dropped

`validate_manifest` rejects `platforms: ["all"]` only when a required tool has a non-empty `commands` list; it does not apply the same rule to `validation.kind == "command"` (`src-tauri/src/source.rs:668-700`). `commands.rs::manifest_external_actions` maps `ManifestPlatform::All` to `None` and `filter_map` silently removes the action (`src-tauri/src/commands.rs:1613-1635`).

Current bundled command components use explicit Windows declarations, so the current route is not affected. A future manifest command on `all` would pass runtime validation but disappear from dry-run evidence rather than fail closed.

### Low — Flatten skill matching accepts broader paths than the documented mapping

The documented input is exactly `.agents/skills/<skill>/SKILL.md`. `flatten.rs` accepts any path with at least four segments and uses the directory immediately before `SKILL.md` as the flat name (`src-tauri/src/flatten.rs:73-85`). A nested `references/SKILL.md` could be exported as `references.md`. The current bundled manifest has 11 `SKILL.md` entries and all use the expected four-segment shape.

## Confirmed fail-closed behavior

- Latest mode reads GitHub `default_branch`, resolves its branch object to a full commit, and verifies the commit object before manifest access (`HttpSourceClient::resolve_commit`).
- Pinned commit accepts only a 40-hex commit. Pinned release resolves a release tag, dereferences bounded annotated tags, verifies the commit, and records canonical release identity.
- `resolve_source` fetches the manifest at the resolved commit and accepts remote or bundled evidence only for that exact commit.
- Files are fetched from raw GitHub URLs containing the same commit. Only selected component blobs are fetched; the recursive tree request is metadata and rejects truncation or more than 100,000 entries. No application `git clone` or local-checkout search was found.
- Source URLs require HTTPS, no userinfo or port, and hosts limited to `api.github.com` and `raw.githubusercontent.com`; redirects are bounded to three approved hops.
- Selected inputs are limited to 20,000 files/1 GiB and 512 MiB per file. Cache keys include revision and SHA-256; cache entries are rechecked by size/hash and link components are rejected.
- Manifest paths and selected destination paths are normalized; case/Unicode canonical duplicate destinations are rejected.
- Every selected non-generated file requires manifest SHA-256 and size and is reverified before planning. Plan operations retain source path/hash/size, and lock files retain revision, source hash/size, installed result hash/size, component, ownership, platform, and destination.
- Dependency cycles and unknown dependencies block. Optional unsupported components become `unsupported_platform`; unsupported required components block.
- The bundled manifest has 917 expected files, all with SHA-256 and size, and no duplicate evidence paths.
- Wiki evidence contains 586 files: 50 Markdown, 2 JPG, 411 PNG, and 123 SVG. All 11 required pages have file evidence. Media policy is `all_declared`; provenance is `repository_only`; license is `not_found`; snapshot marker is null. These uncertain states are honest.
- Flat output rejects missing required inputs, case-insensitive output collisions, traversal, output-folder recursion, secret-shaped paths/content, links, invalid required text, and per-file/aggregate bounds.

## Reproducibility and integrity gaps

- Flat lock rows identify generated destinations and output hashes but do not retain a source-to-flat origin map. Independent reconstruction cannot prove which accepted skill/subagent/local file produced each flat row.
- `scripts/generate_manifest_evidence.py` uses an explicit source root and `git archive` for an exact revision, which is good, but it only writes; it has no read-only/check mode for CI reproduction.
- Latest branch metadata has no implemented ETag/short-TTL cache despite `docs/04_remote_repository_manifest.md:128`; exact commit resolution is still performed on every request.
- Wiki media policy is enum-checked, and current `all_declared` coverage follows from strict tree/evidence equality, but no test exercises policy-specific media omissions or unknown provenance/license values.
- The current workspace audit did not independently fetch the external workflow revision, so bundled file hashes were checked for internal completeness, not re-derived from external bytes.

## Unsupported or uncertain routes

- Archive/bundle sources are not represented by the current source-kind schema or Rust model. No archive extraction route, ratio/depth/count policy, or archive tests exist; archive installs must remain unsupported.
- `signing.required` is parsed but no signature verification route was found. The current bundled manifest sets `required: false`; signed assets must not be claimed.
- Pinned release currently means release/tag identity resolved to a commit, not immutable release-asset/archive installation.
- MCP and 3D are Windows-only in current evidence. Their optional macOS state is correctly non-blocking and unsupported.
- MCP package name, install command, immutable version, executable hash, and macOS route are missing.
- Wiki source provenance and license remain unresolved and must continue to display `repository_only` / `not_found`; no snapshot marker should be invented.

## Tests run and meaningful gaps

Passed on the final applicable file hashes:

- `cargo test --manifest-path src-tauri/Cargo.toml source::tests -- --test-threads=1`: 9 passed
- `cargo test --manifest-path src-tauri/Cargo.toml ai::tests -- --test-threads=1`: 6 passed
- `cargo test --manifest-path src-tauri/Cargo.toml --all-features flatten::tests -- --test-threads=1`: 3 passed
- `cargo test --manifest-path src-tauri/Cargo.toml --all-features commands::tests -- --test-threads=1`: 16 passed
- `python scripts/validate_repository_templates.py`: 12 integrity groups passed
- Manual Draft 2020-12 validation: repository-manifest, installation-plan, and installation-lock examples each had zero schema errors.

Meaningful missing tests:

- fake HTTP/GitHub tests for default-branch-to-commit resolution, commit-object mismatch, annotated release identity, redirect host policy, body limits, truncated trees, cache corruption, and checksum/size mismatch;
- supplied-revision versus `generated_for_revision` mismatch, including bundled-lock maintenance;
- provider-conditioned dependency closure and reverse-dependent impact;
- keep/merge/rename source conflicts followed by exact flat regeneration in create and maintenance plans;
- endpoint switch after provider analysis confirmation;
- MCP command/package/version/hash identity and unsupported-state behavior when identity metadata is absent;
- runtime parsing of the checked-in repository manifest example;
- wiki required-media policy variants and provenance/license validation;
- archive and signature verification tests, once those routes are explicitly designed.

## Recommended parent actions

1. Build flat artifacts from the effective, post-conflict project result. Regenerate dependent flat operations whenever a source conflict changes, retain source-to-flat origin hashes, and verify mapping freshness in readiness.
2. Apply provider constraints after dependency closure. Block or mark unsupported any non-Codex selection whose closure contains `codex.config`, Codex MCP, or another Codex-only component.
3. Bind a canonical endpoint identity (or non-secret endpoint digest/session configuration ID) into non-Codex analysis records and compare it through confirmation, plan, and lock. Require endpoint evidence conditionally in plan/lock schemas.
4. Keep MCP installation/health unsupported until the manifest provides exact command arguments, package/install source, immutable version or executable hash, expected server identity, environment names, capabilities, health checks, and platform route. Do not label a PATH command as a repository script.
5. Compare `generated_for_revision` with the supplied revision inside `validate_manifest` itself and add fresh and bundled-maintenance mismatch tests.
6. Keep schema/example validation green, make the repository manifest example runtime-valid, and add a test that runs the Rust manifest validator against that example.
7. Add reverse-dependency evidence and reject every command-bearing `all` route unless explicit platform variants exist; never silently drop an external action.
8. Keep archive, signing, macOS MCP/3D, and wiki provenance/license states explicitly unsupported or uncertain until repository evidence exists.

Only this audit handoff was added. No source, schema, example, workflow, documentation, or template file was modified by this auditor.
