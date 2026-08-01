# Acceptance criteria

## Source and manifest

- SRC-01: The app installs from GitHub without cloning Agentic-HOI4-Modding.
- SRC-02: Latest mode resolves the default branch and records one commit.
- SRC-03: Pinned commit uses one immutable revision for manifest and files.
- SRC-04: Pinned release records release and commit when available.
- SRC-05: Every download is SHA-256 verified.
- SRC-06: Manifest mismatch blocks apply.
- SRC-07: Only selected component files are downloaded.

## New project and launcher readiness

- NEW-01: The user can enter a natural-language description.
- NEW-02: Selected-provider proposals are reviewable and editable before rendering.
- NEW-03: Project ID is stable and valid.
- NEW-04: No file is created before approval.
- NEW-05: The initial folder profile is editable.
- NEW-06: Apply creates the internal `descriptor.mod`.
- NEW-07: Apply creates the external `<project_id>.mod` in the confirmed HOI4 user mod directory.
- NEW-08: The launcher descriptor points to the canonical project root with platform-correct escaping.
- NEW-09: Both descriptors are previewed, parsed, and validated before apply and after staging.
- NEW-10: Apply creates a deterministic, valid `thumbnail.png` placeholder.
- NEW-11: The thumbnail preview and readiness check decode the final PNG successfully and record its hash.
- NEW-12: The descriptor picture reference resolves to the installed thumbnail where supported.
- NEW-13: A fresh project is discoverable by the HOI4 launcher without manual file creation or descriptor editing.
- NEW-14: The selected initial directories exist after apply.
- NEW-15: The app does not fabricate a Workshop ID or `remote_file_id`.
- NEW-16: A user-modified thumbnail is detected and never overwritten silently.
- NEW-17: A valid existing thumbnail may replace the generated placeholder without blocking readiness.
- NEW-18: Missing or invalid launcher artifacts block launcher-ready status and Open in Codex for a newly created project.
- NEW-19: Entering a mod name and brief populates project ID, script prefix, primary namespace, descriptor tags, and starter folders before review; each remains editable and a manual edit is preserved.
- NEW-20: Windows and macOS resolve the HOI4 user `mod` directory from the native user `Documents` location, including redirected Documents, without whole-computer search or a fixed guessed path.
- NEW-21: A validated project ID auto-fills the new project root and external launcher descriptor filename before review.
- NEW-22: The user can explicitly override either auto-filled path; changing the ID updates only untouched auto-filled fields and preserves explicit overrides.
- NEW-23: No project root or launcher descriptor is created during resolution, scan, planning, or staging.

## Deterministic scanning and required provider analysis

- ANA-01: Observable project facts come from the deterministic Rust scanner.
- ANA-02: Create, Import, Update, and Repair planning require the selected provider configuration, or a compatible local Codex App Server for Codex.
- ANA-03: Codex users sign in through the App Server managed ChatGPT flow; other profiles use their explicit verified endpoint and OS-vault credential route.
- ANA-04: Browser login is primary and device-code login is available as fallback.
- ANA-05: The Codex route has no OpenAI API key field or API-key fallback; non-Codex key fields are provider-scoped, vault-backed, and never serialized.
- ANA-06: Codex owns ChatGPT token persistence and refresh; provider adapters never persist hosted tokens.
- ANA-07: The application does not read, copy, serialize, or log ChatGPT tokens.
- ANA-08: Full email, account ID, plan type, usage, and rate limits are absent from project state and installation locks.
- ANA-09: All semantic identity and convention fields are proposed by the selected provider profile.
- ANA-10: Every Codex input manifest is visible and editable before transmission.
- ANA-11: Secrets, binaries, credential stores, Git objects, and unapproved files are excluded.
- ANA-12: Semantic turns use read-only sandboxing and expose no writable project root.
- ANA-13: Every response validates against `codex-analysis.schema.json`.
- ANA-14: No provider can write files, approve operations, resolve conflicts automatically, override deterministic facts, or pass readiness checks.
- ANA-15: Values are labeled Detected, Suggested by the selected provider, or Confirmed.
- ANA-16: Proposed IDs, namespaces, tags, profiles, and paths pass deterministic validation before confirmation.
- ANA-17: Authentication, usage, process, and malformed-response failures preserve the draft and start no transaction.
- ANA-18: Recovery, rollback, backup inspection, and managed removal remain available while signed out or disconnected.
- ANA-19: Codex does not hardcode a model; non-Codex users select a model and the profile is persisted as non-secret configuration.
- ANA-20: Stored analysis metadata contains only schema version, analysis ID, provider/model/profile, input and output digests, confirmed fields, and timestamps.
- ANA-21: Provider references are keyed by provider, hosted endpoints are explicit HTTPS, local endpoints are loopback HTTP, redirects are disabled, and response bodies are bounded before parsing.

## Provider profiles and flattened Chat sources

- AI-01: Codex, Claude, Kimi, GLM, DeepSeek, local, and custom provider profiles are selectable at the start.
- AI-02: The selected profile changes semantic guidance, adapted `AGENTS.md`, generated `README.md`, state, plan, lock, and maintenance review.
- AI-03: Provider changes clear stale analysis and cannot reuse a record from another provider or model.
- AI-04: A Codex-only final checkbox prepares `chatgpt_project_sources/`; non-Codex setup never renders or persists it as selected.
- AI-05: Flattening renames `.agents/skills/<skill>/SKILL.md` to `<skill>.md` and includes selected subagents, adapted AGENTS, and README.
- AI-06: Flattening rejects links, case-insensitive collisions, secret-shaped content, and bounded file/aggregate-size violations.
- AI-07: Flattening uses the normal dry-run, backup, staging, validation, apply, readiness, journal, and rollback path.
- AI-08: The final recommendation says to start planning using ChatGPT “Chat” and performs no upload, conversation, or planning action.

## Existing project

- EXT-01: The user selects one root.
- EXT-02: Scan performs no project writes.
- EXT-03: Structure, descriptors, launcher registration, thumbnail, Git, IDs, namespaces, naming, localisation, docs, skills, subagents, Codex, MCP, and conflicts are covered.
- EXT-04: Findings have evidence and confidence.
- EXT-05: Findings appear in small review groups.
- EXT-06: The user can accept, edit, reject, or defer values.
- EXT-07: Existing instructions and config are not silently replaced.
- EXT-08: Duplicate launcher registrations and mismatched paths are reported.
- EXT-09: A valid managed installation lock is detected without scanning or
  writing the `.hoi4-mod-setup/` metadata tree.
- EXT-10: Existing-project review offers repair or add-workflow maintenance
  when a managed setup is found.
- EXT-11: Repair can add a previously unselected 3D workflow from the exact
  locked source revision, preserving modified files and leaving core readiness
  usable when the optional key or platform route is unavailable.
- EXT-12: Before an existing-project scan, discovery inspects only direct
  launcher descriptor candidates in the selected root's immediate parent.
- EXT-13: The candidate path and matching evidence are visibly shown and the
  user explicitly confirms, declines, or cancels before any external
  descriptor is read.
- EXT-14: Discovery never searches sibling trees, other drives, or the whole
  computer.

## Components

- CMP-01: Source, destination, dependencies, platforms, tools, environment, validation, and update behavior are shown.
- CMP-02: Automatic dependencies remain visible.
- CMP-03: Dependency cycles block.
- CMP-04: Unsupported optional components remain visible and non-blocking.
- CMP-05: Unknown manifest major version blocks.

## Wiki

- WIKI-01: Wiki installs under `<mod_project>/paradox_wiki/`.
- WIKI-02: Required pages and media policy are validated.
- WIKI-03: Broken integrity blocks core readiness.
- WIKI-04: Missing formal source or license metadata is reported honestly.
- WIKI-05: Local modifications are not overwritten silently.

## 3D

- 3D-01: The exact 3D question appears.
- 3D-02: Meshy.ai and possible cost are explained.
- 3D-03: Key is stored outside the project.
- 3D-04: Project stores only an opaque reference.
- 3D-05: Approved processes receive `MESHY_API_KEY`.
- 3D-06: Key never appears in logs, manifest, lock, config, Git, or previews.
- 3D-07: Missing key leaves 3D incomplete.
- 3D-08: Missing key does not block core readiness.
- 3D-09: Packages, versions, commands, adapters, and tools come from repository evidence.
- 3D-10: No substitute MCP or Blender command is invented.
- 3D-11: Dependency and MCP health checks are visible.
- 3D-12: Current unsupported macOS route is reported.

## Super Events

- SE-01: The Optional workflows screen asks **Do you want to set up the Super Events workflow?** immediately after the exact 3D question.
- SE-02: Selecting it adds `workflow.super_events` and selectively downloads only the verified manifest-declared `.agents/skills/hoi4-super-events/` tree at the one bound source revision.
- SE-03: The component is provider-neutral, depends only on `core.skills`, and requires no credential, environment variable, external command, or provider-specific health route.
- SE-04: A declined install downloads neither the Super Events tree nor Super Events-specific `AGENTS.md` guidance.
- SE-05: Super Events is visible in readiness as optional/non-blocking and its state is remembered in the managed lock and bounded scan summary.
- SE-06: Update can add it from a target manifest; Repair can add it only when the same immutable locked source declares it, otherwise Repair directs the user to Update.
- SE-07: Existing or modified Super Events files use normal ownership and conflict rules and are never silently overwritten.

## Portrait workflow handoff

- POR-01: No LoRA or ComfyUI setup choice or dedicated screen appears.
- POR-02: No portrait preference, component, transaction action, lock state,
  maintenance option, or readiness check exists.
- POR-03: Successful readiness shows one concise external link to
  `https://github.com/klimPaskov/comfyui-hoi4-portraits`.
- POR-04: The link is a fixed HTTPS destination opened through the typed
  browser action and is not built from provider, manifest, project, or scan
  content.
- POR-05: The app does not install, configure, inspect, or report the external
  portrait workflow.

## MCP

- MCP-01: Servers show requirements, capabilities, variables, status, and health.
- MCP-02: TOML is merged structurally.
- MCP-03: Conflicting server ID requires review.
- MCP-04: Secrets are not literal TOML values.
- MCP-05: Unsupported commands do not run.

## Git

- GIT-01: Initialize, preserve, and skip are supported.
- GIT-02: `.gitignore` can be merged.
- GIT-03: Existing rules are preserved.
- GIT-04: New default branch is selectable.
- GIT-05: Initial commit is optional.
- GIT-06: Remote is optional.
- GIT-07: No online repository is created automatically.
- GIT-08: No push occurs automatically.
- GIT-09: The user can separately review and approve a push to an existing
  remote after the core readiness gate passes.
- GIT-10: The user can separately review and approve public GitHub repository
  creation, followed by a separate push approval.
- GIT-11: Online actions bind the root, named branch, clean tree, exact HEAD,
  destination, executable identity, and supported Git configuration; changed
  state invalidates the review.
- GIT-12: Completed online actions write a secret-free, schema-backed recovery
  record without storing credentials.

## Conflicts

- CON-01: Modified files always receive a decision.
- CON-02: Keep, replace, merge, rename, and skip are offered where valid.
- CON-03: Binary files do not offer text merge.
- CON-04: Merge preview is validated.
- CON-05: Bulk choices require identical signatures.
- CON-06: Choices are stored in lock.
- CON-07: Thumbnail conflicts preserve valid user art by default.
- CON-08: External launcher descriptor conflicts are included in backup, review, and rollback.

## Transaction

- TX-01: All 12 stages are present.
- TX-02: Backup completes before apply.
- TX-03: Staging validates before apply.
- TX-04: Apply is checkpointed.
- TX-05: A crash cannot produce a successful lock.
- TX-06: Resume verifies observed state.
- TX-07: Rollback restores original hashes in fault tests.
- TX-08: Completion creates a rollback record.
- TX-09: External launcher descriptor writes are transactional.
- TX-10: Descriptor and thumbnail checks run against staged and applied outputs.
- TX-11: An absent new-project root is represented as one reviewed `create_leaf`; the leaf is created exactly once and only at apply.
- TX-12: Rollback removes that leaf only after it is empty; unknown or later user content is preserved and the parent is never recursively removed.

## Maintenance

- MNT-01: Update compares base, local, and incoming.
- MNT-02: Repair defaults to locked revision.
- MNT-03: Missing or corrupted unmodified files can be restored.
- MNT-04: Modified files require review.
- MNT-05: Reinstall preserves safe merge policy.
- MNT-06: Removal deletes only owned unmodified files by default.
- MNT-07: Optional credentials can be configured later.
- MNT-08: Repair can recreate a missing launcher descriptor after preview.
- MNT-09: Repair never replaces a modified thumbnail automatically.
- MNT-10: Update requires a fresh read-only evidence manifest and core-confirmed selected-provider reanalysis before its maintenance plan is accepted.
- MNT-11: Update and Repair preserve `workflow.super_events` state and managed ownership; Repair uses only the locked revision for a later addition.

## Readiness

- RDY-01: Every required core and optional surface is checked.
- RDY-02: Each check has status and evidence.
- RDY-03: Internal descriptor, external launcher descriptor, launcher destination, descriptor agreement, and thumbnail integrity are blocking checks for a new project.
- RDY-04: Core blocks disable Open in Codex.
- RDY-05: Optional incomplete states do not disable it.
- RDY-06: Codex authentication or selected-provider configuration and confirmed analysis are blocking core readiness checks.
- RDY-07: A final report is stored.
- RDY-08: `workflow.super_events` has an evidence-backed optional state and never blocks core readiness; its no-credential state is not rendered as a missing credential.

## Desktop responsiveness

- CMD-01: Every Tauri desktop command dispatches filesystem, network, Git, provider, and other blocking waits through async/thread-pool work rather than the desktop event loop.
- CMD-02: A regression test holds representative waits open and verifies that the event loop remains responsive and cancellation remains observable while each command is pending.

## UI and accessibility

- UI-01: The setup rail uses seven grouped phases.
- UI-02: Each screen has one title and no more than one supporting sentence by default.
- UI-03: Each screen has one primary work area and no more than two visible content regions by default.
- UI-04: Evidence, hashes, dependency graphs, full file lists, logs, and advanced settings use progressive disclosure.
- UI-05: The same fact is not repeated across several interface regions.
- UI-06: Ordinary controls do not receive copy that only restates their label.
- UI-07: Clear Back and primary action controls remain reachable.
- UI-08: Progress includes the current stage and durable checkpoint without showing the full log by default.
- UI-09: Full keyboard use is supported.
- UI-10: Status does not rely on color.
- UI-11: 200 percent scaling works.
- UI-12: Project identity shows compact descriptor rows and thumbnail preview on demand.
- UI-13: Selected-provider analysis shows the approved request manifest and separates detected, suggested, and confirmed values without adding a permanent explanatory panel.
- UI-14: The flatten checkbox is native, keyboard accessible, Codex-only, appears in the Install review, and is followed on Ready by the ChatGPT “Chat” recommendation.
- UI-15: The Ready-screen portrait workflow link is keyboard accessible, has a
  clear name, and is not presented as setup or readiness state.
- UI-16: New-project identity fields are populated from the name and brief; optional metadata stays secondary and generated values remain keyboard-editable.
- UI-17: New-project root and launcher paths are visibly marked as auto-filled or overridden and remain editable.
- UI-18: Existing-project launcher descriptor candidates are visibly confirmed before scan and unconfirmed paths are not read.
- UI-19: The Ready-screen external link has a clear name and fixed safe destination.
- UI-20: The Optional workflows screen places the exact Super Events question directly after the exact 3D question and shows no Super Events credential control.
- UI-21: Ready and maintenance screens expose Super Events state and add/repair actions without adding a separate workflow phase or a LoRA/ComfyUI setup surface.

## Open-source repository

- OSS-01: The root README is user-facing and excludes internal build and branch instructions.
- OSS-02: Contributor, development, security, conduct, release, changelog, and license-decision files exist.
- OSS-03: A real LICENSE is blocking before public source or binary release.
- OSS-04: AGENTS.md distinguishes application development from target-mod instructions.
- OSS-05: At least eight focused living skills exist under `.agents/skills/`.
- OSS-06: Every living skill states update triggers or maintenance role.
- OSS-07: Narrow subagents exist for source, scanner, transaction, security, UI, platform release, docs, and skill maintenance.
- OSS-08: Every project subagent requires `fork_context=false` and explicit scope.
- OSS-09: Issue forms, pull request template, CODEOWNERS, and Dependabot configuration exist.
- OSS-10: GitHub workflow YAML parses and uses read-only default permissions.
- OSS-11: Main is protected through a pull-request ruleset with required checks.
- OSS-12: Release automation builds from an exact annotated tag and publishes a normal release only after all platform, signature, and curation gates pass.
- OSS-13: The public stable release contains exactly the Windows x64 installer and macOS arm64 and x64 disk images; checksums and platform verification remain in internal workflow evidence.
- OSS-14: Pull requests require documentation and living-skill review.
- OSS-15: Planning validation checks schemas, examples, YAML, TOML, skills, subagents, README boundary, and goal prompt length.
- OSS-16: The canonical goal prompt is included at `docs/GOAL_PROMPT.md` and remains no more than 4000 characters.
