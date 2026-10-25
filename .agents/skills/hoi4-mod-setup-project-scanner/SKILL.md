---
name: hoi4-mod-setup-project-scanner
description: Use for targeted agentic HOI4 setup scanning, descriptor discovery, evidence and confidence models, finding review, or scan performance changes.
---

# Existing project scanner

## Purpose

The scanner reads one user-selected mod project and returns evidence-backed findings without modifying the project. A required pass by the selected setup assistant interprets approved evidence after the deterministic scan. It is a separate read-only layer; Codex uses ChatGPT authentication and App Server, while other providers use their configured adapter.

## Required sources

Read:

- `AGENTS.md`
- `docs/02_user_flows.md`
- `docs/03_scanner_design.md`
- `docs/35_project_profiling.md`
- `docs/schemas/scan-result.schema.json`
- `docs/examples/scan-result.existing.example.json`
- `docs/20_testing_strategy.md`
- `docs/30_codex_chatgpt_authentication.md`
- `docs/31_ai_provider_profiles_and_chat_sources.md`
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
- Git root, branch, remotes, ignore rules, and dirty state
- documentation
- bounded project profiling for gameplay structure, identifiers, localisation,
  and documentation facts
- canonical `.claude/skills` and legacy `.agents/skills`
- canonical `.claude/agents` and generated `.codex/agents`
- Codex config
- MCP config
- the fixed `.hoi4-mod-setup/install.lock.json` managed-installation marker
- absolute paths and machine-local assumptions
- possible install conflicts

The setup inventory and the profile share one walk (`ProjectWalk` in
`src-tauri/src/scanner.rs`). Each directory is enumerated once through the
retained no-follow handle and offered to both. The targeted setup inventory
(`TargetedNames`) and the profile (`project_profile::ProfileNames`, `admit`,
`record_entry`) each keep their own exclusions, budgets, and stop state. A
directory is entered when either one wants it. Keep setup-only checks (identity
fences, `path_has_link_component`, the directory budget) on setup
directories, so profile-only folders cost what the profile walk always cost.
When one inventory reaches a limit, the other continues; cancellation, timeout,
and a changed boundary stop both. Never add a second tree walk. Test the profile
alone through `project_profile::scan`, which runs this walk with the setup
inventory off.

Detect native coding environments only inside the selected root, from the
markers in the `NATIVE_CLIENTS` registry (`crates/hoi4-setup-core/src/coding_environment.rs`).
Claude Code (`CLAUDE.md`, `.claude/`, and `.mcp.json`) is the canonical package;
Codex is detected only by `.codex/config.toml`, because generated TOML agents
are installed for every selection. The registry's `scan_directories` are the
only extra folders the scanner enumerates, one level each. Files there and
exact client markers use a presence-only content policy: they are observed
without reading their bytes, because client folders such as `.continue/` or
`.aider.conf.yml` can hold credentials. Only the registry's small
`config_files` are read, retained, and parsed with JSONC tolerance; an
unparseable file is a blocking `conflict.<client>.config.parse` only for the
original five clients and a warning for any other client. Registry client
paths pass the ignore list (`.vscode/mcp.json`), but `.github` and `.vscode`
files count only as exact markers or config files. A linked presence-only
marker is a warning, not a blocking link conflict, because it is never read.
Project-profile limits (`scan.profile.*`) never make the setup scan partial. Prefer the managed lock's recorded primary/additional selection; for
fresh installs the manifest default is Claude Code, while a legacy project
without a recorded primary uses Codex as the compatibility migration default.
This finding is bounded setup evidence, not a reason to widen the setup scan or
its Git pathspecs. The separate project profile in
`docs/35_project_profiling.md` may collect bounded metadata and approved text
prefixes from eligible gameplay, localisation, and documentation paths, while
keeping binary/media bytes and unapproved content out of semantic input.

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
- Keep the actual external launcher path in the finding value for review.
  Use `@approved-launcher/descriptor.mod` as its semantic evidence reference
  and for its validation conflict. This reserved summary identifier is never
  opened as a file; generic absolute-path rejection and exact core scan/hash
  approval remain unchanged. Test a real scan through evidence approval, not
  just manually constructed relative-path evidence.

Aggregate scan evidence uses reserved `@scan/git-summary`,
`@scan/coding-environments`, and `@scan/absolute-paths` identifiers instead of
`.` or raw `.git` paths. These identifiers and `@approved-launcher/descriptor.mod`
are summaries, never filesystem inputs. They remain bound to the core scan ID,
canonical project root, finding/conflict reference, excerpt hash, and explicit
evidence approval. Do not relax the generic absolute-path or `.git` rejection.

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

Preserve origin, recommendation, and core conflicts through the typed desktop
bridge. Provider approval binds normalized redacted finding/conflict summaries
to their core hashes; inventory alone never approves raw project file text.

Use `confirmed`, `probable`, `ambiguous`, `missing`, and `conflicting` or the schema-approved equivalents. Low confidence is not an error. It is a review requirement.

## Detection rules

- Prefer exact parser evidence over filename heuristics.
- Descriptor parsing must accept real HOI4 descriptors: multi-line `{ ... }`
  blocks such as `tags={` or `dependencies={`, repeated `replace_path`, a
  UTF-8 BOM, and `#` comments outside quotes. Unclosed or oversized blocks and
  other duplicate keys stay malformed.
- Skill validity uses `skill_frontmatter_is_valid`: an opening and closing
  `---` line with exactly one non-empty top-level `name` and `description`
  (plain or quoted keys, block scalars allowed) and no duplicate keys.
- For Git, use bounded read-only commands with `--no-optional-locks` and
  record branch, detached state, commit, dirty/staged/unstaged/untracked
  counts, remote names, submodule paths, hook names, ignore files, and
  tracked secret-like path names. Never follow a linked worktree's external
  gitdir; report the current bounded Git review conflicts as visible review
  state, not as overall scan truncation. Classify those advisory IDs explicitly
  rather than excluding the `scan.git.*` namespace, so a future Git detector
  limit remains an honest partial result.
- Prefer repeated patterns over one isolated match.
- Do not infer a project-wide namespace from generated, vendored, or test files.
- Separate existing project conventions from proposed installation values.
- Keep secret-like content redacted while preserving evidence location.
- Inspect a prior managed setup only through the fixed lock path. Skip the
  metadata tree during the normal walk, and emit a bounded summary containing
  project ID, component IDs, `workflow.3d` state, `workflow.super_events`
  state, and the non-secret 3D key-configured bit. Use the schema-approved
  `installation` category for the `installation.managed` finding. This lock summary is the
  remembered installed state used to repopulate scan/maintenance choices;
  readiness later re-reads the lock rather than trusting transient UI state.
  Machine-local portrait routes (local ComfyUI root, local server, RunPod URL
  and workspace) go only into the separate `installation.portrait_routes`
  finding (`LOCAL_ONLY_PORTRAIT_ROUTES_FINDING`). The renderer excludes it from
  setup-assistant input and `approve_scan_evidence` refuses it.
  Ignore the retired portrait-interest field in a legacy lock; it is not an
  installed feature or current scan finding. A missing lock is
  absent/non-blocking; link, size, read, parse, or schema failures are blocking
  and must not be guessed into an installed state.
- The default targeted setup inventory covers 150,000 setup-relevant files,
  200,000 directories, depth 64, and ten minutes. It prunes ordinary gameplay,
  localisation, binary/media, root data dumps, and generated `docs/assets/`
  or `docs/formables/` corpora before setup file inventory; those intentional
  setup exclusions do not make that pass partial. Classify every directory
  entry before charging the targeted entry-count, entry-name, path, file, or
  directory budgets. Traverse skills only through
  `.claude/skills/<skill>/SKILL.md` or the legacy
  `.agents/skills/<skill>/SKILL.md`, canonical and native agents only through
  their direct files, and approved documentation only through
  `docs/<section>/<file>`. Nested references, archives, and assets are not
  setup-scan inputs. Read only bounded detector inputs:
  project and launcher descriptors, thumbnail, root CLAUDE/AGENTS/README and
  `.gitignore`, approved docs, skills, canonical Markdown subagents, generated
  Codex TOML projections, and the separately
  bounded managed lock. Detector text is capped at 16 MiB per
  file and 256 MiB per scan; retained parser evidence is capped at 128 MiB.
-  Run the project profile as a separate contained metadata walk. It has
  independent limits of 500,000 files, 200,000 directories, depth 64, 50,000
  relevant entries and 8 MiB of relevant entry names per directory, 96 content
  categories, 256 samples, 32 KiB per sampled prefix, and 8 MiB sampled bytes.
  Use deterministic path-hash sampling with at most four candidates per
  category and fair round-robin scheduling. Skip hidden, cache, tooling, wiki,
  and secret-like paths; use metadata only for binaries/media; never execute
  scripts, follow links, inspect Git, or invoke content filters. Report
  `profile_stats` separately from setup counters and label coverage `sampled`
  or `truncated`; a profile limit or read/link error blocks semantic planning
  until a complete scan is available.
  Emit `profile.coverage`, `profile.structure`, `profile.identifiers`,
  `profile.localisation`, and `profile.documentation` findings with observed
  counts, sampled namespaces/identifier prefixes, actual localisation headers,
  BOM and quoted-key style, and redacted short documentation overviews. Never
  infer a project-wide convention or exhaustive gameplay validity.
- Bind project-tree and launcher-parent enumeration, reads, and read-only Git
  probes to retained, no-follow, identity-checked directory handles. Unix name
  enumeration must use a duplicated descriptor-backed directory stream so an
  ABA swap-back cannot redirect it. Retain a separate identity-checked `.git` handle
  while invoking Git. Unix Git children enter that retained directory with `fchdir` before
  exec; Windows holds the non-delete-sharing handle while using its canonical
  path. Reject linked `HEAD`, referenced refs, config, index, refs, objects,
  info descendants, object-pack metadata, object alternates, or other critical
  child metadata. Recheck the policy immediately before and after every child
  and discard output when it changes. Use fixed
  `ls-files`/cached-index dirty probes restricted to exact
  literal scanner-observed Agentic setup paths, recover deleted setup paths
  through bounded case-insensitive index-only probes, and batch path arguments below platform
  command-line limits; never walk gameplay files merely to populate
  the import review. Do not invoke attribute-selected content filters, and reject repository configuration that
  changes before or after a child. Treat a capped child output or targeted path
  inventory as `scan.git.limit` and an honest partial result. Check cancellation
  before batches and while a reviewed child is running. Bound relative paths to 4 KiB, individual segments to 255 bytes,
  aggregate retained inventory paths to 64 MiB, conflicts to 4,096 entries,
  and each targeted directory sort to 50,000 relevant entries / 8 MiB of
  relevant entry names. Bound
  malformed agentic samples to 512, Git-ignore samples to 1,024, and launcher
  discovery to 10,000 parent entries / 512 descriptor
  candidates.
- Treat launcher discovery as a separate bounded gate: selecting the project
  root permits parsing direct-parent `.mod` candidates only to compare their
  declared `path=` text with that root. Never open a candidate-declared target.
  Continuing confirms the displayed match; **Scan without launcher file**
  excludes it from the scan and semantic evidence. At scan invocation, bind
  any renderer-supplied path back to the current unique core-discovered
  candidate and same retained parent identity, then parse the captured
  no-follow bytes instead of reopening the path; a forged or race-replaced
  absolute path is never approval.
- Feed the absolute-path accumulator while reading approved text, then discard
  bytes that no later parser needs. Keep retained parser evidence bounded and
  open detector files with the shared no-follow contained reader. A file,
  directory, depth, time, read, detector-byte, or retained-evidence limit is an
  honest partial result; never label skipped required evidence complete.
- Skip app-managed tooling, temporary output, and offline-wiki trees
  (`.tools/`, `.tmp/`, and `paradox_wiki/`) during the recursive walk. Detect
  the exact `paradox_wiki/` root entry separately for component inventory and
  report links without following them.
- Retain every file a detector parses. `scan_content_policy` must set `retain` for a path class before a detector validates its bytes; a class that is observed but not retained validates as empty and reports every installed file as malformed (native client subagent Markdown did, giving a freshly installed project 19 blocking subagent conflicts). Add a scan test for each new detector with real installed bytes.
- Leave files an earlier setup installed, still matching the lock byte for byte, out of the project profile (`unmodified_managed_visible_files` in the scanner feeds `Profiler::skip_managed_files`). A lock that cannot be read means no exclusion, never a failed scan.
- Support cancellation and partial result reporting.
- Stream progress through an opaque request ID with stage, relative path, file count, directory count, and bytes read. Do not emit or render a percentage when a total is unknown.
- Cancellation is cooperative, returns explicit `partial` and `cancelled` metadata, emits a terminal cancellation event, and clears the approved-evidence binding before any Codex analysis can use it. Any non-cancelled partial result also clears that binding and blocks semantic analysis until a complete scan is available.
- Hash the exact UI-approved evidence representation: raw UTF-8 for string
  values and compact JSON for all other values. Require finding confidence,
  blocking state, proposed action, and decision state in the current result
  schema. Normalize Unicode case for collision detection, share its 4,096-entry
  conflict budget, reject Unix literal-backslash candidates, accept LF/CRLF
  skill frontmatter, and detect both Windows drive separators plus UNC paths.

## Review grouping

Present findings in small groups:

1. project identity and descriptors
2. project profile
3. instructions and paths
4. Codex, skills, subagents, and MCP
5. Git
6. conflicts and unresolved findings

Do not show a giant scan report as the default screen.

## Setup-assistant boundary

After deterministic scan completion, send only the approved evidence through the selected setup-assistant adapter. Core-owned `model_excerpt` strings are the canonical normalized/redacted representation used for evidence hashes; UI-only values such as absolute launcher paths must not leak into semantic input or change the hash through reordering. Codex uses `codex app-server` over stdio JSONL with managed ChatGPT authentication. Every provider requires an approved input manifest, read-only analysis, the shared strict output schema, deterministic proposal validation, and user confirmation. Missing provider configuration, authentication where required, or valid output blocks new planning while preserving scan evidence. The setup-assistant choice never selects or configures the AI tools installed for later mod development.

## Required tests

- project remains byte-identical after scan
- malformed descriptors
- nested Git root
- symlink and junction escape
- case collisions
- unreadable files
- huge files and deep trees
- Git status probe failure, linked worktree metadata, remotes, submodules,
  hooks, ignore files, and tracked secret-like paths
- pre-existing managed files
- valid managed-lock recognition without walking metadata
- remembered optional-workflow state from a valid lock, including selected and
  unselected `workflow.super_events`
- malformed, linked, oversized, and unreadable managed-lock states
- cancellation and timeout
- shared walk: the profile from a full scan equals a profile-only walk; a
  setup file or depth limit leaves the profile complete; cancellation in the
  middle of the walk stops both and yields no profile; a link in a directory both
  inventories read is refused by both
- proof that large gameplay/media/root-data/generated-doc trees are excluded
  from the targeted setup inventory and setup-only Git pathspecs, while a
  separate profile records bounded metadata and text-prefix observations;
  include targeted detector-text, profile-prefix, and retained-memory budgets
  and a real approved very-large mod fixture whose scan leaves app-owned
  metadata unchanged
- synthetic profile tests for read-only/cancellation, links, large files,
  depth and per-directory limits, deterministic fair sampling, localisation
  headers/BOM/quoted-key style, documentation redaction, model-excerpt hash
  stability, schema validation, and token-shaped value absence
- progress event correlation, indeterminate progress, and cancelled-evidence invalidation
- confidence downgrade when evidence conflicts
- standard HOI4 mod-directory resolution, collision reporting, malformed/link
  launcher candidates, and ambiguous sibling registrations
- browser and device-code sign-in
- usage limit, process interruption, and malformed schema
- no account data or raw project text in persisted scan metadata

## Update this skill when

Update this skill when scan or profile categories, evidence, confidence,
grouping, boundaries, parser behavior, performance limits, provider-input
projection, or read-only guarantees change.
