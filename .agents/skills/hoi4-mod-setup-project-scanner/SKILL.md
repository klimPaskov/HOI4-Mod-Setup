---
name: hoi4-mod-setup-project-scanner
description: Use for existing HOI4 project scanning, descriptor discovery, identifier and convention detection, evidence and confidence models, finding review, or scan performance changes.
---

# Existing project scanner

## Purpose

The scanner reads one user-selected mod project and returns evidence-backed findings without modifying the project. A required ChatGPT-authenticated Codex pass interprets approved evidence after the deterministic scan. It is a separate read-only layer.

## Required sources

Read:

- `AGENTS.md`
- `docs/02_user_flows.md`
- `docs/03_scanner_design.md`
- `docs/schemas/scan-result.schema.json`
- `docs/examples/scan-result.existing.example.json`
- `docs/20_testing_strategy.md`
- `docs/30_codex_chatgpt_authentication.md`
- `docs/schemas/codex-analysis.schema.json`

## Read-only boundary

During scan, do not:

- create project temp files
- initialize Git
- acquire a project mutation lock
- write recent-project metadata inside the project
- repair malformed files
- normalize line endings
- follow unapproved links outside the root
- execute project scripts

Use memory or an application-owned cache outside the project for transient scan state.

## Scan surfaces

Detect only with evidence:

- mod root and descriptor relationships
- launcher descriptor
- folder structure
- Git root, branch, remotes, ignore rules, and dirty state
- namespaces and event IDs
- country tags and other identifiers
- naming patterns
- localisation files, language conventions, encoding, and key prefixes
- documentation
- `.agents/skills`
- `.codex/agents`
- Codex config
- MCP config
- the fixed `.hoi4-mod-setup/install.lock.json` managed-installation marker
- absolute paths and machine-local assumptions
- possible install conflicts

## Launcher and standard-path boundary

- `src-tauri/src/paths.rs::hoi4_user_mod_directory` resolves the Windows known
  Documents folder or macOS Foundation user Documents search path, then requires the existing
  `Paradox Interactive/Hearts of Iron IV/mod` directory without symlink or
  junction components. It does not search for alternate mod folders.
- `src-tauri/src/commands.rs::suggest_project_paths` derives the new-project
  leaf and external `<project_id>.mod` path from that directory, reports both
  collision bits, and refuses link-containing destinations. The UI may fall
  back to one explicit folder choice when the standard directory is missing.
- `discover_launcher_descriptor` canonicalizes the selected project, inspects
  only its immediate parent, ignores links, malformed or oversized `.mod`
  files, and accepts only descriptors whose absolute `path` resolves to the
  selected root. Prefer the exact `<project-leaf>.mod` filename; multiple
  non-exact matches remain ambiguous rather than being guessed.
- Existing-project folder selection returns the canonical project path plus
  any discovered launcher path. A scan never searches sibling drives or
  unrelated folders.

## Finding model

Every finding needs:

- stable ID
- category
- status
- confidence
- evidence
- inferred or detected value
- proposed action
- blocking class
- editable review state

Use `confirmed`, `probable`, `ambiguous`, `missing`, and `conflicting` or the schema-approved equivalents. Low confidence is not an error. It is a review requirement.

## Detection rules

- Prefer exact parser evidence over filename heuristics.
- For Git, use bounded read-only commands with `--no-optional-locks` and
  record branch, detached state, commit, dirty/staged/unstaged/untracked
  counts, remote names, submodule paths, hook names, ignore files, and
  tracked secret-like path names. Never follow a linked worktree's external
  gitdir; report partial probe evidence as a review state.
- Prefer repeated patterns over one isolated match.
- Do not infer a project-wide namespace from generated, vendored, or test files.
- Separate existing project conventions from proposed installation values.
- Keep secret-like content redacted while preserving evidence location.
- Inspect a prior managed setup only through the fixed lock path. Skip the
  metadata tree during the normal walk, and emit a bounded summary containing
  project ID, component IDs, `workflow.3d` state, `workflow.super_events`
  state, and the non-secret 3D key-configured bit. This lock summary is the
  remembered installed state used to repopulate scan/maintenance choices;
  readiness later re-reads the lock rather than trusting transient UI state.
  Ignore the retired portrait-interest field in a legacy lock; it is not an
  installed feature or current scan finding. A missing lock is
  absent/non-blocking; link, size, read, parse, or schema failures are blocking
  and must not be guessed into an installed state.
- Bound file size, depth, count, and parse work.
- Skip app-managed tooling, temporary output, and offline-wiki trees
  (`.tools/`, `.tmp/`, and `paradox_wiki/`) during the recursive walk. Detect
  the exact `paradox_wiki/` root entry separately for component inventory and
  report links without following them.
- Support cancellation and partial result reporting.
- Stream progress through an opaque request ID with stage, relative path, file count, directory count, and bytes read. Do not emit or render a percentage when a total is unknown.
- Cancellation is cooperative, returns explicit `partial` and `cancelled` metadata, emits a terminal cancellation event, and clears the approved-evidence binding before any Codex analysis can use it. Any non-cancelled partial result also clears that binding and blocks semantic analysis until a complete scan is available.

## Review grouping

Present findings in small groups:

1. project identity and descriptors
2. structure and paths
3. IDs and naming
4. localisation and docs
5. Codex, skills, subagents, and MCP
6. Git
7. conflicts and unresolved findings

Do not show a giant scan report as the default screen.

## Codex boundary

Use `codex app-server` over stdio JSONL after deterministic scan completion. Require managed ChatGPT auth, an approved input manifest, read-only sandboxing, strict `outputSchema`, deterministic proposal validation, and user confirmation. Missing authentication or valid output blocks new planning while preserving scan evidence.

## Required tests

- project remains byte-identical after scan
- malformed descriptors
- nested Git root
- symlink and junction escape
- case collisions
- unreadable files
- huge files and deep trees
- mixed encodings
- conflicting namespaces
- Git status probe failure, linked worktree metadata, remotes, submodules,
  hooks, ignore files, and tracked secret-like paths
- pre-existing managed files
- valid managed-lock recognition without walking metadata
- remembered optional-workflow state from a valid lock, including selected and
  unselected `workflow.super_events`
- malformed, linked, oversized, and unreadable managed-lock states
- cancellation and timeout
- progress event correlation, indeterminate progress, and cancelled-evidence invalidation
- confidence downgrade when evidence conflicts
- standard HOI4 mod-directory resolution, collision reporting, malformed/link
  launcher candidates, and ambiguous sibling registrations
- browser and device-code sign-in
- usage limit, process interruption, and malformed schema
- no account data or raw project text in persisted scan metadata

## Update this skill when

Update this skill when scan categories, evidence, confidence, grouping, boundaries, parser behavior, performance limits, or read-only guarantees change.
