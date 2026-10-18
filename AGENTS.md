# HOI4 Mod Setup repository instructions

This file governs development of the **HOI4 Mod Setup** desktop application. It does not govern a Hearts of Iron IV mod that the application prepares. The application may generate or adapt an `AGENTS.md` inside a target mod project, but that generated file is a separate artifact with separate ownership.

## 1. Product promise

HOI4 Mod Setup prepares a new or existing Hearts of Iron IV mod for agentic development with a user-selected AI provider. Claude, signed in through the user's own Claude Code with Claude Haiku 5.5, is the default setup assistant. The checked-in Claude catalog keeps the legacy Haiku 4.5 choice and accepts a bounded exact model ID under Advanced.

Every implementation must preserve these promises:

- inspect before changing an existing project
- create a launcher-discoverable, game-loadable new project with both descriptors and a valid placeholder thumbnail
- keep deterministic facts separate from required semantic proposals until the user confirms them
- never clone the complete Agentic HOI4 Modding source repository
- resolve latest mode to an exact commit before downloading files
- if unauthenticated GitHub REST resolution hits a rate-limit-only 403/429,
  use only the fixed repository's read-only Git ref advertisement for names and
  commit IDs; never use it for object downloads or a checkout
- support immutable pinned commit or release installs
- download only selected manifest-declared files
- verify downloaded content before apply
- never overwrite a user-modified file silently
- keep credentials outside the target project and installation lock
- use staged, journaled, reversible transactions
- keep optional workflows non-blocking for core AI readiness
- show honest unsupported and incomplete states
- keep the desktop interface restrained and task-focused
- support Windows, macOS and Linux without inventing unsupported platform routes

The source planning package, accepted architecture decisions, schemas, and tests are part of the product contract. A change that weakens one of these promises requires an explicit design decision and matching documentation.

## 2. Required reading

For a first implementation pass or a broad architectural change, read:

1. `README.md`
2. `docs/GOAL_PROMPT.md`
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
19. `docs/31_ai_provider_profiles_and_chat_sources.md` when provider selection or flattened Chat sources are involved
20. the repo-local skill that owns the current task

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

The live workflow source is the public [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding) repository. To test unpublished package changes end to end, set `HOI4_MOD_SETUP_SOURCE_REPOSITORY` to the absolute path of a local checkout before starting the app; it then serves the checkout's committed files in place of GitHub.

The application must:

- consume the single canonical `hoi4-mod-setup.manifest.json` route and use
  its required `schema_version` for compatibility; do not publish or consume
  parallel filename routes as update history
- resolve the repository default branch in latest mode
- resolve an exact commit before reading the install manifest
- use one revision for the manifest and all selected source files
- support exact commit and release pinning
- selectively fetch manifest-declared files or bundles
- use each selected component's manifest-declared `expected_files` inventory
  for public installation rather than requiring a recursive repository tree
- verify SHA-256 before staging
- retain source revision and hash evidence in the installation plan and lock
- reject path traversal, duplicate destinations, platform mismatches, unsupported manifest majors, and checksum mismatch
- use the bounded source-resolution memory cache (at most 15 minutes) only as
  a resolution optimization; cache hits retain the exact revision and manifest
  evidence and never mix revisions
- consume compatible newly published skills, subagents, default-profile
  components, and optional workflow declarations from the exact resolved
  manifest without requiring an app release; preserve dependency, provider,
  platform, checksum, conflict, and user-choice gates

The Agentic repository owns manifest file-evidence generation. Changes under a
declared component tree are published with exact path, size, and SHA-256
evidence by its source workflow; HOI4 Mod Setup must not hand-maintain or infer
those file lists.

The application must not:

- clone the complete workflow source repository
- search the computer for a local checkout
- mix files from different revisions
- trust filenames or remote redirects as integrity proof
- invent missing wiki provenance, license data, package names, commands, MCP servers, or platform support

Use `hoi4-mod-setup-source-manifest` for changes to this surface.

## 5A. AI provider authentication and Codex App Server

The default setup assistant is the Claude account route (`claude_account`). It runs the user's own installed, unmodified, Anthropic-signed Claude Code: `claude auth login` owns sign-in, `claude auth status --json` supplies a non-identifying summary, and analysis is one isolated print-mode turn with tools, MCP servers, customizations, and session persistence disabled, run in an empty working directory with Claude Haiku 5.5 by default. The checked-in catalog keeps legacy Haiku 4.5 and the Advanced field accepts a bounded exact Claude model ID without requiring an app release. Anthropic does not permit third-party apps to offer Claude.ai login or to collect, store, or intermediate Claude credentials, so never build an app-owned Claude OAuth flow, read Claude Code's credential store, accept a pasted authorization code, or forward `ANTHROPIC_*` credentials to the child. The full boundary is in `docs/31_ai_provider_profiles_and_chat_sources.md`.

Experimental Gemini and Grok account adapters are separate from the hosted API
profiles. Their official CLI owns OAuth, token storage, refresh, and logout;
the app fetches only the explicitly pinned package after the user chooses sign
in, runs it from an app-private isolated home, and never reads or copies its
credentials. Current worker support is Windows/Linux only; macOS remains
unavailable until a verified terminal/runtime route exists. Provider-owned
session persistence is permitted only for approved analysis inputs in that
isolated home and never for target-project writes or app-managed records. See
the focused decision in `docs/34_provider_account_cli_architecture.md`.

Codex remains a first-class provider. ChatGPT sign-in is a core prerequisite for Codex Create, Import, Update, and Repair planning. Use the official local `codex app-server` process over stdio JSONL. Do not implement an application-owned OAuth service or an OpenAI API-key fallback for the Codex route.

The first setup screen also supports the bounded provider registry in `docs/31_ai_provider_profiles_and_chat_sources.md`. Claude API key, Gemini API key, Grok API key, Kimi, GLM, and DeepSeek use checked-in model/address defaults verified against official provider documentation plus an OS-vault API key; those non-secret defaults remain editable under Advanced. The reviewed Gemini OpenAI-compatible route defaults to `gemini-3.8-flash`; the reviewed Grok Responses route defaults to `grok-4.7`. Experimental `gemini_account` and `grok_account` CLI-owned login adapters are a separate in-progress surface documented in the account-adapter decision; they must not be described as a stable account route until their integration and live tests pass. Custom hosted routes require an explicit HTTPS address and OS-vault API key. Local models require an explicit loopback HTTP address and do not claim a hosted account. The app must not invent provider URLs, login routes, packages, commands, model names, MCP servers, or platform support.

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
- do not hardcode a model name for the Codex path; the selected setup model is persisted only as non-secret analysis provenance
- native structured-output request formats are required for Anthropic, Gemini,
  and Grok; truncated, refused, unreadable, transport, authentication, and
  usage-limit responses are discarded without a corrective retry

The provider, model, and reasoning effort selected on the first screen are the
**setup assistant**. They do not select, install, recommend, or restrict the AI
client used later for Agentic HOI4 Modding. Generated `AGENTS.md` and README
content must remain provider-neutral. Development-client integrations such as
`codex.config`, Open in Codex, the optional ChatGPT packager, and provider-neutral
MCP tooling are selected or enabled independently of the setup assistant.

Every semantic turn uses approved inputs and the current `codex-analysis` output schema. The runtime validator is authoritative even when a provider advertises native structured output. The core replaces the model-supplied analysis ID with its own UUID, accepts empty evidence references for genuinely inferred proposals, rejects unknown references, and keeps the scan/hash/session approval binding. Codex turns use a dedicated App Server thread and restricted read-only sandbox. Provider adapters never write files or approve transactions. The selected setup profile shapes semantic proposals for names, IDs, namespaces, descriptions, tags, folder profiles, and component recommendations. It may help adapt project-specific guidance, but it must not insert the setup provider, model, reasoning effort, or provider-specific development convention into the generated project. Deterministic Rust validates and renders the final provider-neutral bytes after user confirmation.

No AI provider can write project files during analysis, approve a transaction, resolve conflicts automatically, or pass readiness checks. Missing authentication/configuration or usage availability blocks new planning. Recovery, rollback, backup inspection, and managed removal remain locally available while signed out or disconnected.

Use `hoi4-mod-setup-codex-integration` for changes to this boundary.

## 5B. Coding-environment selection

The Components phase includes a separate **Coding Environments** step. Require
exactly one primary environment. New installs use the manifest-declared
default (the current package defaults to Claude Code); any other supported
environment may be selected as an additional client. The primary is never
listed as an additional choice, and this selection is independent of the setup
assistant and Core/Core plus 3D workflow profiles.

Resolve complete native packages from manifest components tagged with
`coding_environment`. Always include the shared runtime-neutral package:
canonical adapted `CLAUDE.md`, `.claude/skills/`, and `.claude/agents/`, the
adapted `AGENTS.md` projection, `.tools/sync/`, shared documentation, and
selected workflows. The Claude instruction, skill, and Markdown-subagent files
are the source package; every other client's instructions, agents, and skill
copies are generated projections and must pass source-side drift checks.

Manifest format 6 declares each client's MCP support: a `project` file the
installer manages, a `manual` route in settings the client owns, or `none`.
Offer a format 6 client only when its project route matches an adapter in
`src-tauri/src/coding_environment.rs`, or when its route is manual or absent.
Never write a client's user, IDE, or cloud settings. A component may serve
several clients through `shared_coding_environments`, so one installed copy
never duplicates a destination. Format 5 and older manifests offer only the
five clients they shipped. The catalog, evidence, and unsupported routes are
in `docs/36_coding_environment_catalog.md`.

Persist primary and additional IDs in project state, plans, locks, and journals.
Existing-project scans preserve a valid recorded selection, including a prior
Codex primary. Legacy projects without a recorded primary use Codex as the
compatibility migration default; new installs follow the resolved manifest
default. Update, repair, reinstall, rollback, and removal must preview client
changes, preserve existing or modified files, and remove only unchanged
managed files when a client is deselected. Use
`hoi4-mod-setup-transactions` and `hoi4-mod-setup-source-manifest` for changes
to this surface.

## 6. Existing project scanner

The existing-project scan is read-only. It may read the selected project root and explicitly approved external descriptor paths. It must not create cache markers, temporary files, Git locks, or transaction folders inside the project during scan. The scanner is deterministic Rust code and owns all observable facts. Required semantic analysis runs only after the selected provider is configured or Codex is ChatGPT-authenticated, receives only approved inputs, returns schema-validated proposals, and performs no writes.

Every finding needs:

- stable finding ID
- category
- status
- confidence
- evidence paths and excerpts or hashes
- proposed value or action
- editable user decision
- blocking or non-blocking severity

Low confidence is a visible state. Do not turn a guess into a detected fact. Label values as `Detected`, `Suggested by <selected provider>`, or `Confirmed`. Provider proposals never override descriptor validity, hashes, paths, encoding, Git state, file existence, identifier validity, collisions, or other deterministic evidence.

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

Binary files never use text merge. TOML and JSON use structured merge.
`CLAUDE.md` is the canonical instruction merge plus project adaptation;
`AGENTS.md` is its provider-neutral projection and is also reviewed through a
three-way merge. A merge result must validate before apply.

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

The optional-workflow titles are declarative:

**3D models workflow**

**Super Events workflow**

A missing or invalid `MESHY_API_KEY` leaves the optional workflow incomplete and keeps core setup usable. Derive packages, commands, versions, adapters, Blender integration, and health checks from verified repository files. Do not invent a macOS route for a Windows-only repository workflow.

The HOI4 portrait-production workflow is a bounded optional
workflow with Cloud, Local, RunPod, and Disabled choices. Its non-secret
provider state is persisted through project setup, import, settings, repair,
and readiness; provider credentials stay outside the project. LoRA is not a
separate setup option, component, project preference, or readiness state, and
the app does not invent a general ComfyUI route beyond this portrait contract.
Enabled projects install the verified complete portrait-production contract,
the explicit provider router, exactly one selected provider skill, the bounded
portrait subagent, and non-secret configuration through the manifest dependency
graph. Disabled projects receive none of that optional component closure.
When enabled, the Ready screen may show one concise external link to
`https://github.com/klimPaskov/comfyui-hoi4-portraits`; disabled projects retain
source-based portrait handling without ComfyUI-specific project instructions.

The startup updater always surfaces a newer signed app version, downloads and
verifies it, replaces the current installation, and restarts. A failed update
leaves the running version usable and exposes retry.

The app does not package ChatGPT project sources. The packager is the optional
`runtime.chatgpt_sources` manifest component, a `.tools` script installed only
when the user checks it on Components and run only by the user; no plan
prepares a `chatgpt_project_sources/` folder. See
`docs/33_chatgpt_source_packaging.md`.

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
- Windows, macOS and Linux end-to-end tests
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
- `hoi4setup_platform_release_auditor` for Windows, macOS, Linux, signing, notarization, packaging, and release evidence
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
