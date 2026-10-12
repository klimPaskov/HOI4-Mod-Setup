# Public-readiness scanner audit

Date: 2026-09-08  
Scope: bounded existing-project scanner, Git read-only helpers, scan bridge, named scanner fixtures/tests, scanner design and owning skill. No application source was changed. The only written artifact from this audit is this handoff.

## Result and real-fixture evidence

The scanner remained read-only in the approved Chaos Redux run. The ignored test `scanner::tests::external_very_large_mod_fixture_completes_without_mutation` (`src-tauri/src/scanner.rs:4092`) passed with:

- 141 files, 37 directories, and 2,444,184 detector bytes;
- `partial=false` and no `limits_hit` entries;
- the fixture mutation guard unchanged; and
- approximately 1.9 seconds for the test (the terminal scan completed at about 1.86 seconds).

The fixture's `.git/config` has `extensions.worktreeConfig=true`. The hardened Git review therefore reports `status_probe="unsafe_configuration"` and leaves commit, dirty-state, and remote facts unavailable. This is the intended fail-closed behavior in `src-tauri/src/git.rs:2101-2103`; `src-tauri/src/scanner.rs:2037-2055` exposes it as the advisory `scan.git.inspection` warning, without making the targeted scan partial. It is a useful reproduction for the reported “import fails” experience if the UI treats that warning as a scan failure. The run did not exercise the expensive safe-repository Git path because it exits before child probes.

## Coverage table

| Surface | Result | Evidence and remaining risk |
|---|---|---|
| Read-only root and external boundary | Pass with bridge defect | `BoundedReadRoot`, handle identity checks, launcher approval, no-follow reads, and mutation tests cover the filesystem boundary. An approved launcher is allowed only after parent enumeration and `path=` agreement. The approved path is later serialized unsafely for semantic evidence; see F-SCAN-001. |
| Descriptor and thumbnail discovery | Partial | Internal and approved launcher descriptors are parsed, malformed descriptors become blocking conflicts, and thumbnails are bounded. `detect_descriptors` emits only `descriptor.name` (`scanner.rs:1752-1831`); valid but incomplete `version`, `supported_version`, `tags`, `picture`, and internal/launcher field agreement do not receive deterministic findings. The provider-owned ID/namespace/naming/localisation proposals are intentionally outside this scanner, as specified by `docs/03_scanner_design.md:42-49,123-126`. |
| Targeted inventory and irrelevant files | Pass | Classification precedes relevant-entry budgets; gameplay, localisation, media, wiki pages, tooling, caches, and ordinary dumps are skipped. The scanner tests cover pruning, entry budgets, links, case collisions, and sensitive names. The real fixture confirms no limit was charged by unrelated content. |
| Git root, branch, remotes, ignore, dirty state | Security pass; completeness is advisory | Git children use read-only/no-optional-lock profiles and recheck metadata. Linked worktrees, unsafe config, and linked metadata fail closed and remain visible warnings. Dirty status is intentionally limited to Agentic setup paths, so the result must not be presented as the whole-repository dirty state. Safe large Git performance is untested; see F-SCAN-002. |
| Documentation and absolute paths | Bounded inventory | README/docs/AGENTS presence and bounded absolute-path locations are reported. This is inventory evidence, not semantic validation or line-level parser evidence. Absolute paths in managed-lock values are not covered by the detector redaction test; see F-SCAN-004. |
| Skills and subagents | Mixed | Subagent TOML parsing and exact top-level `fork_context=false` are parser-backed. Skill validity is a string heuristic (`scanner.rs:2135-2145`) that checks normalized text for a start marker, `name:`, and `description:`; it does not require a closing delimiter, valid YAML, scalar types, or duplicate-key rejection. |
| Codex/MCP and native clients | Bounded but shallow | `.codex/config.toml` is TOML-parsed and MCP IDs are extracted (`scanner.rs:2198-2316`). Native `.mcp.json`, Cursor, Qoder, and OpenCode files are detected/JSON-parsed, but their server shape, command/cwd/environment/path safety, and secret-like values are not validated in the scan. The design intentionally defers those details to later structured review; the scan should keep labeling these as presence/parse facts rather than valid MCP configurations. |
| Managed installation | Parser-backed with privacy gap | A bounded, schema-checked lock is summarized without walking metadata. The summary includes portrait `local_root`, `local_server_url`, `runpod_url`, and `runpod_workspace` (`scanner.rs:2787-2807`). `redact_scan_value` only applies secret-pattern redaction (`scanner.rs:2848-2895`), so machine-local paths and route values can enter the approved semantic excerpt. |
| Finding model and evidence | Good helper invariants; runtime gaps | `finding()` sets deterministic origin, confidence, status-derived blocking/decision state, and an excerpt hash. `ScanFinding`/`ScanConflict` are free-form strings rather than Rust enums, and the scan path does not validate every result against the JSON Schema; the only scanner schema assertion is the valid managed-result test at `scanner.rs:4194-4229`. |
| Links, escapes, and path containment | Pass in bounded scanner tests | Root identity fences, linked setup files, Unix literal backslashes, case collisions, launcher approval swaps, path budgets, and secret-like paths are covered. The external path crossing into Codex is a separate validation failure, not a filesystem escape. |
| File/size/depth/time budgets and cancellation | Pass | Detector, launcher, lock, inventory, retained-memory, directory, conflict, timeout, and cancellation paths return honest partial/cancelled metadata. Partial results clear semantic approval as required by the skill. Git advisory warnings are deliberately excluded from partial classification. |
| Review groups, conflicts, confidence | Mostly pass | Stable finding IDs, evidence paths/hashes, confidence, recommendations, conflict kind/severity, and low-confidence review states are present. There is no end-to-end assertion that every emitted result has unique IDs, schema-valid enums, or a user-visible origin label after the React mapping. |

## Findings

### F-SCAN-001 — approved launcher evidence cannot cross the semantic bridge

This is a reproducible blocking correctness bug. `scanner.rs:1814-1828` creates the accepted `descriptor.launcher` finding with both its value and `ScanEvidence.path` set to `external.display()`, an absolute path such as the approved parent `.mod` file. `App.tsx:769,831` includes every non-rejected finding in the approved evidence set, and `src/lib/tauri.ts:353` takes the first evidence path. `commands.rs:1282-1289` calls `codex::validate_analysis_evidence`; `codex.rs:934-946` rejects any evidence path that fails `normalize_relative_path`.

Therefore an existing project with a confirmed launcher descriptor fails evidence approval before its semantic turn. The path also remains in the finding value/excerpt; redacting secret-shaped strings does not remove the machine-local absolute path. The existing bridge tests use `descriptor.mod` and do not cover a positive external launcher. Parent action: represent the approved companion as a stable redacted external reference (or omit it from model evidence), keep only project-relative descriptor facts in `ApprovedEvidence.path`, and add a command-level test that scans a project with a valid parent launcher and successfully approves the resulting evidence vector.

### F-SCAN-002 — repeated full Git metadata walks are a large-repository performance risk

Each hardened read-only child checks `git_child_metadata_links_are_safe` before and after the process (`src-tauri/src/git.rs:1166-1201`); the helper recursively inspects Git metadata (`:1876-1927`). The targeted status probe batches pathspecs and can invoke multiple children (`:1839-1873`), while the outer inspection performs another metadata check (`:2101-2105`). On a safe repository with a large `objects`/`refs` tree this can multiply metadata traversal by the number of Git probes. No test measures a large safe `.git` with multiple batches, and the Chaos Redux run exits at unsafe configuration before this path.

Retain the pre/post security checks, but parent work should measure this path and consider one retained, bounded metadata snapshot per scan/child chain or an explicit aggregate Git budget. Add a regression benchmark that records files, metadata entries, child count, elapsed time, cancellation, and partial status; do not remove the race/link rechecks.

### F-SCAN-003 — descriptor validity is syntactic plus `name`, not the documented field review

The descriptor parser is used for syntax and the name field, but `detect_descriptors` has no deterministic findings for valid-but-missing or inconsistent `version`, `supported_version`, `tags`, `picture`, or internal/launcher field agreement. This leaves false negatives for an existing descriptor that is parseable but not launcher/game-loadable. Add fixture cases for each field and distinguish parser evidence from provider proposals. This is separate from the provider-owned project ID, namespace, naming, and localisation proposal layer.

### F-SCAN-004 — managed portrait summary is not proven safe for semantic input

`installation.managed` includes the portrait local root and endpoint/workspace fields (`scanner.rs:2798-2807`). The design promises a safe workflow summary and no lock contents in semantic evidence (`docs/03_scanner_design.md:97-105`), while scanner redaction only calls `redact_secrets`. A valid lock containing a machine-local root or URL therefore has no test proving that those values are removed before `App.tsx` builds evidence. Parent action: reduce the summary to provider/status/commit and other non-path state, or explicitly redact local routes; add a test asserting the exact model-visible input contains neither absolute roots nor credential-shaped URL material.

### F-SCAN-005 — skill frontmatter heuristic can both accept malformed files and reject unusual valid files

The current predicate (`scanner.rs:2143-2145`) accepts any normalized text containing the three marker substrings. It can accept an unterminated or nested pseudo-frontmatter block and can reject valid YAML whose keys are quoted, reordered in an unusual representation, or otherwise not matched by the literal substring test. The only positive test is CRLF (`scanner.rs:3737-3750`), with no malformed-closing-delimiter, duplicate-key, type, or quoted-key cases. Either use the bounded YAML/frontmatter parser already required by the design or mark this as heuristic/needs-review and add the negative/false-positive matrix.

### F-SCAN-006 — native MCP configuration is presence/JSON evidence only

The scan treats native client files as coding-environment evidence and only extracts server IDs from `.codex/config.toml`; valid JSON with an invalid or dangerous `mcpServers` shape is not a deterministic conflict. This is acceptable only if the UI clearly says “configuration present/parseable” and later structured review validates commands, arguments, cwd, environment names, and paths. Add tests for malformed server values, duplicate IDs, absolute cwd/command paths, and secret-like environment names/values so the boundary cannot accidentally be represented as a valid MCP setup.

## Evidence, confidence, and false-result risks

Parser-backed evidence is strongest for descriptor syntax, TOML subagents/config, exact `fork_context`, JSON presence, hashes, and filesystem identity. Inventory counts and documentation are deterministic but low-information. Skill frontmatter and native MCP files are heuristic/shallow and should remain visible as review states. Provider proposals for `project_id`, namespaces, naming, descriptor tags, folder profile, and localisation are correctly separate from deterministic facts; provider confidence must not be copied onto scanner findings.

The largest false-positive risks are heuristic skill acceptance, treating native JSON as a valid MCP configuration, and presenting `scan.git.inspection` as a failed import rather than an incomplete advisory Git review. The largest false-negative risks are omitted descriptor fields, machine-local path material in managed evidence, and any UI path that silently drops the absolute launcher finding before the core validator reports the failure.

## Codex layer audit

The core protocol satisfies the required shape in the inspected paths:

- `codex.rs:260-395` gates account operations behind initialization, uses ChatGPT account type, supports browser and `chatgptDeviceCode` login, waits for completion/account update, and uses `account/logout`; token persistence remains Codex-owned.
- `codex.rs:502-521` starts a read-only, no-network App Server turn with `approvalPolicy=never`, the strict output schema, and a read-only sandbox policy.
- `codex.rs:868-968` bounds the approved input manifest, requires project-relative evidence, checks excerpt hashes, rejects secret/account-shaped input, and rejects duplicate references. Output validation is strict and evidence references are bound to the approved set.
- Scanner findings are labeled deterministic by `finding()`; provider proposals are kept in the semantic analysis record. Commands bind the approved evidence to the latest root/scan ID and block planning when Codex authentication, usage, completed analysis, or confirmation is unavailable. Existing tests cover initialize/account type, browser/device login, cancellation/logout, usage limits, interruption, schema rejection, redaction, and no-secret persistence.

The required missing Codex integration test is the external-launcher case in F-SCAN-001. There is also no single end-to-end test asserting the React-built approved manifest, core approval, App Server turn, strict output, origin labels, and planning-blocked state for a completed real scan. The unit tests are strong at the protocol boundary but do not catch this scan-to-bridge path mismatch.

## Recommended parent actions

1. Fix the external-launcher evidence representation and add the positive bridge test before claiming existing-project semantic readiness.
2. Ensure `scan.git.inspection`/`unsafe_configuration` is rendered as a reviewable Git limitation on Chaos Redux, not a failed or partial scanner result; preserve the no-executable-configuration guarantee.
3. Benchmark a safe large Git repository and add an aggregate Git metadata/time/cancellation budget without weakening pre/post link checks.
4. Add descriptor-field, skill-frontmatter, native-MCP-shape, managed-portrait-redaction, and full scan-result schema/ID uniqueness tests.
5. Keep Codex planning blocked until the corrected approved evidence vector is validated and a complete semantic result is available; do not bypass validation to accommodate absolute companion paths.

