# Source audit and verification record

## Audit scope

This planning package uses two evidence sets:

1. The project files supplied with the request, including all Markdown skills, all supplied subagent TOML files, the project instruction file, the mechanics guide, and the current event, cluster, and scenario catalog CSV snapshots.
2. The initial audit resolved the live `klimPaskov/Agentic-HOI4-Modding` repository to commit `27128a7b311d728a959afff7238a9aeeb9987f2b` on `main` on 26 July 2026. The Agentic source branch used for this update is `d37c26c8565269d4aac5ade5ec4f0c7790964909`; it contains the portrait lock removal and Super Events filename adaptation contract. The portrait integration adds a provider-neutral portrait contract, one selected-provider skill component, a bounded subagent, and non-secret config containing the exact upstream revision alongside `workflow.super_events`. The application resolves the remote manifest at runtime and retains the bundled copy only as offline bootstrap evidence.

Machine-readable inventories are in `source-audit/uploaded_sources_inventory.json` and `source-audit/live_repository_inventory.json`. The checked-in app manifest evidence is generated from Agentic source revision `d37c26c8565269d4aac5ade5ec4f0c7790964909`.

## Fully read and processed

The supplied text sources were loaded, parsed, section-indexed, and reviewed. This covers `AGENTS(4).md`, `CHAOS_REDUX_MECHANICS(4).md`, every supplied skill Markdown file, every supplied `chaosx_*.toml` subagent file, and all three supplied catalog CSV snapshots.

The live repository inspection covered every current generic skill and every current generic Codex subagent TOML resolved at the inspected commit. It also covered:

- `README.md`
- `AGENTS_template.md`
- `AGENTS_chaos_redux.md`
- `.codex/config.toml`
- the generic 3D skill and 3D subagent
- `.tools/3d_pipeline/bootstrap_3d_workflow.py`
- the Meshy and Blender wrapper commands
- the checked-in 3D dependency record
- the offline wiki directory structure and observed page names
- the wiki snapshot marker filename
- `.gitignore`
- the remaining cronjob documentation

## Verification limits

The body of every offline wiki article was not fully read. The installer design inspected the distribution, observed page set, required core page names, and snapshot marker. Binary media in the wiki and visual reference libraries were not individually inspected. Their paths, containment, hashes, and component ownership belong to installation validation.

No formal root `LICENSE` file or `paradox_wiki/LICENSE` file was found at the tested paths. The repository README contains permissive wording, but this package does not treat that wording as a verified formal license. The proposed manifest records repository license evidence as `declared_unverified` and wiki license status as `not_found`.

The updated root manifest at commit
`1017ccf22ba326c185dd006b8d2cf512d24d3bd1` has raw SHA-256
`1c6a04f0388dea8588b2a22661f5c347ba24370fc1060851265235ed12d7aa8b`
and declares `generated_for_revision`
`d37c26c8565269d4aac5ade5ec4f0c7790964909`. Its declared file records
cover 23 components and were generated from immutable Git blob bytes before
publication.

The manifest is now published upstream infrastructure. Runtime resolution
still verifies the manifest and every selected blob against one exact source
revision before staging.

## Live repository inventory

| Group | Observed role |
| --- | --- |
| `.agents/skills/` | Generic reusable HOI4 workflows |
| `.codex/agents/` | Generic bounded Codex subagents |
| `.codex/config.toml` | Codex and MCP configuration example |
| `.tools/3d_pipeline/` | Optional 3D bootstrap and support files |
| `paradox_wiki/` | Offline Markdown snapshot and media |
| `AGENTS_template.md` | Mod-agnostic project instruction template |
| `AGENTS_chaos_redux.md` | Full project-specific example |
| `README.md` | Setup guidance and repository overview |

Generic skills reviewed:

- `hoi4-3d-model-pipeline`
- `hoi4-decisions-missions`
- `hoi4-events`
- `hoi4-feature-assets`
- `hoi4-feature-planning`
- `hoi4-focus-trees`
- `hoi4-frame-animation`
- `hoi4-improvement-loop`
- `hoi4-mtth`
- `hoi4-subagents`
- `hoi4-text-audio-research`

Generic subagents reviewed:

- `hoi4_3d_model_pipeline`
- `hoi4_asset_source_researcher`
- `hoi4_audio_researcher`
- `hoi4_country_package_auditor`
- `hoi4_decision_mission_auditor`
- `hoi4_documentation_curator`
- `hoi4_feature_completion_auditor`
- `hoi4_focus_tree_auditor`
- `hoi4_generated_feature_art`
- `hoi4_icon_artist`
- `hoi4_improvement_loop_planner`
- `hoi4_localisation_auditor`
- `hoi4_quote_remark_researcher`
- `hoi4_repo_explorer`
- `hoi4_scripted_system_architect`
- `hoi4_skill_maintainer`
- `hoi4_spreadsheet_doc_worker`

## Findings that shape the product

### Exact commit content must be authoritative

Repository directory pages and cached listings can lag or differ. The application must resolve a commit first and expand that exact tree. A branch directory listing alone cannot define an installation.

### Current MCP configuration is Windows-specific

The inspected `.codex/config.toml` uses `hoi4-agent-tools.cmd`. The application must not claim a verified macOS route until the repository declares and validates one.

### Current 3D setup is Windows-oriented

The bootstrap and wrappers use `.cmd`, PowerShell guidance, `winget`, `blender.exe`, Windows Program Files, and LocalAppData. The app must not invent Homebrew packages, shell scripts, Blender paths, or macOS commands. On macOS, this optional workflow is `unsupported_platform` until the repository adds a route.

### 3D version policy documentation has drift

The README describes pinned dependencies. The bootstrap resolves several dependencies at setup time and records observed versions and hashes. The app must show executable repository behavior as the source of truth, preserve the exact observed resolution in the project lock, and surface the wording mismatch to maintainers.

### Generic files still contain project-specific examples

Some reusable skills include absolute Chaos Redux paths or project-specific reference locations. The scanner must report them. AGENTS adaptation must replace, preserve by explicit approval, or remove them. They cannot become defaults for another mod.

### Wiki provenance is incomplete

The wiki is a Markdown and media tree with an observed marker `_last_updated_on_27_Nov_2025.txt`. No formal source or license metadata was verified. The app may install repository content, but it must show provenance as repository-only and licensing as unverified.

## Catalog observations

The supplied Chaos Redux event catalog snapshot contains hundreds of mixed-state records, including finished, new, needs-testing, blank, malformed, and shifted rows. The cluster and scenario snapshots contain smaller registries. This supports evidence-backed schema validation, explicit source ownership, and a rule against silent normalization. The application does not edit these catalogs during setup.

## Source precedence

1. The exact live repository commit defines the current reusable package.
2. Supplied project files define Chaos Redux-specific expectations and deeper examples.
3. Differences are recorded and reviewed. They are never silently resolved.
4. Repository scripts define actual executable behavior. Documentation remains guidance and evidence.

## Repository work remaining before production

- declare platform support for command-bearing components where the source
  provides a verified route
- add wiki source and license metadata when available
- define stable MCP health checks with immutable executable provenance
- add preflight-only output to scripts that install external dependencies
- publish a machine-readable list of runtime-generated files

## Open-source GitHub repository references

The open-source repository additions were checked against current official GitHub documentation for repository customization and licensing, issue and pull request templates, CODEOWNERS, Dependabot version updates, and security policies on July 25, 2026.

The evidence inventory is stored in `source-audit/github_repository_practices.json`.

The package does not claim that repository settings, rulesets, signing environments, or private vulnerability reporting are active. Those require configuration in the final GitHub repository.

## OpenAI Codex integration verification, 2026-07-26

The official Codex App Server documentation was inspected for the product integration boundary. It identifies App Server as the deep product integration interface, documents stdio JSONL transport, ChatGPT-managed browser and device-code authentication, account state, rate-limit methods, and per-turn `outputSchema`.

The official ChatGPT plan help article confirms that users access Codex by signing in with their ChatGPT account and that availability and usage limits depend on current product policy.

The resulting product contract uses ChatGPT-managed authentication and does not request an OpenAI API key. Evidence is recorded in `source-audit/openai_codex_app_server.json`.
