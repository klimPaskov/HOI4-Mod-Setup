# Project scanner and required provider analysis

## Purpose and authority

The scanner creates an evidence-backed project profile without changing the
selected project. It receives one explicit root. Companion paths outside the
root are allowed only when the user approves them, such as a launcher
descriptor or vanilla game directory.

The scanner is deterministic Rust code and is the sole authority for
observable setup facts: descriptor validity, approved paths, hashes, Git
state, project instructions, agentic configuration, and conflicts. A
read-only scan creates no project cache, Git lock, transaction folder, package
installation, launcher change, or other project write.

## Bounded launcher descriptor discovery

For an existing selected root, discovery is a separate pre-scan gate. It may
enumerate only direct `*.mod` files in the root's immediate parent, with the
same bounded, link-safe reads used by the scanner. It parses candidates only to
compare their normalized `path=` value with the selected root and to expose
the evidence needed for review. It never recurses into siblings, searches
Documents or other drives, or chooses a descriptor implicitly. Multiple
matching registrations and a canonical `<project>.mod` file that declares a
different root are explicit discovery conflicts, never silently selected. A
candidate count beyond the 512-file bound is also an explicit review state,
not a truncated successful discovery.

The UI visibly presents the candidate path and match before the scan starts.
The user confirms by continuing, chooses **Scan without launcher file**, or
cancels by going back. Selecting the root authorizes only this bounded parse of
direct-parent candidates; declared target paths are compared as normalized
text and are never opened. Only a confirmed candidate is added to the approved
companion paths and scanner input. A declined or absent candidate produces an
internal-only scan and does not grant permission to read another path.

## Two-layer analysis contract

The Rust scanner produces observable facts. The selected provider adapter
produces semantic proposals from approved normalized evidence summaries.

The deterministic layer owns the targeted setup inventory, descriptors,
launcher registration, thumbnail decoding, Git state, component inventory,
agentic configuration, conflicts, and platform support.

The provider layer owns project purpose, normalized description, display name
and ID proposals, prefix and namespace proposals, folder profile, `AGENTS.md`
adaptation direction, component recommendations, convention interpretation,
and concise conflict explanation.

Codex uses ChatGPT-managed authentication through the official local App
Server. Other hosted providers use an explicit endpoint and OS-vault key;
local models use loopback HTTP. Every provider uses a user-reviewed input
manifest, a read-only turn, and `schemas/codex-analysis.schema.json`. No
provider has write access.

## Deterministic scan phases

1. **Filesystem boundary:** canonical root, case behavior, links, large
   directories, descriptors, and the approved external destination. Never
   follow a link outside the root or read an unconfirmed companion path.
2. **Descriptors and thumbnail:** internal and launcher descriptor fields,
   duplicate keys, quoting, supported versions, path agreement, thumbnail
   existence, decoding, dimensions, color mode, hash, and replacement state.
3. **Targeted setup inventory:** root `AGENTS.md` and `README.md`, `.agents/skills`,
   `.codex/agents`, `.codex/config.toml`, approved documentation, descriptors,
   thumbnail, Git metadata, and the managed setup lock. Ordinary HOI4 gameplay,
   localisation, media, content dumps, and generated documentation corpora are
   outside this setup scan and are neither opened nor counted. Directory entries
   are classified before targeted file, path, and directory-sort budgets are
   charged, so a large gameplay directory cannot hide or truncate relevant setup
   evidence.
4. **Git:** repository root, branch, detached state, commit, dirty buckets,
   remotes, submodules, hooks, ignore files, linked worktrees, and tracked
   secret-like paths using bounded read-only commands.
5. **Documentation and instructions:** bounded AGENTS, README, and approved-doc
   inventory plus machine-local absolute-path locations; the scan does not
   turn arbitrary project prose into a deterministic fact.
6. **Skills and subagents:** skill count and frontmatter validity, subagent TOML
   count and parse validity, and an exact top-level `fork_context=false` gate.
   Helpers, assets, and model choices are reviewed later through selected
   component and merge previews rather than executed or inferred by the scan.
   Native Codex, Claude Code, Cursor, Qoder, and OpenCode files are detected
   inside the selected root. A validated managed lock supplies the recorded
   primary and additional environment selection; loose files are merged into
   the detected set, and Codex is the migration default only when no valid
   recorded selection exists.
7. **Codex and MCP:** structural TOML validity, configuration presence, and MCP
   server IDs. Commands, arguments, cwd, environment names, timeouts, sandbox,
   approval policy, and feature flags remain visible in the later structured
   merge preview; the scanner never executes or semantically interprets them.
8. **Conflict synthesis:** path, platform, dependency, ownership,
    generated-destination, launcher, thumbnail, and Git risks against the
    selected remote manifest.
9. **Managed setup state:** inspect only the fixed
    `.hoi4-mod-setup/install.lock.json` path. A valid lock is reported as
    `installation.managed` with a safe component and optional-workflow summary,
    including the remembered `workflow.super_events` and
    `workflow.portraits` provider states and coding-environment selection. The
    summary does not expose lock
    contents or credential references as scan evidence; portrait provider
    state is non-secret and credentials remain outside the project. A
    missing lock is a non-blocking absent state. A linked, unreadable,
    oversized, malformed, or schema-invalid lock is a blocking finding rather
    than a reason to guess that the project is unconfigured.

## Required provider semantic review

The selected-provider pass runs only after a completed deterministic scan or a
new-project brief. It is a separate analysis layer, never a scanner phase.
Before invocation, show the request manifest and allow removal or cancellation.
Allowed inputs include normalized scan JSON, the mod description, bounded
redacted finding summaries and hashes, and incoming component metadata. Raw
project file excerpts are not sent merely because a file was inventoried; a
future excerpt requires a separate user-visible approval and exact core hash.

Exclude binaries, secrets, credential stores, `.git/objects`, files outside
approved roots, ignored secret files, and deselected content. The result must
validate against the strict output schema and include the complete required
semantic proposal set. Rust rejects malformed, incomplete, credential-shaped,
account-shaped, or extra fields before any proposal is shown as confirmed.

The selected provider may suggest project purpose, namespace and prefix,
component and skill selection, AGENTS adaptation, naming and localisation
conventions, an initial folder profile, project-specific paths requiring
review, and human-judgment conflicts. No provider can override deterministic
facts, create operations, write files, select conflict resolutions, or mark
readiness checks passed.

Result labels are:

- `detected`: deterministic and evidence-backed;
- `provider_suggested`: semantic inference awaiting review; and
- `confirmed`: accepted or edited by the user.

Store only non-secret audit metadata, approved input field or path names,
selected provider/model/profile, response hash, validated suggestions,
timestamps, and user decisions. Do not request, expose, or store hidden
chain-of-thought, account identity, tokens, raw thread history, or unapproved
project text.

Missing provider configuration, signed-out Codex, usage-limited, cancelled,
failed, or malformed-response states block a new installation plan while
preserving the deterministic scan and local draft. Recovery, rollback, backup
inspection, and managed removal remain available.

## Evidence and confidence

Evidence summaries are hashed. Large result sets store bounded counts and
samples. Confidence is visible: 1.00 for explicit user selection or an
exact descriptor field; 0.90-0.99 for a strong repeated deterministic pattern;
0.70-0.89 for a plausible pattern requiring review; and below 0.70 for a
provider suggestion only. Provider confidence never increases deterministic
confidence.

## Incremental and large-repository behavior

Cache metadata and hashes in application data only. Recompute every touched
local hash before installation; cache is advisory. Use bounded concurrency and
exclude known tooling, build, and cache trees such as `.git`, `.hoi4-mod-setup`,
`.venv`, `venv`, `env`, `__pycache__`, `.pytest_cache`, `.mypy_cache`,
   `.ruff_cache`, `.tox`, `.nox`, `.idea`, `.vscode`, `.vs`, `.cache`, `cache`,
   `coverage`, `htmlcov`, `node_modules`, `target`, `dist`, `build`, `out`,
   `.tools`, `.tmp`, and `paradox_wiki`.
   The managed offline wiki is detected from its exact root entry but its pages
   are not part of the project scan.
Ignore common editor and generated artifact files such as `.DS_Store`,
`Thumbs.db`, `desktop.ini`, `*.pyc`, `*.pyo`, `*.log`, `*.tmp`, `*.bak`, and
`*.swp`. Ordinary HOI4 content is pruned by the explicit targeted-inventory
policy, not by filename guessing. Stream
phase/path/counters, support backpressure, keep temporary evidence outside the
project, and cancel promptly. Events carry an opaque request ID and the UI
ignores another request's events. Never invent a percentage when no total is
known.

The targeted inventory covers up to 150,000 setup-relevant files, 200,000
directories, and 64 levels with a ten-minute deadline. It prunes ordinary
gameplay, localisation, binary/media, root data dumps, and generated
`docs/assets/` and `docs/formables/` corpora before file inventory. Read at most
16 MiB from a detector-relevant text file and 256 MiB across the scan; launcher
and project descriptors remain capped at 256 KiB, thumbnails at 16 MiB, the
managed lock at 2 MiB, and retained parser evidence at 128 MiB. Feed a bounded
absolute-path accumulator as each approved text file is read, then discard
content that no later parser needs. File reads do not follow links, traversal
fences the stable filesystem identity of the selected root and each directory,
retained no-follow directory handles bind both project-tree and launcher-parent
enumeration, and a retained root handle binds bounded reads and read-only Git
probes to that identity. Unix enumeration reads names from a duplicated
descriptor-backed directory stream, including across rename/symlink swap-back
races; Windows retains its non-delete-sharing identity fence. Credential-shaped names
produce one non-identifying aggregate warning, and raw iterator failures
produce at most one fixed-detail warning per directory. Relative paths are
capped at 4 KiB, segments at 255 bytes, conflicts at 4,096 entries, aggregate
retained inventory paths at 64 MiB, and each targeted directory sort at 50,000
relevant entries / 8 MiB of relevant entry names. Case-collision keys use
Unicode lowercase normalization and share the same conflict budget. Unix
rejects backslash-bearing relative scan candidates instead of letting a later
shared reader reinterpret the literal name as a separator. Skill traversal
stops after `.agents/skills/<skill>/SKILL.md`, canonical
subagent and client-agent traversal stops after the direct agent file, and
approved documentation traversal stops after `docs/<section>/<file>`; nested
references, assets, archives, and generated corpora are not scan inputs.
Malformed agentic
samples are capped at 512, structure and Git-ignore samples at 1,024 each, and
launcher discovery at 10,000 parent entries / 512 descriptor candidates.
An approved launcher is rebound to the same retained parent identity and read
once through that handle; detector parsing uses the captured bytes, so a later
parent-path replacement cannot redirect the approved evidence.
The retained root and `.git` directory handles also bind every Git probe to the
approved directories. Unix children enter the retained `.git` handle with
`fchdir` before exec; Windows retains the non-delete-sharing handle while using
its canonical path. Dirty-state probes use fixed `ls-files` and cached-index
operations restricted to exact, literal, scanner-observed Agentic setup paths,
with fixed index-only probes recovering deleted managed paths. Exact paths are
batched under command-line size bounds and deletion roots are case-insensitive,
so wildcard-shaped names cannot expand, case aliases are not missed,
and ordinary gameplay files never enter the import-time Git worktree probe. The
operations do not invoke attribute-selected content filters; repository
configuration is checked immediately before and after each child. Git child
stdout/stderr reports whether its byte cap was exceeded; truncated output makes
the bounded probe partial instead of being parsed as complete. Cancellation is
checked before every batch and while each child is running, terminating the
reviewed process tree promptly. Before any Git child starts, critical metadata
such as `HEAD`, the referenced ref, config, index, refs, objects, and info must
be link-free; linked descendants and object-alternate declarations are rejected.
The complete policy is rechecked immediately before and after every child, and
output is discarded if the metadata route changes.
The `files_scanned` and `directories_scanned` counters describe only this
targeted setup inventory. Intentional out-of-scope content remains complete by
definition. An unreadable,
oversized, timed-out, linked, identity-changed, or count-truncated detector
surface is partial.

A bounded Git status probe that cannot collect complete repository metadata is
reported as a visible `needs_review` finding and conflict. It does not set
`partial` or appear in `limits_hit` unless a separate detector surface was
actually truncated. Git review evidence must not make an otherwise complete
targeted Agentic setup scan fail. Only the explicitly classified current Git
review conflicts receive this treatment; `scan.git.limit` records a path-count
or process-output truncation and is always an honest partial result.

Finding evidence hashes cover the exact rendered excerpt: raw UTF-8 bytes for a
string value and compact JSON for every other JSON value. This is the same byte
representation stored in the approved-evidence map for semantic analysis.
Skills accept LF or CRLF YAML frontmatter. Absolute-path evidence recognizes
Windows drive paths with either separator, UNC paths, and Unix home roots.

Cancellation returns `partial: true` and `cancelled: true`, emits a terminal
event, and clears approved evidence. Any partial result blocks semantic
analysis until an untruncated scan completes; limits remain visible.

## Required fixtures and tests

- empty new project and normal existing mod;
- local AGENTS rules, skills, subagents, Codex/MCP configuration, nested
  worktree, cloud-synced root, and link escape;
- very large gameplay/media/content-dump trees proving they are not inventoried
  or read and do not make the targeted scan partial;
- a very large real mod whose bounded Git status remains reviewable while the
  targeted Agentic setup evidence completes without a safety-limit result;
- locally modified wiki, skill collision, MCP collision, and a Windows-only
  incoming component on macOS;
- valid descriptors with missing or modified thumbnails and duplicate launcher
  registrations;
- parent-level launcher candidates that match, disagree, multiply, disappear,
  or are declined before scan; and proof that an unconfirmed candidate is not
  read;
- each provider adapter's configured, missing-key, malformed-response,
  endpoint, bounded-response, and secret-redaction cases;
- Codex sign-out, login cancellation, device-code fallback, usage limits, App
  Server interruption, and schema-valid and invalid suggestions; and
- proof that provider suggestions cannot replace deterministic findings.
- valid managed-lock recognition without walking `.hoi4-mod-setup/`
- malformed, linked, oversized, and unreadable managed-lock states.
- selected, unselected, and incomplete `workflow.super_events` lock summaries;
  the unselected case must not synthesize the skill tree or Super Events
  guidance in `AGENTS.md`.

The scan result may store analysis ID, input/output digests, proposal keys,
provider/model/profile, and confirmation state. It must not store account
email, account ID, plan, rate-limit details, tokens, raw thread history, or
hidden reasoning.
