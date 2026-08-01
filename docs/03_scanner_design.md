# Project scanner and required provider analysis

## Purpose and authority

The scanner creates an evidence-backed project profile without changing the
selected project. It receives one explicit root. Companion paths outside the
root are allowed only when the user approves them, such as a launcher
descriptor or vanilla game directory.

The scanner is deterministic Rust code and is the sole authority for
observable structural facts: file existence, descriptor validity, paths,
hashes, encodings, Git state, identifiers, namespaces, and conflicts. A
read-only scan creates no project cache, Git lock, transaction folder, package
installation, launcher change, or other project write.

## Bounded launcher descriptor discovery

For an existing selected root, discovery is a separate pre-scan gate. It may
enumerate only direct `*.mod` files in the root's immediate parent, with the
same bounded, link-safe reads used by the scanner. It parses candidates only to
compare their normalized `path=` value with the selected root and to expose
the evidence needed for review. It never recurses into siblings, searches
Documents or other drives, or chooses a descriptor implicitly.

The UI visibly presents the candidate path and match before the scan starts.
The user must choose **Confirm descriptor**, **Scan without external
descriptor**, or **Cancel**. Only a confirmed path is added to the approved
companion paths and scanner input. A declined or absent candidate produces an
internal-only scan and an explicit launcher-registration finding; it does not
grant the scanner permission to read another path.

## Two-layer analysis contract

The Rust scanner produces observable facts. The selected provider adapter
produces semantic proposals from approved facts and text excerpts.

The deterministic layer owns file inventory, descriptors, launcher
registration, thumbnail decoding, Git state, identifier indexes, encoding,
component inventory, conflicts, and platform support.

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
3. **Folder structure:** normalized tree summary, standard HOI4 surfaces,
   counts, and representative files. Missing optional folders are not defects.
4. **Git:** repository root, branch, detached state, commit, dirty buckets,
   remotes, submodules, hooks, ignore files, linked worktrees, and tracked
   secret-like paths using bounded read-only commands.
5. **Identifiers and namespaces:** event, focus, decision, scripted effect,
   trigger, idea, character, country, technology, localisation, sprite, and
   file-prefix indexes with frequency and confidence.
6. **Naming and localisation:** slug and prefix patterns, filenames, line
   endings, BOM, language headers, duplicate keys, encoding, and parse errors.
7. **Documentation and instructions:** AGENTS, READMEs, source-of-truth
   statements, specs, plans, manifests, absolute paths, foreign project names,
   and missing skill or agent references.
8. **Skills and subagents:** skill frontmatter, helpers, scripts, assets,
   commands, project tokens, MCP references, subagent TOML, model settings,
   sandbox, `fork_context=false`, references, duplicates, and paths.
9. **Codex and MCP:** TOML approval policy, sandbox, feature flags, server IDs,
   command, arguments, cwd, environment, timeout, duplicate IDs, and platform
   executable suffixes.
10. **Conflict synthesis:** path, namespace, platform, dependency, ownership,
    generated-destination, launcher, thumbnail, and Git risks against the
    selected remote manifest.
11. **Managed setup state:** inspect only the fixed
    `.hoi4-mod-setup/install.lock.json` path. A valid lock is reported as
    `installation.managed` with a safe component and optional-workflow summary,
    including the remembered `workflow.super_events` state. The summary does
    not expose lock contents or credential references as scan evidence; the
    credential-free Super Events entry has no credential value to expose. A
    missing lock is a non-blocking absent state. A linked, unreadable,
    oversized, malformed, or schema-invalid lock is a blocking finding rather
    than a reason to guess that the project is unconfigured.

## Required provider semantic review

The selected-provider pass runs only after a completed deterministic scan or a
new-project brief. It is a separate analysis layer, never a scanner phase.
Before invocation, show the request manifest and allow removal or cancellation.
Allowed inputs include normalized scan JSON, the mod description, selected
descriptor and README excerpts, AGENTS and skill frontmatter, TOML excerpts,
localisation headers, naming samples, and incoming component metadata.

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

Evidence excerpts are hashed. Large result sets store counts plus a separate
evidence file. Confidence is visible: 1.00 for explicit user selection or an
exact descriptor field; 0.90-0.99 for a strong repeated deterministic pattern;
0.70-0.89 for a plausible pattern requiring review; and below 0.70 for a
provider suggestion only. Provider confidence never increases deterministic
confidence.

## Incremental and large-repository behavior

Cache metadata and hashes in application data only. Recompute every touched
local hash before installation; cache is advisory. Use bounded concurrency,
exclude `.git/objects` and known caches, stream phase/path/counters, support
backpressure, keep temporary evidence outside the project, and cancel
promptly. Events carry an opaque request ID and the UI ignores another
request's events. Never invent a percentage when no total is known.

Cancellation returns `partial: true` and `cancelled: true`, emits a terminal
event, and clears approved evidence. Any partial result blocks semantic
analysis until an untruncated scan completes; limits remain visible.

## Required fixtures and tests

- empty new project and normal existing mod;
- local AGENTS rules, mixed localisation encodings, namespaces, nested
  worktree, cloud-synced root, and link escape;
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
