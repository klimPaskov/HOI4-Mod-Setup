# HOI4 Mod Setup repository instructions

This file governs development of the **HOI4 Mod Setup** desktop application. It does not govern a Hearts of Iron IV mod that the application prepares. The application may generate or adapt an `AGENTS.md` inside a target mod project, but that generated file is a separate artifact with separate ownership.

## 1. Product promise

HOI4 Mod Setup prepares a new or existing Hearts of Iron IV mod for agentic development in Codex.

Every implementation must preserve these promises:

- inspect before changing an existing project
- create a launcher-discoverable, game-loadable new project with both descriptors and a valid placeholder thumbnail
- keep deterministic facts separate from required Codex semantic proposals until the user confirms them
- never clone the complete Agentic HOI4 Modding source repository
- resolve latest mode to an exact commit before downloading files
- support immutable pinned commit or release installs
- download only selected manifest-declared files
- verify downloaded content before apply
- never overwrite a user-modified file silently
- keep credentials outside the target project and installation lock
- use staged, journaled, reversible transactions
- keep optional workflows non-blocking for core Codex readiness
- show honest unsupported and incomplete states
- keep the desktop interface restrained and task-focused
- support Windows and macOS without inventing unsupported platform routes

The source planning package, accepted architecture decisions, schemas, and tests are part of the product contract. A change that weakens one of these promises requires an explicit design decision and matching documentation.

## 2. Required reading

For a first implementation pass or a broad architectural change, read:

1. `README.md`
2. `GOAL_PROMPT.md`
3. `docs/01_product_requirements.md`
4. `docs/02_user_flows.md`
5. `docs/03_scanner_design.md`
6. `docs/04_remote_repository_manifest.md`
7. `docs/09_component_dependency_model.md`
8. `docs/10_merge_conflict_rules.md`
9. `docs/13_security_model.md`
10. `docs/14_transaction_rollback.md`
11. `docs/16_platform_architecture.md`
12. `docs/17_ui_accessibility.md`
13. `docs/20_testing_strategy.md`
14. `docs/22_acceptance_criteria.md`
15. `docs/26_open_source_github_workflow.md`
16. `docs/27_repo_local_skill_strategy.md`
17. `docs/28_agents_subagent_architecture.md`
18. `docs/30_codex_chatgpt_authentication.md` when authentication or analysis is involved
19. the repo-local skill that owns the current task

For a bounded change, read the owning skill and the directly relevant design documents. Do not scan unrelated source trees or generated artifacts when exact files are already known.

## 3. Source-of-truth order

Use this precedence when sources disagree:

1. explicit user instruction for the current task
2. accepted product requirement or architecture decision
3. JSON Schema and transaction invariants
4. owning repo-local skill
5. implementation tests and current verified behavior
6. general documentation
7. examples and UI references

Examples are not allowed to override schemas or security rules. Current implementation is evidence of behavior, not automatic approval to change the intended product.

## 4. Architecture boundary

The planned architecture is a Tauri desktop shell with a Rust core and a TypeScript React interface.

The Rust side owns:

- filesystem access
- path normalization and containment
- project scanning
- descriptor parsing and rendering
- source resolution and downloads
- manifest validation
- hashing and cache integrity
- merge planning
- transaction journal, backup, apply, resume, and rollback
- credential store integration
- external process allowlisting and execution
- Git operations
- MCP health checks
- readiness evaluation
- platform adapters

The React side owns:

- wizard navigation
- editable review state
- evidence and preview presentation
- conflict comparison interaction
- progress presentation
- readiness presentation
- accessible keyboard and screen-reader behavior

The UI must not perform direct filesystem writes, run external commands, access credentials, or decide whether a transaction is safe. It calls typed Tauri commands and renders returned state.

Keep core domain logic independent from UI components and platform APIs. Platform-specific code belongs behind explicit interfaces. Test the core with fake filesystem, source, credential, process, Git, and clock adapters.

## 5. Repository source rules

Use "C:\Users\klimp\OneDrive\Documents\Paradox Interactive\Hearts of Iron IV\mod\agentic_hoi4_modding" as the live workflow source.

The application must:

- resolve the repository default branch in latest mode
- resolve an exact commit before reading the install manifest
- use one revision for the manifest and all selected source files
- support exact commit and release pinning
- selectively fetch manifest-declared files or bundles
- verify SHA-256 before staging
- retain source revision and hash evidence in the installation plan and lock
- reject path traversal, duplicate destinations, platform mismatches, unsupported manifest majors, and checksum mismatch

The application must not:

- clone the complete workflow source repository
- search the computer for a local checkout
- mix files from different revisions
- trust filenames or remote redirects as integrity proof
- invent missing wiki provenance, license data, package names, commands, MCP servers, or platform support

Use `hoi4-mod-setup-source-manifest` for changes to this surface.

## 5A. ChatGPT authentication and Codex App Server

ChatGPT sign-in is a core prerequisite for Create, Import, Update, and Repair planning. Use the official local `codex app-server` process over stdio JSONL. Do not implement an OpenAI API key field or an application-owned OAuth service.

Required behavior:

- complete the App Server initialize handshake before account or thread requests
- read account state with `account/read`
- accept the normal product path only when the account type is `chatgpt`
- start browser login with `account/login/start` using `type = chatgpt`
- offer `chatgptDeviceCode` when the browser callback cannot complete
- wait for login completion and account update notifications
- use `account/logout` for sign-out
- let Codex own token persistence and refresh
- never inspect, copy, persist, or log ChatGPT tokens
- never write email, account ID, plan type, usage, or rate limits into a project or installation lock
- do not use the experimental externally managed token mode
- do not hardcode a model name for the normal path

Every semantic turn uses a dedicated thread, read-only sandboxing, approved inputs, and the current `codex-analysis` output schema. Codex proposes names, IDs, namespaces, descriptions, tags, folder profiles, project instructions, and component choices. Deterministic Rust validates and renders the final bytes after user confirmation.

Codex cannot write project files during analysis, approve a transaction, resolve conflicts automatically, or pass readiness checks. Missing authentication or usage availability blocks new planning. Recovery, rollback, backup inspection, and managed removal remain locally available while signed out.

Use `hoi4-mod-setup-codex-integration` for changes to this boundary.

## 6. Existing project scanner

The existing-project scan is read-only. It may read the selected project root and explicitly approved external descriptor paths. It must not create cache markers, temporary files, Git locks, or transaction folders inside the project during scan. The scanner is deterministic Rust code and owns all observable facts. Required Codex semantic analysis runs only after ChatGPT-managed authentication, receives only approved inputs, returns schema-validated proposals, and performs no writes.

Every finding needs:

- stable finding ID
- category
- status
- confidence
- evidence paths and excerpts or hashes
- proposed value or action
- editable user decision
- blocking or non-blocking severity

Low confidence is a visible state. Do not turn a guess into a detected fact. Label values as `Detected`, `Suggested by Codex`, or `Confirmed`. Codex proposals never override descriptor validity, hashes, paths, encoding, Git state, file existence, identifier validity, collisions, or other deterministic evidence.

Use `hoi4-mod-setup-project-scanner` for scanner changes.

## 7. Transaction contract

Every project mutation uses this ordered transaction:

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

No shortcut may bypass backup, staging validation, or journal persistence. A successful lock file is written only after final verification.

Operations must be:

- deterministic
- idempotent where practical
- checkpointed
- resumable or explicitly roll-backable
- root-contained
- hash-aware
- safe under interruption

Fault injection at every stage and operation boundary is required for transaction changes.

Use `hoi4-mod-setup-transactions` for changes to plans, apply, recovery, repair, rollback, reinstall, or managed removal.

## 8. Merge and ownership rules

Managed content has one of four ownership types:

- `managed`
- `merged`
- `generated`
- `external`

Never silently replace a locally modified file. Compare base, local, and incoming content where a base exists. Offer only valid actions from keep, replace, merge, rename, and skip.

Binary files never use text merge. TOML and JSON use structured merge. `AGENTS.md` uses three-way merge plus project adaptation. A merge result must validate before apply.

## 9. Credential and process rules

Secrets belong in Windows Credential Manager or macOS Keychain. The project stores only a non-secret credential reference.

Never write a secret to:

- project files
- `.env` files generated by the app
- logs
- manifests
- installation plans
- installation locks
- crash reports
- analytics
- screenshots
- process previews
- Git history

Pass credentials only to an allowlisted child process through a scoped environment. Redact both known values and credential-shaped output before persistence or display.

Run processes through argument arrays. Do not construct shell command strings from user or manifest input. Show command, working directory, environment variable names, network access, expected writes, and privilege needs in dry run. Never display secret values.

Use `hoi4-mod-setup-security` for this surface.

## 10. Optional workflow rules

The 3D setup question must be exactly:

**Do you want to set up the 3D models workflow?**

A missing or invalid `MESHY_API_KEY` leaves the optional workflow incomplete and keeps core setup usable. Derive packages, commands, versions, adapters, Blender integration, and health checks from verified repository files. Do not invent a macOS route for a Windows-only repository workflow.

The portrait workflow question must be exactly:

**Do you want to set up LoRAs and ComfyUI for portrait generation?**

Version 1 records interest only. It must not download, install, configure, or modify ComfyUI, models, LoRAs, Python environments, GPU software, or drivers. It never reports installed or ready.

## 11. UI rules

Use the seven grouped phases:

- Project
- Review
- Components
- Integrations
- Git
- Install
- Ready

Each screen should normally contain:

- one title
- zero or one short supporting sentence
- one focal task
- no more than two visible content regions
- persistent Back and primary action controls where applicable

Hide evidence, full file lists, hashes, dependency graphs, logs, and advanced settings behind progressive disclosure. Do not explain obvious controls or repeat the same fact in several places.

The merge conflict screen may show more detail because the comparison is the task.

Meet WCAG 2.2 AA, complete keyboard operation, visible focus, reduced motion, non-color status cues, and 200 percent scaling.

Use `hoi4-mod-setup-ui-accessibility` for interface work.

## 12. Testing and validation

Use the repository scripts as the stable command surface. Do not make contributors memorize internal package commands that a script can own.

The expected test layers are:

- Rust unit and property tests
- parser and schema fuzzing
- integration tests with fake GitHub, filesystem, credential, process, and Git adapters
- transaction fault injection
- Windows and macOS end-to-end tests
- security tests
- UI accessibility and visual regression tests
- performance tests on small, medium, and very large projects

A change is incomplete when the relevant failure path is untested. Happy-path screenshots are not transaction, security, or recovery evidence.

Use `hoi4-mod-setup-testing` for test architecture and release-gate changes.

## 13. Open-source repository rules

Keep `README.md` user-facing. Contributor setup belongs in `CONTRIBUTING.md` and `DEVELOPMENT.md`. Release maintenance belongs in `RELEASING.md`. Security reporting belongs in `SECURITY.md`.

Use pull requests for `main`. Keep changes focused. Update documentation and the owning skill in the same pull request when behavior, commands, file locations, invariants, validation, or common failure modes change.

Do not commit:

- real credentials
- private mod projects
- signing keys or certificates
- notarization credentials
- generated release packages
- transaction backups
- local caches
- unredacted logs

Use `hoi4-mod-setup-open-source-release` for GitHub workflow, dependency update, release, packaging, and community-health changes.

## 14. Living skill contract

Repo-local skills are maintained implementation memory. They are not frozen templates.

Update the owning skill in the same pull request when a change introduces or alters:

- a repeated workflow
- an invariant
- a command
- a source path
- a schema or migration rule
- a platform difference
- a validation step
- a common failure and recovery method
- a security boundary
- a handoff format

Do not add one-off ticket details, temporary debugging context, or feature-specific prose to a general skill.

Before completion, run the skill ownership check described in `.agents/skills/hoi4-mod-setup-skill-maintenance/SKILL.md`.

## 15. Subagent routing

All custom subagents are spawned with `fork_context=false`. The parent prompt must contain the exact scope, files, constraints, accepted decisions, and handoff path.

Use:

- `hoi4setup_codex_integration_auditor` for ChatGPT authentication, App Server, structured analysis, and data-boundary audits
- `hoi4setup_source_manifest_auditor` for source, manifest, selective download, and wiki distribution audits
- `hoi4setup_scanner_auditor` for read-only scan evidence and classification audits
- `hoi4setup_transaction_recovery_auditor` for journal, apply, recovery, rollback, and fault-injection audits
- `hoi4setup_security_auditor` for credentials, process execution, filesystem containment, redaction, and supply-chain audits
- `hoi4setup_ui_accessibility_auditor` for bounded UI and accessibility audits or narrow fixes
- `hoi4setup_platform_release_auditor` for Windows, macOS, signing, notarization, packaging, and release evidence
- `hoi4setup_documentation_curator` for documentation-only consistency work
- `hoi4setup_skill_maintainer` for repo-local skill creation and updates

Subagents do not own final completion. The parent reviews every handoff and reruns the relevant final tests.

## 16. Completion report

For meaningful work, report:

- files changed
- product surfaces changed
- schemas or migrations changed
- security and credential impact
- transaction and rollback impact
- platform impact
- tests and fault scenarios run
- UI evidence when visible behavior changed
- documentation updated
- skills updated or a reason no skill update was needed
- subagent handoffs reviewed
- blockers, unsupported routes, and simplifications

Do not claim completion because the app builds or a single happy path works. Completion means the accepted behavior, failure handling, documentation, and relevant tests agree.

## 17. Codex integration completion gate

A task that changes authentication or semantic analysis is incomplete until browser login, device-code login, cancellation, logout, usage limits, App Server interruption, output-schema rejection, redaction, and no-secret persistence are covered by tests. Run `hoi4setup_codex_integration_auditor` before the parent claims the integration is complete.
