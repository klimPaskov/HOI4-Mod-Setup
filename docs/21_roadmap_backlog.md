# Implementation roadmap and backlog

## Phase 0: repository contract

Add and validate the remote manifest, generate a resolved file index, declare platform support, publish wiki provenance state, define MCP health checks, add script preflight output, and reconcile README wording with executable bootstrap behavior.

**Exit:** a manifest validator resolves a commit and lists selected files without cloning.

## Phase 0A: Codex account foundation

- resolve and verify `codex app-server`
- implement initialize and process supervision
- implement `account/read`
- implement browser ChatGPT login
- implement device-code fallback
- implement logout and account updates
- implement usage-limit state
- prohibit API-key fallback
- add redacted protocol fixtures
- add the Codex analysis schema and typed client

## Phase 1: application skeleton

- Tauri shell
- async/thread-pool Tauri command dispatch and event-loop responsiveness regression
- Rust service boundary
- React wizard shell
- local settings and recent projects
- error model
- redacted structured logging
- Windows and macOS packaging prototypes

## Phase 2: project identity and descriptors

- new description
- identity fields
- descriptor parser and renderer
- launcher descriptor adapter and duplicate-registration detection
- native redirected-Documents resolution and project-ID path defaults with
  editable overrides
- deterministic placeholder thumbnail renderer
- thumbnail preview, decode, hash, and replacement policy
- folder profile
- launcher-ready validation

## Phase 3: scanner

- filesystem boundary
- descriptors
- Git
- namespaces and identifiers
- localisation
- docs
- skills
- subagents
- Codex and MCP
- required selected-provider semantic review; Codex uses App Server after ChatGPT authentication
- request manifest and schema-constrained suggestions
- Detected, Suggested by the selected setup assistant, and Confirmed states
- finding review

The existing-project scanner phase also includes bounded immediate-parent
launcher descriptor discovery and visible confirmation before any external
descriptor read.

## Phase 4: source and manifest engine

- GitHub resolver
- latest and pinned modes
- immutable cache
- selected tree expansion
- SHA-256
- component graph
- platform resolver

## Phase 5: plan and conflicts

- operation model
- AGENTS merge
- TOML merge
- binary conflict UI
- rename and skip
- dry-run report

## Phase 6: transaction and recovery

- backup
- staging
- validation
- atomic apply
- journal
- rollback
- interruption recovery

## Phase 7: core components

- AGENTS adaptation
- skills
- subagents
- Codex config
- MCP component
- wiki component
- readiness gate
- Open in Codex adapter

## Phase 8: Git

- initialize or preserve
- ignore merge
- branch
- optional initial commit
- remote review without push
- separately approved public GitHub repository creation and push

## Phase 9: optional workflows

- Meshy credential vault
- 3D component and bootstrap adapter
- provider-neutral `workflow.super_events` component and generic
  `.agents/skills/hoi4-super-events/` tree from the verified manifest
- exact Optional workflows title order: 3D models workflow, Super Events workflow, then ComfyUI portrait production
- selected-provider ComfyUI portrait production with Cloud, Local, RunPod, and Disabled generic states
- Super Events lock/scan state and no-guidance behavior when unselected
- optional readiness
- Update and Repair optional-workflow actions, including same-locked-revision
  Repair for Super Events

## Phase 10: maintenance

- update
- repair
- reinstall
- rollback history
- removal
- lock viewer
- modification detection

## Phase 11: hardening and release

- accessibility
- fault injection
- security review
- code signing
- notarization
- performance
- migrations
- sanitized support bundle

## Must-have backlog

- live remote manifest
- exact commit resolution
- read-only existing scan
- internal and external descriptors
- generated replaceable thumbnail placeholder
- launcher discoverability and empty-mod load readiness
- deterministic scanner with required ChatGPT-backed Codex semantic review
- AGENTS adaptation
- skills and subagents
- offline wiki and coverage
- dry run
- no silent overwrite
- transaction and rollback
- lock
- readiness and Open in Codex
- optional 3D incomplete state
- optional Super Events selection, non-blocking readiness, and later maintenance

## Should-have backlog

- comment-preserving TOML merge
- offline cache repair
- richer identifier parser
- initial commit preview
- per-component update history
- sanitized support bundle
- localization-ready UI strings

## Could-have backlog

- signed manifest
- enterprise mirror
- custom workflow repositories
- release bundles
- optional-workflow plugin SDK
- repository topology viewer
- external diff editor integration

## Deferred

- Steam Workshop publishing
- cloud project sync
- whole-drive mod discovery
- automatic HOI4 launch validation

## Technical spikes

1. Comment-preserving TOML library.
2. Open in Codex integration on both platforms.
3. Multi-file apply in cloud-synced folders.
4. MCP initialize health wrapper.
5. Manifest signing and key rotation.
6. Repository-maintained macOS routes for current Windows-only components.
7. Codex App Server startup, ChatGPT-managed authentication, stdio JSONL lifecycle, and schema-constrained project analysis.
8. Clean-machine HOI4 launcher discovery and duplicate-descriptor behavior on Windows and macOS.

## Open-source repository bootstrap

Complete before broad implementation begins:

- create public GitHub repository after license decision
- add user-facing README and community health files
- add AGENTS, living skills, and bounded subagents
- add planning and template validation
- configure main branch ruleset
- configure Dependabot and private vulnerability reporting
- establish stable CI check names
- create protected release environments
- implement repository-owned build, test, packaging, and release scripts

**Exit:** a contributor can clone the repository, read one clear setup document, run the planning validation, and open a pull request through the protected workflow without needing private instructions.
