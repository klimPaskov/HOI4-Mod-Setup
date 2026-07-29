# Validation report

## Result

The revision 4 planning package passed its integrity checks. The current
workspace implementation passes the frontend, schema, security, template,
formatting, and all-feature Rust gates exercised after the provider,
flattening, journal, and release-metadata changes. A fresh unsigned Windows
NSIS package was built, architecture-checked, hash-verified, and launch-smoke
tested. The release workflow contains runner-only Windows and macOS signing
setup, signed-evidence markers, and curated publication-asset verification;
it fails closed when protected material is absent. macOS, signing, and
public-release gates remain explicitly bounded below.

## Verified package contract

- The first setup screen selects Codex (default), Claude, Kimi, GLM, DeepSeek, a local model, or another configured provider; the selected model and optimization profile are persisted.
- Codex uses the official local Codex App Server and ChatGPT-managed access; other providers use only their verified adapter, explicit endpoint, and OS-vault key route when required.
- All providers return the same schema-constrained proposal shape; no provider response can write files or approve a transaction.
- The Codex-only final option prepares a staged flattened ChatGPT project-sources folder with `<skill>.md` mappings and an offline Chat recommendation.
- Maintenance updates rebuild that view only from accepted source bytes and preserve a reviewed flat conflict when later non-flat content changes.
- Deterministic Rust validates facts, identifiers, paths, conflicts, descriptors, PNGs, plans, transactions, and readiness.
- Update planning requires a fresh bounded scan, visible approved evidence, and a core-session-confirmed selected-provider reanalysis record; provider, model, profile, and evidence bindings are carried into the typed maintenance plan and lock.
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
| Planning-package checksum inventory | Pass, 203 entries recomputed and matched. |
| JSON Schemas | Pass, 10 schemas parsed and 11 examples validated by the package validator |
| Subagent TOML | Pass, 9 files parsed |
| Living skill frontmatter and update triggers | Pass, 10 skills checked |
| Goal prompt | Pass, 3920 characters and all mirrors match |
| GitHub YAML | Pass, parsed by the package validator |
| User-facing README boundary | Pass |
| Markdown style | Pass, no em dash characters or semicolons |
| Mermaid source inventory | Pass, 10 diagram files present |
| UI references | Pass, 17 full-resolution PNG files retained |
| Frontend typecheck, lint, unit (25 tests), accessibility, browser smoke | Pass |
| Frontend production build | Pass, Vite build completed on the elevated host run |
| JSON-schema examples (manifest, plan, lock, journal) | Pass |
| Cargo formatting | Pass |
| Rust toolchain | Pass, pinned to Rust 1.88.0 by `rust-toolchain.toml` |
| Rust all-feature tests | Pass, 169 tests under pinned Rust 1.88.0 |
| Rust all-feature clippy | Pass with `-D warnings` under pinned Rust 1.88.0 |
| Fuzz target compilation | Pass, manifest, relative-path, Codex-analysis, descriptor/thumbnail, structured-TOML, and flattened-source targets |
| Committed-secret scan | Pass |
| Windows Tauri release build | Pass locally, unsigned NSIS `.exe` package built |
| Release artifact hash and architecture verification | Pass locally for frontend and NSIS package artifacts |
| Release signing configuration contract | Pass, workflow imports runner-only certificates/keychains and removes temporary signing roots; live credentials not available locally |
| Windows native desktop launch smoke | Pass |
| Release script syntax and revision-binding checks | Pass, `node --check` for build, verify, and publication-asset scripts; native strict verification remains gated on signed CI metadata |

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

## Source verification limits

The supplied project Markdown, TOML, and CSV sources were previously loaded and indexed for this planning package. The live workflow repository's public default branch now resolves to commit `54da3e7a43cce43f15edc54ef80fb0099822b3e2`. Its root manifest is published at that exact revision with SHA-256 `0e8db882f4ae61f7b030b415d4a575f643fd1a5c5dc475f7a0dcccc6933bd3ba` and `generated_for_revision` `27128a7b311d728a959afff7238a9aeeb9987f2b`. The workspace bootstrap manifest is parsed-content equivalent and remains available for offline bootstrap; a newly resolved install records the remote manifest origin and exact commit rather than substituting the bundle.

The installed Codex executable, `app-server --help`, and live JSONL `initialize` plus `account/read` exchange were verified on 26 July 2026. The current account is unauthenticated with `auth_mode=chatgpt`; browser/device login completion was not exercised because it requires the user to complete authentication. Mocked transport and schema-boundary tests remain the deterministic automated contract layer.

The current verified source manifest intentionally does not provide immutable
wrapper, command-interpreter, or runtime identity evidence for its Windows MCP
route. The application therefore binds readiness and transaction planning to
the locked manifest/config but reports that route as `planned_unavailable` and
does not execute a same-named PATH command. A live MCP health probe is not
release evidence until independently trusted executable provenance and
descendant launch containment are available.

Filesystem containment now treats Windows reparse points as link components in
the shared security boundary, so junctions are rejected alongside symbolic
links during scanning, staging, backup, readiness, and rollback checks.

The body of every offline wiki article and every binary wiki or visual-reference asset was not individually inspected. Formal wiki licensing evidence was not verified. Those limits remain recorded in `docs/00_source_audit.md`.
