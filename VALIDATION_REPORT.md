# Validation report

## Result

The revision 4 planning package passed its integrity checks. The current
workspace implementation passes the frontend, schema, security, template,
formatting, all-feature Rust, Windows packaging, artifact, and native launch
gates available on this host. macOS, signing, and public-release gates remain
explicitly bounded below.

## Verified package contract

- ChatGPT sign-in is required for Create, Import, Update, and Repair planning.
- Semantic work uses the local Codex App Server and the user's ChatGPT-managed Codex access.
- The normal product has no OpenAI API key field, provider selector, or externally managed ChatGPT token path.
- Codex proposes semantic fields and returns schema-constrained output.
- Deterministic Rust validates facts, identifiers, paths, conflicts, descriptors, PNGs, plans, transactions, and readiness.
- Update planning requires a fresh bounded scan, visible approved evidence, and a core-session-confirmed Codex reanalysis record; the record is bound into the typed maintenance plan.
- Existing-project Git evidence is bounded and read-only: branch or detached state, commit, dirty buckets, remotes, submodules, hooks, ignore files, and tracked secret-like paths are surfaced without following external linked-worktree metadata.
- Existing-project scan progress is request-correlated and read-only; stage/path/counter events feed an indeterminate UI, while cancelled or safety-limited results clear Codex evidence and block semantic analysis until a complete scan is available.
- Transaction operations carry immutable ownership evidence through plan, journal, lock, and rollback; explicit skipped or external operations are rollback no-ops, and configured remotes require explicit final dry-run approval.
- First-install skips of modified managed files preserve the local hash as a visible modification and remain non-removable during later managed removal; matching preserve-mode remotes are no-ops and conflicting URLs fail closed.
- 3D health results are core-session cache entries bound to the canonical project root and locked workflow fingerprint; they carry only `ready` or `incomplete` and never persist Meshy values or process output.
- Credential-bearing external process timeouts use the shared canonical Windows process-tree termination path, with direct-child kill only as a bounded fallback.
- Open in Codex returns a typed manual-opening result when no verified opener is found, so the Ready screen keeps core readiness passed and announces the validated project path; authentication, readiness, and process failures remain errors.
- New-project output includes the internal `descriptor.mod`, the external `<project_id>.mod`, a replaceable `thumbnail.png`, and the selected initial folder scaffold.
- Launcher and thumbnail files are included in plans, locks, conflicts, rollback records, and readiness evidence.
- Signed-out recovery, rollback, backup inspection, and managed removal remain locally usable.

## Automated validation performed

| Check | Result |
| --- | --- |
| Full package validator | Pass, Validated 12 integrity groups for full planning package. |
| Repository-template validator | Pass, Validated 12 integrity groups for repository template. |
| Secret-pattern scan | Pass, No committed secret patterns found. |
| JSON Schemas | Pass, 9 schemas parsed and 10 examples validated by the package validator |
| Subagent TOML | Pass, 9 files parsed |
| Living skill frontmatter and update triggers | Pass, 10 skills checked |
| Goal prompt | Pass, 3920 characters and all mirrors match |
| GitHub YAML | Pass, parsed by the package validator |
| User-facing README boundary | Pass |
| Markdown style | Pass, no em dash characters or semicolons |
| Mermaid source inventory | Pass, 10 diagram files present |
| UI references | Pass, 17 full-resolution PNG files retained |
| Frontend typecheck, lint, unit (14 tests), accessibility, browser smoke | Pass |
| Frontend production build | Pass, Vite build completed on the elevated host run |
| JSON-schema examples (manifest, plan, lock, journal) | Pass |
| Cargo formatting | Pass |
| Rust all-feature tests | Pass, 113 tests |
| Rust all-feature clippy | Pass with `-D warnings` |
| Fuzz target compilation | Pass, manifest, relative-path, Codex-analysis, descriptor/thumbnail, and structured-TOML targets |
| Committed-secret scan | Pass |
| Windows Tauri release build | Pass, x64 executable and MSI bundle |
| Release artifact hash verification | Pass, local `ARTIFACTS.sha256` and strict exact-revision package mode verified against the committed bootstrap revision |
| Windows native desktop launch smoke | Pass |

## Manual implementation gates retained

The workspace contains the runtime core, React/Tauri boundary, and verified
Windows native packaging path. Release and live external integration evidence
remain incomplete.

Runtime completion still requires:

- browser and device-code login completion with a developer-owned ChatGPT account
- Windows and macOS launcher-path integration and live HOI4 launcher discovery tests
- release signing and notarization evidence
- a published/tagged app revision and CI-produced platform packages for public release; the local bootstrap commit is sufficient only for deterministic local verification
- native macOS compilation, packaging, and desktop E2E launch
- native screen-reader, contrast, scaling, and clean-machine evidence on both supported platforms
- a journaled rollback boundary for the high-impact 3D bootstrap's external environment changes; the source does not declare preflight-only behavior or rollback metadata
- a separate rollback-as-new-transaction backup; the current reversal is root-bound, journaled, and per-operation checkpointed, but rollback-as-new-transaction preview/backup semantics remain release work
- cross-process power-loss durability fixtures for the final journal/rollback-record/lock finalization window

## Source verification limits

The supplied project Markdown, TOML, and CSV sources were previously loaded and indexed for this planning package. The live workflow repository was inspected at commit `27128a7b311d728a959afff7238a9aeeb9987f2b`. Its checked-in manifest was stale at inspection time; the supplied checkout now contains an uncommitted deterministic regeneration from tracked Git bytes at that exact revision, and the workspace bootstrap manifest uses the same generated bytes. The remote branch remains unchanged, so installed plans continue to record `manifest_origin: bundled_revision_bootstrap` until upstream publication is corrected.

The installed Codex executable, `app-server --help`, and live JSONL `initialize` plus `account/read` exchange were verified on 26 July 2026. The current account is unauthenticated with `auth_mode=chatgpt`; browser/device login completion was not exercised because it requires the user to complete authentication. Mocked transport and schema-boundary tests remain the deterministic automated contract layer.

The source-declared Windows MCP wrapper was also manually probed through its
fixed JSONL route on 26 July 2026. The application now binds installed-project
readiness and transaction readiness to the locked manifest/config, resolves a
canonical `cmd.exe` and wrapper, performs bounded MCP `initialize` plus
read-only `tools/list` metadata validation, never invokes a tool, passes no
credential environment, and terminates the wrapper's child process tree after
the probe.

Filesystem containment now treats Windows reparse points as link components in
the shared security boundary, so junctions are rejected alongside symbolic
links during scanning, staging, backup, readiness, and rollback checks.

The body of every offline wiki article and every binary wiki or visual-reference asset was not individually inspected. Formal wiki licensing evidence was not verified. Those limits remain recorded in `docs/00_source_audit.md`.
