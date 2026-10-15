# Bounded project profiling

## Status and source of truth

This document records an accepted product decision for the existing-project
import flow. On 2026-10-15 the user instruction changed the old setup-only
description: a large project such as Kaiserreich must provide evidence about
gameplay structure, localisation, and project documentation, rather than being
represented only by its descriptors and README.

The decision adds a deterministic, read-only **project profile** pass beside
the existing targeted setup scan. It does not turn the scanner into a game
validator, a content indexer, or a whole-repository Git review. The Rust
implementation is in progress; this document records the contract and planned
validation, and does not claim a local runtime, build, or desktop result.

`docs/03_scanner_design.md` remains the scanner phase and evidence overview.
This document is the detailed source of truth for the profile pass and its
separate limits and findings.

## Two read-only passes

The import scan keeps two independent evidence surfaces:

| Pass | Purpose | Counters and limits | Git behavior |
| --- | --- | --- | --- |
| Targeted setup scan | Descriptors, launcher registration, instructions, skills, subagents, client/MCP configuration, managed state, and approved Git evidence | The existing setup limits remain: up to 150,000 setup-relevant files, 200,000 directories, depth 64, the existing detector/read and retained-evidence budgets, and the existing ten-minute scan deadline | Existing fixed literal pathspecs cover only scanner-observed Agentic setup paths. They remain bounded review evidence and never become a whole-repository clean/dirty claim. |
| Project profile | Metadata and bounded samples that establish observed project structure, identifiers, localisation conventions, and documentation shape | Independent profile limits in this document; profile counters are exposed separately from setup counters | No Git commands, Git metadata reads, or content filters. |

The passes share the selected root identity, cooperative cancellation, and
read-only containment rules. A profile count or sample must not be presented as
an installation file inventory, a setup component inventory, or a complete
repository audit.

## Read-only containment

The profile walks one user-selected project root through the same contained,
no-follow filesystem boundary as the scanner. It must:

- retain and re-check the selected root identity;
- reopen a fresh root handle and verify it matches the retained root before
  enumeration, so its directory-stream offset cannot inherit an unrelated
  walk;
- refuse symlinks, junctions, reparse points, and other linked entries instead
  of following them;
- execute no project script, hook, helper, interpreter, or external command;
- make no network request and follow no link in project content;
- perform no Git operation, inspect no Git object, and invoke no
  attribute-selected content filter;
- skip hidden entries, cache and build/dependency trees, app tooling, the
  offline wiki tree, and secret-like paths; and
- read only file metadata for binary and media files. Their bytes are not
  sampled or sent for semantic analysis.

The profile excludes entries such as `.git`, `.tools`, `.tmp`,
`paradox_wiki`, `node_modules`, `target`, `dist`, `build`, `out`, `venv`,
`env`, `__pycache__`, `coverage`, and other hidden or cache/tooling trees.
The exact security/path classifier remains implementation-owned; a new
exclusion must preserve deterministic counts and visible coverage reporting.
The setup scan's separate `.claude/skills/<skill>/SKILL.md` traversal remains
bounded to the canonical skill file (with the legacy `.agents/skills` route
where applicable); project profiling must not widen that traversal.

## Profile limits

The following limits apply only to the project profile:

- 500,000 indexed files;
- 200,000 indexed directories;
- maximum traversal depth 64;
- at most 50,000 relevant entries and 8 MiB of relevant entry names per
  directory;
- at most 96 observed content categories;
- at most 256 sampled files in total;
- at most the first 32 KiB read from each sampled file; and
- at most 8 MiB of sampled bytes in one profile.

The existing setup-scan limits remain independent. In particular, increasing
the profile's file budget must not widen setup-only Git pathspecs, detector
inputs, launcher discovery, managed-lock reads, or semantic evidence.

Every limit, unreadable entry, unsafe link, identity change, cancellation, and
other profile error is reported in profile coverage. A limit or error makes
the profile coverage `truncated`; it is not silently treated as a complete
observation. The containing scan remains partial and blocks semantic planning
until a complete scan is available, using the existing scan approval and root
binding rules. Cancellation preserves the existing request/root identity
binding for cleanup and invalidates its approved evidence; a later root cannot
reuse the cancelled session's evidence.

## Deterministic sampling

The profile first collects bounded metadata: relative path, file/directory
classification, size, root directory, safe content category, and observed
filename separator style. It then chooses samples deterministically from the
normalized relative path. Selection uses a path hash and caps each category at
four samples.

The planned selector uses fair round-robin scheduling across categories so an
early large category cannot consume the entire 256-file sample budget. Samples
are prefixes only. Each sample records a safe relative path, source
size, sampled byte count, encoding classification, whether the prefix is
complete, and a SHA-256 of the sampled prefix. The contained prefix reader
rechecks path containment and the source size/modified-time identity while
returning the source size and prefix hash. A sample hash never claims the hash
of the complete source file. Paths and excerpts are redacted before they can
become model-visible evidence; a raw gameplay or localisation file is not sent
merely because it was sampled.

The profile recognizes categories needed for bounded project facts, including
gameplay roots such as `common/`, `history/`, `events/`, `decisions/`,
`interface/`, and `map/`, localisation roots, and documentation roots such as
`docs/`, `doc/`, `documentation/`, and root README files. Binary/media entries
may contribute metadata counts but do not contribute content samples. Generated
documentation asset/formables corpora are not treated as documentation prose.

## Findings and confidence

The deterministic profile emits separate findings for:

- `profile.coverage`: observed files/directories, excluded entries, sampled
  files and bytes, limits/errors, and `sampled` or `truncated` coverage;
- `profile.structure`: observed root directories, content-category counts,
  sampled encodings, and filename separator counts;
- `profile.identifiers`: sampled namespace declarations and identifier
  prefixes, explicitly marked as observations rather than a project-wide
  convention;
- `profile.localisation`: actual sampled language headers, UTF-8 BOM versus
  UTF-8-without-BOM versus non-UTF-8 classification, and versioned versus
  unversioned quoted-key style; and
- `profile.documentation`: sampled document headings and short redacted
  overviews, marked as untrusted project prose.

The optional root `profile_stats` object contains `files_indexed`,
`directories_indexed`, `samples_read`, `bytes_read`, `excluded_entries`, and
`coverage` (`sampled` or `truncated`). The existing `files_scanned`,
`directories_scanned`, and `bytes_read` scan counters retain their targeted
setup-scan meaning; profile statistics are presented separately in progress
and final results.

Counts are observed counts under the profile's exclusion and limit policy.
They are not proof that every gameplay file follows the sampled pattern.
Namespaces and identifier prefixes may be intentionally plural. The profile
never infers a single project-wide namespace, naming convention, localisation
policy, folder profile, gameplay rule, or documentation requirement.

Profile findings are deterministic evidence with visible confidence and review
state. The selected setup assistant may interpret approved profile summaries
and propose project-specific values, but those proposals remain separate,
editable, and subject to the normal schema, deterministic validation, and user
confirmation gates.

## Model-visible evidence

The core owns the optional `model_excerpt` (canonical JSON or string) used for
each finding's evidence hash. A UI may display richer values for review, such
as the real approved launcher path, but UI-only ordering or absolute paths must
not change the string sent to semantic analysis. This prevents an absolute
launcher path or other machine-local value from leaking through a profile/scan
finding and prevents JavaScript integer-key reordering from producing a hash
mismatch.

The existing evidence approval and exclusion flow remains authoritative:

- a profile finding is model-visible only after the user-approved evidence
  manifest is built by the core;
- the exact normalized excerpt string is hashed and rechecked by the core;
- no raw gameplay, localisation, binary, secret, credential-store, Git-object,
  or unapproved external-path content is included; and
- a future file excerpt requires a separate visible approval and exact core
  hash, just as for other scanner evidence.

The setup assistant still cannot write project files, approve a transaction,
resolve a conflict, or mark readiness. Deterministic Rust remains responsible
for rendering provider-neutral project bytes after confirmation.

## Validation plan

The profile contract requires the following evidence before the implementation
is considered complete:

- a native, remote Kaiserreich fixture proving that a large import produces
  useful gameplay-structure, localisation, and documentation observations
  while staying bounded;
- synthetic read-only and cancellation fixtures, including root identity
  changes, linked entries, hidden/cache/tooling/wiki/secrets exclusions, very
  large files, per-directory limits, depth limits, and partial coverage;
- deterministic sampling and fairness checks, including the four-per-category
  cap, the 256-file cap, prefix-only hashes, and the 8 MiB aggregate bound;
- localisation fixtures covering the actual language-header, BOM, and
  versioned quoted-key observations;
- documentation fixtures proving heading extraction, short-overview
  redaction, and untrusted-content labeling;
- schema validation for separate profile stats/findings and the core-owned
  model excerpt, including an evidence-hash test that cannot be changed by UI
  reordering;
- token/credential redaction and no-raw-gameplay-input tests; and
- proof that the existing setup-only Git pathspec and root-id/cancellation
  binding remain unchanged.

These are planned validation cases, not results claimed by this documentation
change. No local runtime, build, or desktop test is claimed here.
