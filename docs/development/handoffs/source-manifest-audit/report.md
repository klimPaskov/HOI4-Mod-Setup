# Source manifest audit handoff

## Scope and source revision

Read-only audit of revision modes `latest`, `pinned_commit`, and `pinned_release`; profiles `core` and `core_with_3d`; all 10 declared components; Windows and macOS behavior; and the accepted rules of no full clone, exact-commit resolution, one revision, selective downloads, SHA-256 verification, honest unsupported states, and non-blocking optional workflows.

Audited source manifests:

- `source-manifest/hoi4-mod-setup.manifest.json`, SHA-256 `33d9fcaed8b1c11820dd2ac89ca41ef96aed2aa55e57582646aea0a14e25363a`
- `C:\Users\klimp\OneDrive\Documents\Paradox Interactive\Hearts of Iron IV\mod\agentic_hoi4_modding\hoi4-mod-setup.manifest.json`, SHA-256 `6fe115705e21283714c07d7f6d8a073590b44bea09824d900bf25a92e442d417`

Both declare source revision `599497ea2f93612d9094461c6fde114fc87a5c0f` (`source-manifest/hoi4-mod-setup.manifest.json:4`; live manifest:4). Their only semantic differences are four `expected_files` arrays present in the local copy and absent from the live copy. The HOI4 Mod Setup workspace has no Git metadata, so its own commit could not be reported. No other source checkout was searched and no network revision was inferred.

## Audited files and tests

- `.codex/agents/hoi4setup_source_manifest_auditor.toml`
- `.agents/skills/hoi4-mod-setup-source-manifest/SKILL.md`
- `src-tauri/src/source.rs`
- `src-tauri/src/security.rs`
- `schemas/remote-manifest.schema.json`
- `schemas/installation-plan.schema.json`
- `schemas/installation-lock.schema.json`
- `examples/repository-manifest.example.json`
- `examples/installation-plan.example.json`
- `examples/installation-lock.example.json`
- Directly relevant docs: `docs/00`, `04`, `05`, `06`, `07`, `09`, `11`, `13`, `16`, `18`, `20`, `22`, and `23`
- Directly relevant inline Rust tests and `scripts/validate_repository_templates.py`

Read-only Draft 2020-12 validation passed for both manifests and all three audited examples. This is not sufficient integrity evidence, as finding SM-001 explains. Rust tests were inspected but not executed because Cargo can write build artifacts and this role permits only this report write.

## Findings by severity

### Critical — SM-001: downloaded content is accepted without trusted SHA-256 evidence

The live manifest has no `expected_files` on any non-generated component (live manifest:7-16). The local manifest has hashes only for `core.agents`, `codex.config`, `docs.source`, and `template.chaos_redux_agents` (`source-manifest/hoi4-mod-setup.manifest.json:25,74,110,127`); the skill, subagent, wiki, and 3D trees have no file index (`:33-63,135-168`).

The schema makes `expected_files` optional and each SHA nullable (`schemas/remote-manifest.schema.json:366-394`). Selection therefore records `None` when evidence is absent (`src-tauri/src/source.rs:655-665`), and verification compares only when a hash exists (`:671-694`). It then records a self-computed hash as if verified (`:695-705`). The live manifest consequently permits every downloaded file to pass without comparison to manifest-authenticated content. This violates the SHA-256-before-staging acceptance criterion (`docs/22_acceptance_criteria.md:3-11`).

### High — SM-002: the declared manifest revision is not deployable under the resolver contract

Repository evidence says no remote manifest existed at any proposed path at commit `599497…` (`docs/00_source_audit.md:31-43`), yet both current manifests declare exactly that revision. The resolver fetches the manifest from the resolved commit and requires a present declaration to equal that commit (`src-tauri/src/source.rs:246-275,301-309`).

Therefore, based on the audited evidence:

- pinning `599497…` cannot fetch the manifest that the repository audit says was absent there;
- fetching either current manifest from a newer commit fails the declaration equality check;
- the checked-in test only parses embedded fixture bytes and does not test remote availability (`src-tauri/src/source.rs:788-796`).

Current remote state was not queried under this bounded scope, so the exact failing HTTP route remains externally unverified; the internal evidence is nevertheless contradictory.

### High — SM-003: pinned-release resolution and release identity are not immutable for all Git tags

`PinnedRelease` reads `/git/ref/tags/{tag}` and treats `/object/sha` as a commit without checking `/object/type` or dereferencing annotated tag objects (`src-tauri/src/source.rs:150-175`). A 40-hex annotated-tag object SHA passes `validate_commit` but is not the release commit. Tag text is also interpolated into the API path without URL-component encoding.

The canonical release identity is not retained reliably: `resolve_source` stores only `request.release`, although the accepted input may instead be `requested_ref`, and it does not store the release API ID or canonical returned `tag_name` (`src-tauri/src/source.rs:151-167,255-262`). The installation plan has no dedicated release field (`schemas/installation-plan.schema.json:33-66`), and the lock's nullable `release` is not conditionally required for `pinned_release` (`schemas/installation-lock.schema.json:35-68`).

### High — SM-004: one-revision and plan/lock evidence are not enforced end to end

`fetch_file` does not call `validate_commit`, unlike manifest and tree fetches (`src-tauri/src/source.rs:180-190,232-243`). `verify_download` accepts an independent caller-supplied revision and labels the result with it (`:671-705`). With SM-001, mutable content fetched through a non-commit ref can be self-hashed and recorded under an unrelated commit unless every caller preserves the invariant; the API does not make that misuse impossible.

Schema evidence is also weak:

- plan `manifest_sha256` is not required (`schemas/installation-plan.schema.json:33-64`);
- plan operations require `source_sha256` but permit null and carry no source revision (`:156-203`);
- lock component `source_revision` is optional and unconstrained (`schemas/installation-lock.schema.json:70-110`);
- lock file `source_revision` is an unconstrained string and need not equal top-level `source.revision` (`:223-263`).

The examples use placeholder hashes and demonstrate no equality constraint (`docs/18_data_models_schemas.md:77-79`).

### High — SM-005: the default profile blocks macOS core readiness

`mcp.hoi4_agent_tools` is non-optional and Windows-only (`source-manifest/hoi4-mod-setup.manifest.json:83-99`; live manifest:11), but both `core` profiles select it (`source-manifest/hoi4-mod-setup.manifest.json:188-190`; live manifest:18). The resolver marks an unsupported non-optional component `blocked`, reserving `unsupported_platform` for optional components (`src-tauri/src/source.rs:515-567`).

Thus macOS `core` is blocked, contrary to the documented decision that macOS installs platform-neutral core while reporting the current MCP route unsupported (`docs/09_component_dependency_model.md:79-85`; `docs/20_testing_strategy.md:58-64`). `workflow.3d` is correctly optional and becomes `unsupported_platform` on macOS.

### High — SM-006: wiki integrity, coverage, and media distribution cannot be established

Both manifests honestly declare repository-only provenance, missing license evidence, 11 required pages, `all_declared` media, and destination `paradox_wiki/` (`source-manifest/hoi4-mod-setup.manifest.json:192-198`; live manifest:19). However:

- the wiki tree has no expected file/hash/size index;
- `snapshot_marker` is null even though repository evidence observed `_last_updated_on_27_Nov_2025.txt` (`docs/00_source_audit.md:114-116`);
- manifest validation checks only that the required-page list is non-empty, not that selected tree entries cover it (`src-tauri/src/source.rs:390-397`);
- no audited source test checks page coverage, case collisions, relative links, media policy, marker integrity, provenance, or license state.

Because wiki is selected by the core profile, this is a core readiness and reproducibility gap, not merely missing metadata.

### Medium — SM-007: redirect, host, size, archive, and cache policy is incomplete

- Redirects are limited to three but may cross to any host; there is no host allowlist (`src-tauri/src/source.rs:67-75`).
- Bodies are fully buffered before a 64 MiB limit is checked (`:84-103`); the later 512 MiB file limit is unreachable because all file fetches pass through the 64 MiB function (`:232-242`). No `Content-Length` preflight exists.
- Expected size is optional and checked only after download (`:687-694`).
- `cache_key` exists, but the audited source has no immutable-cache write/read validation, ETag, resume, TTL consumption, or corruption recovery (`:738-745`).
- Archive source kinds and extraction policy do not exist in the schema (`schemas/remote-manifest.schema.json:103-140`). Release assets/bundles are therefore unsupported, but this state is not represented explicitly.

The audited source uses fixed GitHub API/raw origins and contains no clone or local-checkout discovery path. Recursive tree metadata is fetched, but selected component files are the intended download set (`src-tauri/src/source.rs:188-230,569-627`).

### Medium — SM-008: expanded destination collision defenses are incomplete

Path traversal, absolute paths, ADS, and reserved Windows names are rejected (`src-tauri/src/security.rs:11-60`). Component-root destinations are compared with ASCII lowercase keys, and the current manifests have only the intended shared merged `.codex/config.toml` destination (`:95-97,161-181`).

Expanded file destinations, however, use a case-sensitive `HashSet` and do not call `validate_destination_set` (`src-tauri/src/source.rs:579-580,648-653`). Unicode normalization/case folding is absent. Case or Unicode-equivalent tree entries can therefore collide on Windows or case-insensitive macOS volumes. With no tree file indexes, these collisions cannot be rejected before remote tree expansion. Source URL path segments are normalized but not percent-encoded.

### Medium — SM-009: dependency graph basics pass, but reverse dependencies and machine-verifiable MCP/3D evidence are incomplete

The current graph is acyclic, unknown dependencies are rejected, and forward dependency expansion is topological (`src-tauri/src/source.rs:359-389,430-513`). Reverse dependencies are not computed by the audited implementation. Current material reverse edges are:

- `core.agents` ← `core.skills`, `codex.config`, `docs.source`, `template.chaos_redux_agents`
- `core.skills` ← `core.subagents`, `workflow.3d`
- `core.subagents` ← `workflow.3d`
- `codex.config` ← `mcp.hoi4_agent_tools`, `workflow.3d`

MCP declarations match the planning evidence in naming `hoi4-agent-tools.cmd`, Windows-only support, Node/npm checks, MCP initialize, and six broad capabilities. However, `npm install --global hoi4-agent-tools@latest` is mutable, and no pinned package resolution, server argument array, cwd, timeout, concrete MCP tool list, or capability provenance is represented (`source-manifest/hoi4-mod-setup.manifest.json:83-98`; `schemas/remote-manifest.schema.json:168-209`).

The 3D component correctly declares Windows-only support, tools `python`, `git`, `node-npx`, and `blender`, secret environment name `MESHY_API_KEY`, and non-blocking missing-key intent (`source-manifest/hoi4-mod-setup.manifest.json:152-168`). It lacks versions, package/source identities, concrete wrapper/MCP commands, and a file index. Its production `3d.bootstrap` validation has no `preflight_only` parameter, unlike the example (`examples/repository-manifest.example.json:555-561`). Download records also hard-code platform `all`, losing Windows-only file evidence (`src-tauri/src/source.rs:695-705`).

### Medium — SM-010: required failure-path tests are mostly missing, and the cycle test is non-probative

Present tests cover checked-in fixture parsing, a two-node topological expansion, one glob assertion, basic traversal, redaction, and a simple path property (`src-tauri/src/source.rs:788-815`; `src-tauri/src/security.rs:338-369`). Schema validation covers the local source manifest and examples but not the named live manifest, hash coverage, graph cycles, or destination collisions (`scripts/validate_repository_templates.py:51-99`).

`dependency_cycles_are_rejected` calls `validate_manifest`, but its fixture has no default profile (`src-tauri/src/source.rs:805-808,817-858`), so validation fails at the earlier default-profile requirement (`:383-386`) whether or not cycle detection works.

Missing meaningful tests include default-branch change, latest-to-exact-commit HTTP behavior, lightweight and annotated releases, canonical release identity, one-revision misuse, selected file set only, missing/nullable hash rejection, checksum mismatch, size mismatch, partial download/resume, redirect host policy, cache corruption, archive traversal/limits or explicit unsupported state, reverse dependencies/removal, case/Unicode destination collisions, wiki coverage/media/marker, macOS core readiness, and MCP/3D evidence derivation.

## Reproducibility, integrity, and unsupported routes

- Reproducible: current graph closure and manifest component metadata can be deterministically parsed; supplied hashes are verified when present.
- Not reproducible: live files, all manifest trees, mutable npm `@latest`, canonical pinned-release identity, per-file revision equality, wiki/media distribution, and external 3D dependency resolution.
- Explicitly unsupported by evidence: MCP and 3D routes on macOS; archive/release-bundle installation in the audited source.
- Uncertain: current remote default branch/commit, whether either manifest is committed remotely, current release tag forms, complete source/wiki trees, and actual MCP/3D commands beyond the manifest and planning audit. No metadata was invented for these routes.

## Recommended parent actions

1. Block production source installs until the remote manifest is committed at a self-consistent revision and every selected non-generated file/tree has a complete non-null SHA-256 and size index. Reject absent evidence rather than self-attesting after download.
2. Make exact revision a single typed capability shared by manifest, tree, file fetch, verification, plan, and lock; add schema equality/conditional validation and require manifest hash evidence.
3. Dereference annotated tags to commits, validate object types, encode tag paths, preserve canonical release identity, and test lightweight/annotated releases.
4. Resolve the macOS core contract: omit or make the Windows-only MCP component optional/non-blocking without inventing a macOS command.
5. Add a complete wiki index, observed marker, required-page/media coverage validation, path/case checks, provenance display, and explicit license state.
6. Add redirect-host allowlisting, streaming/body limits, immutable cache verification, resume/corruption behavior, and either safe archive policy or an explicit unsupported archive state.
7. Apply Unicode-aware, platform-aware collision keys to every expanded destination before download/apply.
8. Replace mutable/free-form MCP and 3D declarations with repository-evidenced executable/argument arrays, package/source identities, resolved versions, environment names, health operations, capabilities/tool lists, and platform routes; preserve runtime resolution in plan/lock.
9. Add the missing focused tests above and repair the vacuous cycle test before treating schema/example validation as a release gate.

No production file, schema, test, workflow, doc, or example was edited. This report is the only write.

## Parent remediation addendum, 2026-07-26

The parent implementation re-audited the explicitly supplied live checkout,
which is now at `27128a7b311d728a959afff7238a9aeeb9987f2b` with a clean working
tree. The committed source manifest is present, SHA-256
`aa9cb85e955227b3065aa9fa60b6acb5290d6e1b78168dc650206832f8e4e451`, but still
declares evidence generated for `599497ea2f93612d9094461c6fde114fc87a5c0f`.
The workspace manifest was regenerated from tracked Git archive bytes at the
current exact revision, with SHA-256
`8d4b80e78d4ca7695107c9ef7ac0ca8590e50d7a3b25241b94e659d1e0895b87` and 917
evidenced files. Runtime accepts the remote manifest only when its declared
generator revision matches the resolved commit; otherwise it requires this
exact-revision bundled bootstrap and records the manifest origin.

The parent also added strict selected-tree evidence coverage, exact-revision
fallback handling, pinned-release drift rejection, generated-file preservation
on update, dependency-closure selection, source size/platform provenance in
plans and locks, bounded expected-size fetching, and schema/example coverage.
The upstream publication drift and the current Windows linker/SDK limitation
remain explicit blockers rather than being treated as solved by self-hashing.

## Parent remediation addendum, 2026-07-28

The checked-in workspace manifest is now generated for the supplied live source
revision `27128a7b311d728a959afff7238a9aeeb9987f2b` and has SHA-256
`cddb7ece7235d033888d85508455c255ffe320f0f28bc924999e8f4ddd1c19b5`.
The current manifest marks `mcp.hoi4_agent_tools` optional and Windows-only.
`resolve_platform_support` therefore reports it as `unsupported_platform` on
macOS, while the platform-neutral core components remain supported and no
required component is blocked. The regression
`source::tests::core_profile_keeps_windows_only_mcp_nonblocking_on_macos`
locks this contract. The earlier SM-005 finding is closed; the other findings
remain historical evidence or external publication/release work unless a
later remediation addendum says otherwise.
