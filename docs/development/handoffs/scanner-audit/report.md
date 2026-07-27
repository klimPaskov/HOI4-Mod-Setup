# Existing-project scanner audit

Date: 2026-07-26

Scope: `src-tauri/src/scanner.rs`, `src-tauri/src/descriptors.rs`, `docs/02_user_flows.md`, `docs/03_scanner_design.md`, `docs/20_testing_strategy.md`, `schemas/scan-result.schema.json`, the two scan-result examples, and the inline scanner/descriptor tests.

The audit applied the scope and constraints in `.codex/agents/hoi4setup_scanner_auditor.toml`, including the `fork_context=false` project-subagent rule. No production file was edited. The handoff is the only requested artifact.

## Read-only boundary result

**Qualified pass for direct mutation behavior; fail for complete containment.** The production scanner performs filesystem metadata and read operations only (`scanner.rs:42-116`, `119-215`, `324-333`), and the descriptor module only parses or renders strings (`descriptors.rs:32-76`). There are no production write, process, Git mutation, lock, or cache calls in the scoped files. The `fs::write` calls at `scanner.rs:750-760` and `779-783` are test-fixture setup only.

The result is not a safe read-only boundary overall: `detect_git` can follow a `.git` symlink or junction and read `.git/HEAD` outside the selected root (`scanner.rs:168-170`, `324-333`). `read_only: true` is set unconditionally (`scanner.rs:103-112`) and therefore is an output claim, not a proof that all reads stayed within the approved boundary.

No Cargo test command was run because building the Rust tests would create build artifacts, while this audit is permitted to write only the requested report. The scan schema and both examples were parsed successfully as JSON; that is not a full JSON-Schema validation.

## Severity-ranked findings

| ID | Severity | Finding | Exact evidence |
| --- | --- | --- | --- |
| SA-001 | High | `.git` links can escape the project root. | `scanner.rs:168-170` skips `.git` before link metadata is checked; `scanner.rs:324-333` uses `is_dir()` and `read_to_string()` on the path, both of which can follow a link. |
| SA-002 | High | Traversal has no hard total resource or time budget. | Defaults are configurable (`scanner.rs:12-31`); `collect_files` recursively reads every file up to 2 MiB and retains all bytes (`scanner.rs:127-215`). There is no total-byte cap, directory cap, deadline, or timeout. |
| SA-003 | High | Cancellation is a counter hint, not prompt cancellation or a partial-result contract. | Only `cancel_after_files` exists (`scanner.rs:17-22`); it is checked between directory entries (`scanner.rs:152-163`), cannot interrupt a read, and returns a normal completed result with an informational conflict (`scanner.rs:103-115`). The schema has no partial/cancelled/timeout fields (`scan-result.schema.json:6-70`). |
| SA-004 | High | A single unreadable directory, metadata entry, or small file aborts the entire scan with no partial result. | `read_dir`, entry, metadata, and `read` errors all propagate with `?` (`scanner.rs:137-138`, `165`, `172-173`, `201-204`; call chain `52-59`). |
| SA-005 | High | Git coverage is limited to a root `.git` presence bit and raw `HEAD` text. | `scanner.rs:324-359` does not discover an ancestor/nested Git root or parse branch/detached commit, remotes, ignore rules, dirty/staged/untracked state, submodules, sparse checkout, or worktrees. A `.git` file is only warned about. |
| SA-006 | High | Scanner descriptor parsing is a lenient name regex, not the dedicated descriptor parser, and accepts malformed input. | `scanner.rs:225-267`, `668-679` only inspect `name`; the unquoted alternative can accept a stray opening quote or trailing text. `descriptors.rs:32-50` is not used by the scanner, silently overwrites duplicate keys at line 45, and ignores malformed non-matching lines. Version, supported version, path, tags, duplicate keys, and descriptor relationships are not scanned. |
| SA-007 | High | Identifier/namespace detection is heuristic and incomplete, with substantial false-positive risk. | `scanner.rs:361-419` counts regex occurrences across all non-empty files, including docs/generated/test/vendor content; it emits one winning prefix only. Tags, event/focus/decision/effect domains, country tags, coverage, exceptions, and conflict evidence are absent. Folder and naming detection is only a top-level set (`scanner.rs:301-321`). |
| SA-008 | High | Localisation, documentation, and project-rule detection are inventory heuristics, not parsers. | Localisation only checks path text, case-sensitive extensions, BOM counts, and a loose prefix regex (`scanner.rs:422-496`); docs only counts `docs/`, root `README.md`, and root `AGENTS.md` (`scanner.rs:498-520`). Language headers, duplicate keys, line endings, encoding errors, links, source-of-truth statements, missing skill references, and foreign paths are not analyzed. |
| SA-009 | High | Skills, subagents, Codex, and MCP are filename inventories; MCP is not detected. | `scanner.rs:522-615` counts matching paths and checks one exact `.codex/config.toml` path but does not parse TOML, IDs, commands, args, cwd, environment, timeouts, sandbox, approval, fork rules, or duplicate IDs. No `mcp` finding is emitted. |
| SA-010 | High | Finding evidence does not meet the design/schema contract for review-safe provenance. | `evidence()` always sets `line_start`, `line_end`, and `excerpt_sha256` to `None` (`scanner.rs:688-697`); most findings get one synthetic path and a confidence number. `ScanConflict` has no evidence, proposal, confidence, or user decision (`scan-result.schema.json:211-249`). Findings also have no `blocking_class`; `recommendation` is optional (`scan-result.schema.json:148-209`). |
| SA-011 | Medium | Result identity and truncation conflicts are not guaranteed stable or unique. | Findings are sorted, but conflicts are not (`scanner.rs:60`, `101`). `scan.depth.{depth}` and `scan.file_limit` can repeat across recursive parents (`scanner.rs:127-150`); cancellation can do the same (`152-163`). Namespace ties use `HashMap::max_by_key` (`377-403`), so tied winners and evidence paths are not deterministic. No uniqueness test exists. |
| SA-012 | Medium | Root and approved external-descriptor link boundaries are underspecified and inconsistently checked. | The selected root is silently canonicalized (`scanner.rs:42-47`), with no root-link finding. The external path is accepted as supplied and checked with `is_file()` (`scanner.rs:269-285`); link rejection is conditional on being outside the root (`286-297`) and does not cover links inside the root, junctions, or races. |
| SA-013 | Medium | Absolute-path detection is narrow and there is no secret-like redaction layer. | Only drive paths, `/users/`, `/home/`, and `/mnt/` are matched, at most 20 files are returned, and no line/match is recorded (`scanner.rs:617-647`). Raw descriptor names and approved external paths can enter findings (`225-240`, `269-285`); the evidence helper performs no redaction (`688-697`). |
| SA-014 | Medium | Conflict synthesis and review grouping are missing or over-broad. | Existing `.agents/skills`, `.codex/agents`, and `paradox_wiki` produce fixed `managed_file_exists` warnings without incoming-manifest ownership/path/dependency/platform evidence (`scanner.rs:649-666`). Findings are a flat ID-sorted array (`101-115`); the schema has no review-group or phase field, despite the required sequence in `docs/02_user_flows.md:58-82`. |
| SA-015 | Medium | Descriptor rendering does not validate the launcher path boundary, and parser tests cover only happy paths. | `render_launcher_descriptor` accepts any `project_root` that converts to Unicode and writes it into `path=` (`descriptors.rs:66-71`); `validate_field` does not reject NUL or path semantics (`25-30`). Tests at `descriptors.rs:95-114` do not cover duplicate keys, invalid quoting/UTF-8, path escape, or descriptor mismatch. |

## Coverage table

| Surface | Result | Evidence-backed assessment |
| --- | --- | --- |
| No project mutation | Partial pass | No production writes or processes in scope; the `.git` link read is still an unapproved read boundary. |
| Root containment and approved external descriptor | Partial/unsafe | Root is canonicalized; child symlinks are recorded and not followed, but `.git`, root aliases, junctions, and external-link cases are not robustly contained. |
| Descriptor and folder discovery | Partial | Exact root `descriptor.mod` and a top-level entry set only; no launcher parse, relationship check, empty-folder tree, normalized standard surfaces, or representative files. |
| Git root/branch/remotes/ignore/dirty | Missing except presence/raw HEAD | `scanner.rs:324-359` is insufficient for the design requirement at `docs/03_scanner_design.md:23-25`. |
| IDs, namespaces, tags, naming | Partial for one regex; tags/naming absent | One frequency winner is emitted; no domain-aware index, exceptions, or conflict state. |
| Localisation | Partial | BOM mixture and loose prefixes only; no language/encoding/key parser. |
| Docs and `AGENTS.md` | Inventory only | Counts selected names; no content/link/rule comparison. |
| Skills, subagents, Codex, MCP | Inventory only; MCP missing | No TOML/frontmatter parser or command/environment/path evidence. |
| Absolute paths and secrets | Partial/no redaction | Four path patterns only; no secret-shaped content handling. |
| Finding IDs/status/confidence/evidence/proposal/blocking | Partial | IDs/status/value/evidence exist, but hashes, line ranges, blocking class, conflict evidence, and explicit review decision are missing. |
| Conflicts and small review groups | Partial/absent | A few fixed warnings; no manifest-aware conflict synthesis or group metadata/order. |
| File count/size/depth/cancellation/timeout/partial | Partial/unsafe | Default file/depth and per-file byte checks exist; overrides, total resource limits, timeout, prompt cancellation, and partial metadata do not. |
| Malformed input | Partial | Missing `name` is blocking; most malformed files are ignored, lossy-parsed, or abort the whole scan. |

## Evidence and confidence quality

The design requires hashed excerpts and parser-backed evidence (`docs/03_scanner_design.md:59-88`). The implementation's common evidence constructor explicitly leaves hashes and line ranges empty (`scanner.rs:688-697`). Several heuristic results are marked `accepted` with confidence `1.0` or `0.95`, including top-level structure, documentation inventory, and config presence (`301-321`, `498-520`, `579-595`). A bounded or truncated scan can therefore produce an apparently accepted "missing" or "clean" result without recording that the relevant files were skipped.

The schema allows only `accepted`, `needs_review`, `edited`, `rejected`, and `blocking` (`scan-result.schema.json:186-194`), while the design calls for visible ambiguous/conflicting/missing/low-confidence states. The scanner uses `needs_review` for some missing cases but never emits a finding with `blocking` status; blocking is represented separately as conflict severity. There is no schema field for a blocking class, editable decision, partial-scan reason, or redaction status.

False-positive risks include namespace matches in arbitrary text, localisation matches in unrelated folders, BOM-only "encoding" conclusions, config/skill/subagent presence inferred from filenames, and any `.git` regular file being labeled a linked worktree (`scanner.rs:348-357`). False-negative risks include files beyond 2 MiB, files after traversal limits, uppercase localisation extensions, empty directories, nested/ancestor Git repositories, UNC and POSIX absolute paths outside the four patterns, MCP files other than the exact config path, malformed TOML/YAML/Markdown, and junction escapes.

## Performance, cancellation, and malformed-input findings

- `MAX_FILES`, `MAX_DEPTH`, and `MAX_TEXT_BYTES` are constants, but `max_files` and `max_depth` are caller-controlled and are not clamped (`scanner.rs:12-31`). The default 20,000 files can retain up to roughly 40 GiB of file bytes before detector work, and a larger option can increase that bound. There is no total-byte, directory, parse-work, or memory budget.
- Traversal is synchronous recursive I/O (`scanner.rs:119-215`). No timeout or cancellation token exists. `cancel_after_files` cannot interrupt `fs::read`, `read_dir`, or a deep directory walk, and it does not expose whether results are partial (`152-163`, `103-115`).
- Errors from directory enumeration, metadata, and file reads terminate the scan rather than becoming low-confidence findings (`137-138`, `165`, `172-173`, `201-204`). This conflicts with the required unreadable-file, huge/deep-tree, cancellation/timeout, and partial-result behavior in `docs/20_testing_strategy.md:26-46`, `86-93`.
- The scan parser uses UTF-8-lossy conversion (`scanner.rs:226-227`, `361-371`, `459-461`, `624-625`). The dedicated descriptor parser is strict UTF-8 (`descriptors.rs:32-35`) but is not wired into scanning. Duplicate descriptor keys overwrite earlier values (`descriptors.rs:38-45`), and a file with one valid assignment plus malformed lines still succeeds (`47-50`).

## Exact test evidence and missing tests

The scoped Rust tests are only:

- `scanner.rs:748-774`: creates a descriptor and one event file, checks the top-level entry count, `read_only`, descriptor name, and one namespace.
- `scanner.rs:776-789`: checks that a descriptor lacking `name` produces a blocking conflict.
- `descriptors.rs:95-101`: simple render/parse round trip.
- `descriptors.rs:103-108`: three project-ID examples.
- `descriptors.rs:110-114`: launcher path string inclusion.

No scoped test covers root containment, root or child symlinks, junctions, `.git` links, approved external descriptors, nested/ancestor Git roots, branch/remotes/ignore/dirty state, case collisions, unreadable files, file/depth/size limits, timeout, prompt cancellation, partial results, deterministic IDs, duplicate conflicts, namespace disagreement, mixed encodings beyond the implemented BOM path, tags, naming, docs links, frontmatter, TOML/MCP parsing, secret redaction, absolute-path variants, or schema validation. The repository fuzz targets present in `fuzz/fuzz_targets/` cover manifest and relative-path logic, not scanner or descriptor parsing.

## Recommended parent actions

1. Make root validation and link policy explicit at the scanner entry point. Reject or visibly classify root links, detect Windows reparse-point/junction components, inspect `.git` with link-safe metadata before any read, and enforce containment for every opened path, including approved external descriptors.
2. Introduce hard, non-overridable scan budgets for files, directories, bytes, depth, parse work, and elapsed time. Use a cancellation token checked before/after every I/O operation, and return explicit `partial`, `cancelled`, `timed_out`, and `truncated` metadata with confidence downgrades.
3. Convert per-file failures into evidence-backed findings where safe; reserve a terminal scan error for root-level failures. Add deterministic traversal and conflict ordering, tie breakers, and a uniqueness check for finding/conflict IDs.
4. Wire the scanner to a strict descriptor parser that reports duplicates, invalid quoting, invalid UTF-8, unknown/malformed assignments, all required fields, and launcher/project relationships.
5. Add read-only Git discovery through a bounded adapter covering repository root, branch/detached state, commit, remotes, ignore behavior, dirty/staged/untracked state, worktrees, submodules, and nested roots.
6. Add typed parsers for localisation, Markdown/project rules, skill frontmatter, subagent TOML, Codex config, and MCP declarations. Keep filename discovery as a separate low-confidence heuristic and emit parser evidence, line ranges, and hashed excerpts.
7. Extend the scan schema/model with `blocking_class`, proposal/review decision, evidence hashes and ranges, redaction metadata, conflict evidence, review groups, and partial-scan limits/reasons. Represent ambiguous/conflicting evidence explicitly.
8. Add the missing fixture, property, fuzz, performance, cancellation, symlink/junction-race, malformed-input, redaction, deterministic-ID, and schema-validation tests required by `docs/20_testing_strategy.md`.
