# Audit: Claude account route (`claude_account`)

Date: 2026-10-03.
Role: read-only `hoi4setup_codex_integration_auditor`.
Scope: the accepted 2026-10-03 design for the default setup assistant (`claude_account`, `src-tauri/src/claude_code.rs`), the shared corrective-retry logic in all three adapters, publisher verification and its memo, the Claude login, logout, and analysis commands, provider-record validation, schemas, and the Welcome Claude panel.
No source, test, doc, or config file was modified; this report is the only file written.

## Verdict

The policy boundary holds: there is no app-owned Claude OAuth, no read of Claude Code credential storage, no pasted authorization code, no sign-in URL reaching the renderer, and no `ANTHROPIC_*` forwarding.
The route is not ready to be claimed complete under AGENTS.md section 17: four P2 defects affect correctness or the stated trust gate, and most section 17 items have no automated coverage for the Claude route.

## Findings (severity ordered)

No P0 or P1 findings.

### P2-1. Redaction runs on the raw Claude JSON before parsing, which can corrupt the envelope and masks secrets instead of rejecting them

Evidence: `process.rs` `run_with_profile` returns `stdout: redact_secrets(&stdout, &known)` (lines 573-576) for every reviewed tool, and `claude_code.rs` `analyze` redacts again before parsing with `extract_analysis_output(&redact_secrets(&result.stdout, &[]))` (line 626).
The third pattern in `security.rs` `secret_redaction_patterns` (line 390) consumes `[^\s,;&]+` after `api_key=`, `authorization:`, and similar keys, which in compact JSON includes the closing quote, brace, and bracket.
Failure scenario 1: a proposal reason or warning containing text such as `api_key=none` (or `authorization: required`) makes the regex eat `none"]}}` up to the next comma; the envelope no longer parses, `extract_analysis_output` returns `AppError::Protocol`, the corrective retry is skipped (Protocol is not Serialization), and the user sees "Claude Code could not complete this request".
This was reproduced with an equivalent Python regex on a compact Claude-style envelope: the output ends at `"warnings":["Keep the api_key=[REDACTED]` and fails to parse.
Failure scenario 2: a credential-shaped value (for example an `sk-ant-...` string) in a proposal is replaced by `[REDACTED]` before validation, so `validate_output_text` (`codex.rs` 1614-1630) sees already-redacted text, passes it, and the proposal is accepted with a `[REDACTED]` marker; the Codex and provider-API adapters reject the same response.
Benign words also mutate silently, for example "standard bearer units" becomes "standard bearer [REDACTED]".
Fix: give the Claude analysis spawn a raw-output profile (no known secrets exist for this route), parse and validate first so the deterministic validator rejects credential-shaped content, and redact only text that is displayed or persisted.

### P2-2. Any first-party Claude Code sign-in is treated as a Claude account sign-in; `authMethod` is read but never enforced

Evidence: `claude_code.rs` `status_from_summary` sets `authenticated = summary.logged_in && summary.first_party` (line 423) and `analyze` uses the same test (line 586); `auth_method` is parsed (lines 359-369) and then ignored.
`first_party` is also true when `apiProvider` is absent (line 373).
Failure scenario: a user who signed in to Claude Code with Console OAuth (`claude auth login --console`), or whose Claude Code settings supply an API key helper, is shown "Signed in to Claude" and "Setup analysis uses your Claude plan through Claude Code" (`App.tsx` line 2271), analysis is billed to Console API credits, and the plan, lock, and readiness record `auth_mode = claude_account`.
That contradicts the doc statement that only a Claude account sign-in qualifies (`docs/31_ai_provider_profiles_and_chat_sources.md` lines 103-107) and the provenance label.
Fix: require an allowlisted subscription `authMethod` (the verified Claude.ai value) for `authenticated`, map other first-party methods to a distinct "Claude Code is signed in with an API key or Console account" state, and add parser tests for each observed value.

### P2-3. The Codex corrective retry fires on an incomplete or timed-out turn, not only on a serialization rejection

Evidence: `codex.rs` `AppServerProtocol::analyze` maps "turn not completed" to `AppError::Serialization("Codex returned no schema-constrained analysis output")` (lines 627-634), and the retry arm accepts any `Serialization` (lines 638-642).
`drain_notifications` returns normally after `max_wait` of 120 seconds without a completion (lines 753-788), and no `turn/interrupt` is sent before the second `turn/start`.
Failure scenario: an `xhigh` turn that takes longer than 120 seconds (the handoff records live turns of 39-58 seconds, so this is reachable on a loaded machine) is treated as a rejected response; a second turn starts in the same thread while the first is still running, consuming usage twice.
When `turn_id_from_start_response` returns `None` (lines 865-879), `event_matches_turn` and `event_completes_turn` correlate by thread only (lines 918-943), so the first turn's late completion and output can satisfy the second drain and be validated and recorded as the corrective response.
Even with a turn ID, notifications that carry only a thread ID still match (line 924).
Fix: return `AppError::Process("... timed out")` when no completion was observed, interrupt the outstanding turn before any retry, retry only when a completed turn produced a rejected or missing structured output, and require a turn ID for the corrective turn.

### P2-4. macOS publisher verification is not anchored to Apple's Developer ID chain

Evidence: `process.rs` `validate_executable_publisher_uncached` runs `codesign --verify --strict --verbose=2` without a requirement (lines 907-922), then parses `codesign -dv --verbose=4` text with `macos_signature_matches_publisher` (lines 923-944, 958-986).
`codesign --verify` without `-R` validates the signature against the binary's own designated requirement, so a self-signed signature verifies; the publisher decision then rests on matching display text (`Authority=` and `TeamIdentifier=` lines).
Failure scenario: a lookalike `claude` placed earlier on PATH or in `~/.local/bin`, signed with a self-made certificate whose common name is `Developer ID Application: Anthropic PBC (Q6L2SF6YDW)`, passes the `Authority=` check; whether `TeamIdentifier=` can be forged for a non-Apple anchor is unverified, so the gate depends on an undocumented codesign behavior rather than a cryptographic check.
The same function protects the Codex route (`OpenAI`), and the Anthropic Team ID itself is recorded as unverified on a Mac in `session-progress-2026-10-03-claude.md`.
Fix: verify with an explicit requirement such as `codesign --verify --strict --all-architectures -R='anchor apple generic and certificate leaf[subject.OU] = "Q6L2SF6YDW"'` (and the OpenAI equivalent), treat that exit status as authoritative, and keep text parsing only as a secondary check.

### P3-1. Renderer account state is not bound to the selected provider

Evidence: `App.tsx` `selectProvider` (lines 2071-2087) resets `aiAccount` but does not cancel an active Claude login; `signInClaude` writes `aiAccount: value` after the wait if only the login ID still matches (lines 2219-2223); `refreshClaude` and `signOutClaude` write `claude_account` statuses unconditionally (lines 2197-2201, 2232-2243); the auto-read effect writes a resolved account without checking that the provider is unchanged (lines 597-600); `providerReady` never compares `aiAccount.provider` with `aiProvider` (lines 1880-1885).
Failure scenario: start Claude sign-in, switch to Kimi, finish the browser sign-in; `aiAccount` becomes an authenticated `claude_account` status and Continue is enabled for an unconnected Kimi profile.
The backend still blocks planning (`commands.rs` `require_ai_session` lines 497-517), so this is a gating and messaging defect rather than a bypass.
The Claude login process also keeps running for up to ten minutes with no visible Cancel button after the switch.
Fix: cancel the active Claude login in `selectProvider`, and ignore any account result whose `provider` differs from the current `aiProvider`.

### P3-2. Claude sign-out does not invalidate an in-flight analysis

Evidence: `claude_logout` clears `codex_analyses` and approved evidence (`commands.rs` lines 1062-1073, 477-481), but `ai_analyze_blocking` inserts its result after the provider call returns (lines 1491-1511) with no session generation check, and Claude analysis has no cancellation (`claude_code.rs` line 615 passes `should_stop = None`).
Failure scenario: sign out while a five-minute analysis is running; the analysis finishes and re-creates a pending proposal set from the signed-out session, which the renderer can receive after it cleared its state.
Planning still re-checks sign-in, so the impact is stale proposals surviving sign-out.
Fix: add a per-provider session generation that logout increments and analysis checks before insertion, and pass a cancellation closure to the analysis spawn that logout and window close can trigger.

### P3-3. Login command races

Evidence: `claude_login_wait` looks up the cancellation flag but does not claim the attempt (`commands.rs` lines 1026-1046), and `cancelled` is read after `run_login` returns (line 1036).
Failure scenario 1: two `claude_login_wait` calls with the same ID start two `claude auth login` processes sharing one flag.
Failure scenario 2: a cancel that lands after a successful sign-in reports "Claude sign-in was cancelled." although Claude Code is now signed in; the next status check corrects it.
Fix: remove (claim) the entry atomically when the wait starts and keep a separate in-flight flag; read the flag only when `run_login` returned a cancellation error.

### P3-4. Executable identity gaps between verification and spawn

Evidence: `find_executable` calls `validate_executable_publisher` and then hashes the file again with a separate `sha256_file` (`claude_code.rs` lines 256-262); spawn re-hashes in `ProcessSpec::validate` (`process.rs` lines 133-142) and then launches by path (line 444).
Failure scenario: with write access to the launcher location, the signed bytes are present during the signature check and replaced before `find_executable` hashes them; the cached hash then belongs to unsigned content and every later spawn check passes.
This requires same-user write access, so it is hardening rather than a privilege boundary.
Fix: have `validate_executable_publisher` return the hash it verified and use it as the cache identity; on Windows, hold a deny-write handle across hash and spawn.

### P3-5. Windows Authenticode check matches only the leaf common name

Evidence: `process.rs` lines 855-901 accept any `Valid` signature whose `SimpleName` equals `Anthropic, PBC`; issuer, code-signing EKU, and the CurrentUser root store are not constrained.
Fix: also pin the issuing CA or the leaf certificate's organization and EKU, matching the accepted "Anthropic, PBC" identity more strictly.

### P3-6. Retry and binding details

`ai.rs` `request_provider` returns `Serialization` for an oversized body and for an unparseable envelope (lines 702-719, 726; `lib.rs` lines 64-67), so those are retried with a second paid request, while the Claude adapter returns early on truncation (`claude_code.rs` lines 620-624); the adapters disagree on which failures are retried.
`corrective_analysis_prompt` claims the reason is never user content (`codex.rs` lines 1275-1277), but `serde_json` error text produced by `serde_json::from_value` (line 1372) can quote the model's own value; it is bounded to 300 characters and redacted, so this is a documentation inaccuracy, not a leak.
`keep_requested_mod_name` (`codex.rs` lines 1471-1503) forces only `display_name`; `project_id`, `script_prefix`, and `primary_namespace` can still derive from a different model-proposed name, and the record's `output_sha256` binds the normalized analysis rather than the raw provider response, which should be stated in the doc.
The Claude record stores `reasoning_effort = "high"` for Haiku although no effort is forwarded (`claude_code.rs` lines 529-532, 655).

### P3-7. Schema and documentation drift

`scanner.rs` lines 678-684 always emit `semantic_analysis.engine = codex_app_server` and `auth_mode = chatgpt`, and `scan-result.schema.json` lines 315-318 make those constants, although the default provider is now `claude_account`.
`docs/31_ai_provider_profiles_and_chat_sources.md` lines 119-121 list only `CLAUDE_CONFIG_DIR`, proxies, and `NODE_EXTRA_CA_CERTS`, while `PASSTHROUGH_ENVIRONMENT` also forwards `USER`, `LOGNAME`, and `ProgramData` (`claude_code.rs` lines 65-77).
The isolation claim that user and project customizations, hooks, plugins, and memory files are disabled rests on `--safe-mode`, whose semantics have no cited upstream reference in the repo; the `--help` probe confirms only that the token exists (`claude_code.rs` lines 206-211).
`--effort` is passed for non-Haiku models but is not in `REQUIRED_PRINT_FLAGS` (lines 48-58).

### P3-8. Unverified operational risks

The child receives no `DISABLE_AUTOUPDATER` or non-essential-traffic setting, so an analysis turn may let Claude Code update itself, which changes the binary fingerprint mid-session.
Each analysis uses a new temporary working directory, which may add one project entry per run to the user's Claude Code configuration; this was not verified because no Claude Code binary is on this auditor's PATH.
npm installs (`claude.cmd`) are reported as "not installed" (`claude_code.rs` lines 136-146), and unsigned lookalikes earlier on PATH are re-verified on every account check because failed checks are not memoized (`process.rs` lines 801-819).

## Missing tests (AGENTS.md section 17 for the Claude route)

| Item | Current coverage | Gap |
| --- | --- | --- |
| Browser login | renderer mock test (`App.test.tsx` 464-475); ignored live test starts and cancels only | no backend test of `run_login` exit-code and timeout mapping or of `claude_login_wait` returning a refreshed status |
| Device-code login | not applicable; terminal fallback is documented | no test that the renderer never receives a URL or code from the Claude commands |
| Cancellation | renderer test (477-492); generic process cancellation test in `process.rs` | no test for `claude_login_cancel`, start-cancels-previous, window-close cancellation, or the cancel-after-success race |
| Logout | renderer test checks cleared renderer fields (505-515) | no backend test that `claude_logout` clears `codex_analyses`, approved evidence, and ready projects, including on a failed logout |
| Usage limits | `extract_analysis_output` maps a limit result to `Credential` | no test of `claude_user_error` categories and no renderer test of the Claude usage-limited message keeping the draft |
| Interruption | none | no test of Claude analysis timeout, truncated output, non-zero exit with empty stdout, or a killed child; `analyze` has no injectable runner |
| Schema rejection and retry | extractor tests only | no test that the Claude loop retries once on `Serialization`, never on `Protocol`, `Credential`, or truncation, and that the record binds the accepted second response |
| Redaction | sign-in summary and hostile `authMethod` tests | no test for P2-1 (envelope corruption and masking) or for `claude_user_error` never echoing raw text |
| No-secret persistence | `account_identity_persisted = false` asserted only in the ignored live test | no unit test that a `claude_account` record passes `validate_confirmed_record`, `confirm_analysis_record`, plan validation, lock migration, and readiness, and that `claude_code_cli` with any other provider or auth mode is rejected |

## Data-leak risks

No path was found by which Claude account identity, tokens, raw Claude output, or raw stderr reaches the renderer, logs, plans, or locks: status keeps three fields, errors are mapped to fixed strings in `claude_user_error`, and non-test code in `claude_code.rs` does not log.
The residual risks are P2-1 (credential-shaped model output accepted as a masked value rather than rejected) and P3-6 (model-quoted text inside a corrective prompt sent back to the same provider).

## State-machine risks

P3-1 (provider switch during login or refresh), P3-2 (sign-out during analysis), and P3-3 (duplicate wait and cancel-after-success) are the open races; window close cancels logins but not a running Claude analysis, and on macOS a running `claude --print` child can outlive the app because only Windows uses a kill-on-close Job Object (`process.rs` lines 55-121).

## Failing acceptance criteria

- AGENTS.md section 17: interruption, schema rejection, logout, cancellation, and no-secret persistence lack Claude-route tests (see the table).
- "Records use auth mode `claude_account`" for Claude account sign-ins only: violated for Console or API-key sign-ins (P2-2).
- "Retry only after a serialization rejection by `validate_analysis_output`": violated by the Codex timeout path (P2-3) and by the provider-API size and envelope paths (P3-6).
- "Requires Anthropic signing (macOS Developer ID team Q6L2SF6YDW)": not cryptographically enforced (P2-4) and the Team ID is unverified on a Mac.

## Recommended fix order

1. P2-1: raw-output parsing for the Claude analysis, then validation, then display-time redaction, with a regression test using the reproduced envelope.
2. P2-3: classify an incomplete Codex turn as a timeout, interrupt before retry, and require a turn ID for the corrective turn.
3. P2-2: enforce the Claude.ai `authMethod` and add the distinct non-subscription state.
4. P2-4: anchored `codesign -R` requirements for Anthropic and OpenAI, then verify the Team ID on a Mac.
5. Add an injectable runner to `claude_code::analyze` and the section 17 tests from the table.
6. P3-1 to P3-3 state-machine fixes with renderer and command tests.
7. P3-4 to P3-8 hardening and documentation alignment.

## Validation run

Targeted unit tests were run read-only in a separate target directory (`target/auditor-tests`); results are recorded below.

Command: `cargo test -p hoi4-mod-setup --all-features --lib -- claude_code:: codex::tests::a_requested process::tests::a_verified_publisher process::tests::unsigned_lookalike ai::tests::claude_account` with `CARGO_TARGET_DIR=target/auditor-tests`.
Result: 15 passed, 0 failed, 1 ignored (`live_claude_code_route`, which needs the user's installed Claude Code).
These passing tests cover argument construction, sign-in summary parsing, result-category mapping, the flag probe, launcher candidates, passthrough names, the requested-name override, and the publisher memo; they do not exercise any of the defects above, which matches the gaps in the missing-tests table.
No Claude Code binary is on this auditor's PATH, so live behavior (`--safe-mode` semantics, `authMethod` values, configuration side effects) was not observed.

## Parent disposition (2026-10-03)

- P2-1 implemented: `run_reviewed_tool(..., raw_stdout)` returns unredacted stdout for the Claude analysis, which is parsed and validated before any redaction; covered by `credential_shaped_output_is_rejected_not_masked_and_harmless_key_words_parse`.
- P2-2 implemented: `ClaudeSignInSummary::is_claude_plan` requires `authMethod = "claude.ai"` (confirmed from the Claude Code 2.1.286 bundle, which adds email and organization fields only for that value); Console or API-key sign-ins get a distinct message; covered by `only_a_claude_plan_sign_in_is_the_claude_account_route`.
- P2-3 implemented: an incomplete Codex turn returns a timeout `Process` error and is never retried, and a corrective turn requires the completed turn's ID; covered by `an_incomplete_codex_turn_is_a_timeout_and_is_not_retried` and the updated `analysis_without_schema_constrained_output_is_rejected`.
- P2-4 implemented in code: `codesign --verify --strict --all-architectures -R=<Developer ID requirement with the reviewed Team ID OU>`; covered by `macos_requirements_are_anchored_to_developer_id_and_the_reviewed_team`. Native macOS execution and the Anthropic Team ID remain unverified on a Mac.
- P3-1 implemented: switching provider cancels a pending Claude sign-in, late results for another provider are ignored, and `providerReady` requires a matching provider; covered by the App test `cancels a pending Claude sign-in when another provider is selected`.
- P3-2 implemented: `claude_logout` advances a session generation and `claude_code::analyze` discards a result from an earlier generation; covered by `sign_out_advances_the_session_generation`. Killing a running Claude analysis process on sign-out is not implemented.
- P3-3 implemented: each login ID can be waited on once, and a cancel after a successful sign-in no longer reports cancellation.
- P3-4 implemented: `verified_executable_sha256` returns the exact verified hash used as the cache identity.
- P3-5 open: the Windows check still matches the leaf common name; issuer pinning is not implemented.
- P3-6 implemented for transport errors: oversized and unreadable provider responses are `Process` or `Protocol` errors and are not retried; Claude analysis retries are covered by `a_rejected_response_gets_one_corrective_turn_and_the_record_binds_the_accepted_one` and `unreadable_timed_out_and_truncated_runs_are_never_retried`. `keep_requested_mod_name` still forces only the display name.
- P3-7 implemented for docs and schema: doc 31 lists the full environment and `scan-result.schema.json` accepts every engine, auth mode, and transport; the scanner placeholder still reports Codex values because the scanner does not know the selected provider. `--safe-mode` semantics come from the installed CLI help only.
- P3-8 implemented in part: `DISABLE_AUTOUPDATER=1` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` are set for every app-started Claude Code process, and analysis uses a stable app-owned empty workspace; npm `claude.cmd` installs remain unsupported.
