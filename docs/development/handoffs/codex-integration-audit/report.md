# Codex integration audit handoff

Audit status: **completion gate not satisfied**. This is a bounded, read-only static audit. It is not a completion claim.

No real ChatGPT login, device-code login, external network access, token-store inspection, or credential inspection was performed. No tests were executed because this role was permitted to write only this handoff; the findings below assess the tests present in the named files.

## Files inspected

- `AGENTS.md`
- `.agents/skills/hoi4-mod-setup-codex-integration/SKILL.md`
- `docs/30_codex_chatgpt_authentication.md`
- `schemas/codex-analysis.schema.json`
- `src-tauri/src/codex.rs`
- `src-tauri/src/commands.rs`
- `src-tauri/src/models.rs`
- `src-tauri/src/migrations.rs`
- `src/lib/tauri.ts`
- `src/App.tsx`
- `src/App.test.tsx`

No source outside that list was inspected.

## Severity-ordered findings

### BLOCKER-1 — The required completion gate has no route-complete test evidence

`AGENTS.md:382-384` requires browser login, device-code login, cancellation, logout, usage limits, App Server interruption, output-schema rejection, redaction, and no-secret persistence to be covered before completion. The Codex skill adds startup/shutdown, existing sessions, failure handling, deterministic identifier rejection, and signed-out recovery at `.agents/skills/hoi4-mod-setup-codex-integration/SKILL.md:104-119`.

The present tests cover only fragments:

- `src-tauri/src/codex.rs::tests::initialize_is_first_request_and_initialized_notification_follows` checks two emitted messages but not command-level startup or refusal to issue pre-initialize requests.
- `src-tauri/src/codex.rs::tests::login_wait_requires_completion_and_account_update_before_reread` covers one notification success sequence, not browser launch, login start parameters, cancellation, failure, device code, timeout, or interruption.
- `src-tauri/src/codex.rs::tests::rate_limit_response_marks_reached_buckets_as_limited` tests a pure parser only.
- `src-tauri/src/codex.rs::tests::protocol_exposes_supervised_transport_liveness` tests a Boolean getter, not child replacement and re-initialize.
- `src-tauri/src/codex.rs::tests::extra_analysis_fields_are_rejected` covers one malformed-output case only.
- `src-tauri/src/codex.rs::tests::confirmation_is_bound_to_the_exact_analysis_and_proposal_keys` confirms a single proposal key and does not test the required field set or renderer/plan gates.
- `src-tauri/src/migrations.rs::tests::persisted_account_identity_is_rejected` covers one forbidden project-state field, not plans, locks, journals, logs, process errors, or crash output.
- `src-tauri/src/commands.rs:2952-3147` has no startup, login, cancellation, logout, usage-limit, restart, schema, or signed-out recovery command test.
- `src/App.test.tsx:26-101` has no Codex authentication, analysis, usage, logout, interruption, confirmation, or recovery test.

The integration therefore remains unproven even where implementation code appears directionally correct.

### HIGH-1 — Signed-out recovery, rollback, and managed removal are not reachable from a fresh UI session

Evidence:

- `src/App.tsx::goNext`, lines 545-547, blocks leaving Welcome while signed out.
- `src/App.tsx:769-781` exposes only Create, Import, sign-in, and a non-functional recent-project placeholder; it has no signed-out recovery or managed-removal entry.
- `src/App.tsx:222-227` searches for an interrupted transaction only after `identity.projectRoot` is already populated.
- `src-tauri/src/commands.rs::rollback_installation`, `read_transaction_journal`, `find_interrupted_transaction`, `resume_installation`, and `discard_installation_staging`, lines 2781-2879, correctly have no Codex-session guard, but the fresh UI cannot reach them.
- `src-tauri/src/commands.rs::build_maintenance_plan`, lines 2172-2186, skips the live-session check for `remove` but still requires a stored confirmed `codex_analysis`. A legacy or otherwise valid lock without that semantic record cannot produce a local removal plan.

This fails `docs/30_codex_chatgpt_authentication.md:20-26` and `AGENTS.md:149-151`, which require recovery, rollback, backup inspection, and managed removal to remain locally available while signed out.

Missing tests:

- Fresh signed-out launch can select a project and discover an interrupted transaction.
- Resume, rollback, journal inspection, staging discard, and managed removal remain callable without starting App Server.
- Managed removal handles a valid legacy lock without requiring online semantics.

### HIGH-2 — An authenticated, usage-limited account passes the planning guard

`src-tauri/src/commands.rs::require_codex_chatgpt_session`, lines 290-308, returns `Ok(())` for any available, authenticated ChatGPT session before evaluating `status.usage_limited`. The `else if status.usage_limited` branch is therefore unreachable for the normal authenticated limited-account state.

That guard is used by:

- `src-tauri/src/commands.rs::build_plan`, line 1667.
- `src-tauri/src/commands.rs::build_maintenance_plan`, lines 2172-2174.
- `src-tauri/src/commands.rs::preview_descriptors`, line 1061.

`src-tauri/src/codex.rs::AppServerProtocol::analyze`, lines 335-350, does block a new semantic turn, but a previously confirmed analysis can still proceed into new planning while usage is limited. `src/App.tsx:776-780` does not display `usage_limited`, and `src/lib/tauri.ts::invokeCommand`, lines 78-88, collapses a command error to `null`, leaving only a generic analysis failure.

This fails the usage-availability planning rule in `docs/30_codex_chatgpt_authentication.md:66-70`.

Missing tests:

- Authenticated plus `usage_limited = true` rejects descriptor preview, Create/Import planning, Update planning, and new analysis without mutating draft, scan, or prepared-plan state.
- The UI presents a distinct resumable usage-limited state.
- Recovery commands remain available in the same state.

### HIGH-3 — A core-only absolute project path is transmitted to Codex and carried into plan/lock records

Evidence:

- `src/App.tsx::runSemanticAnalysis`, lines 285-293, includes the selected absolute `project_root` for existing-project analysis.
- `src/App.tsx::runMaintenanceReanalysis`, lines 379-392, does the same for maintenance.
- `src-tauri/src/codex.rs::analysis_prompt`, lines 587-591, serializes the complete `CodexAnalysisRequest`, including `project_root` and `scan_id`, into the model prompt.
- `src/App.tsx::Findings`, lines 842-845, previews finding IDs, relative evidence paths, and values, but does not disclose that the absolute root and internal scan identifier are also transmitted.
- `src-tauri/src/codex.rs::AppServerProtocol::analyze`, lines 381-397, copies `request.project_root` into `CodexAnalysisRecord`.
- `src-tauri/src/models.rs::CodexAnalysisRecord`, lines 970-994, serializes that field.
- `src-tauri/src/models.rs::InstallationPlan` and `InstallationLock`, lines 631-661 and 728-749, both carry `CodexAnalysisRecord`.
- `src-tauri/src/commands.rs:2038-2050` and `2554-2561` place the record in setup and maintenance plans.

An absolute local path can disclose local account names, directory layout, or private project naming. It is not needed in the model prompt; it is a core authorization binding. Carrying it into a project-resident lock also conflicts with the bounded persisted-field list in `docs/30_codex_chatgpt_authentication.md:119-130`.

Missing tests:

- Prompt snapshots prove that core-only root and scan bindings are excluded from model input.
- The exact user-visible input preview matches transmitted input.
- Serialized plan and lock records contain only the accepted digest/key/time fields and no absolute local root.

### HIGH-4 — Output and confirmation allow a one-field analysis to authorize planning

Evidence:

- `schemas/codex-analysis.schema.json:40-45` requires only one proposal and has no mode-specific required proposal keys.
- `src-tauri/src/codex.rs::validate_analysis_output`, lines 620-663, validates every proposal that is present but does not require the full proposal set for `new_project_identity` or `existing_project_semantics`.
- `src-tauri/src/codex.rs::confirm_analysis_record`, lines 1269-1292, accepts any non-empty subset of returned proposal keys.
- `src-tauri/src/codex.rs::tests::confirmation_is_bound_to_the_exact_analysis_and_proposal_keys`, lines 1464-1501, explicitly demonstrates successful confirmation with only `project_id`.
- `src/App.tsx::confirmAnalysis`, lines 407-419, confirms all returned keys, even if required keys were omitted by the analysis.
- `src-tauri/src/commands.rs::codex_analysis_from_state`, lines 1609-1626, and `build_plan`, lines 1666-1671, treat that record as sufficient.
- UI defaults at `src/App.tsx:23-75` can consequently survive for semantic fields Codex was required to propose.

This weakens the required Codex semantic boundary and permits planning without confirmed Codex proposals for all required identity/convention fields.

Missing tests:

- Mode-specific required keys are enforced before a result enters pending state.
- Confirmation rejects missing required keys and partial confirmation of required fields.
- Edited values invalidate or re-bind the confirmation as designed.
- Descriptor preview and planning reject a record that confirms only a non-complete proposal set.

### HIGH-5 — Login cancellation and device fallback are not a usable bounded state machine

Evidence:

- `src-tauri/src/codex.rs::AppServerProtocol::wait_for_login`, lines 244-289, holds the session for up to 120 seconds and has no caller cancellation signal.
- `src-tauri/src/commands.rs::codex_login_wait`, lines 445-448, runs that wait while `with_codex_session` holds the global session mutex.
- `src/App.tsx::Welcome::signIn`, lines 748-763, starts the wait in the background but exposes no cancel action.
- A device-code start or logout issued while the browser wait is stuck must acquire the same mutex, so the visible fallback can block behind the failed browser wait.
- `src-tauri/src/codex.rs::account_login_notification`, lines 880-919, maps `success = false` to a generic credential error. There is no explicit cancelled state.
- `src/lib/tauri.ts::invokeCommand`, lines 78-88, discards the distinction among cancellation, failure, timeout, and App Server interruption.

The draft is not directly mutated by this path, but the required immediate fallback and retry behavior is not implemented as a controllable state machine.

Missing tests:

- Browser success, explicit cancellation, callback failure, timeout, and child exit.
- Device-code fallback can start without waiting for the abandoned browser wait.
- Logout can interrupt a pending login wait.
- Each failure preserves draft and scan state and starts no plan/transaction.

### HIGH-6 — Remote logout failure is cleaned up in core but hidden from the user

The local cleanup itself is correctly structured in `src-tauri/src/commands.rs::codex_logout`, lines 451-476: it drops the process and clears pending analyses and approved evidence even when `account/logout` fails.

However:

- `src/lib/tauri.ts::invokeCommand`, lines 78-88, catches the command error and returns `null`.
- `src/lib/tauri.ts::logoutCodex`, lines 112-114, reduces the result to a Boolean.
- `src/App.tsx:778` ignores that Boolean and always refreshes account state.

The required visible remote-failure state is therefore lost. A restarted App Server can report the still-active Codex-owned session, making the attempted sign-out appear to have silently failed with no retry guidance. React also retains its stale analysis objects until later navigation, although the core correctly refuses to reuse them.

Missing tests:

- Remote logout success clears core and renderer state.
- Remote logout failure still clears the local child, pending analysis, and evidence, while the UI displays retry guidance.
- No stale confirmed record can preview or plan after either logout route.

### MEDIUM-1 — Local output validation does not fully enforce the checked-in schema or output redaction

Evidence:

- The schema requires `project_summary` length 1 to 1200 at `schemas/codex-analysis.schema.json:35-39`.
- `src-tauri/src/codex.rs::validate_analysis_output`, lines 620-632, checks only the upper bound, so an empty summary is locally accepted.
- `validate_analysis_output` and `validate_proposal_value`, lines 594-773, do not apply `redact_secrets` or reject account-/credential-shaped output in summaries, reasons, warnings, display names, or descriptions.
- `src-tauri/src/commands.rs::project_readme`, lines 1594-1606, can persist an accepted `project_description` after confirmation.
- The only schema rejection test is the extra top-level field case at `src-tauri/src/codex.rs:1359-1383`.

The server-supplied `outputSchema` is a useful first line of defense, but the deterministic core is expected to reject malformed or unsafe output independently.

Missing tests:

- Missing required fields, empty summary, malformed UUID/hash/mode, unknown nested fields, duplicate keys, invalid confidence, bad evidence refs, overlong values, and wrong value types.
- Invalid project ID, script prefix, namespace, tags, and folder paths.
- Credential-shaped or account-shaped output is rejected before UI display and before any generated artifact.

### MEDIUM-2 — Child shutdown and restart are not lifecycle-tested, and normal application exit has no explicit teardown hook

Evidence:

- `src-tauri/src/codex.rs::ProcessJsonlTransport::close` and `Drop`, lines 1150-1167, terminate the child tree when the transport is explicitly dropped.
- `src-tauri/src/commands.rs::with_codex_session`, lines 253-287, replaces a child observed as dead and initializes the replacement.
- `src-tauri/src/commands.rs::run`, lines 365-400, registers commands but has no explicit Tauri exit handler that takes and closes the static `CODEX_SESSION`.
- The only restart-related test is `protocol_exposes_supervised_transport_liveness`, which does not construct a replacement process, assert a second initialize handshake, or verify pending-state cleanup.

The process may exit on inherited stdio closure when the desktop process exits, but that behavior is not an explicit, deterministic shutdown contract in the named code.

Missing tests:

- Spawn, initialize, JSONL request/response framing, clean close, process-tree termination, and application exit.
- Child exit before request, during account read, during login, during turn, and after analysis.
- Retry starts exactly one replacement, completes initialize again, and does not reuse stale pending analysis.

### MEDIUM-3 — Turn completion is not correlated to the started thread/turn

`src-tauri/src/codex.rs::AppServerProtocol::analyze`, lines 351-379, starts a dedicated thread and turn. However, `drain_notifications`, lines 401-423, stops on any structured output or any method ending in `/completed`; `structured_output`, lines 933-973, accepts output from several generic message shapes without checking the started thread or turn identifier.

An unrelated completion can terminate collection early, and stale or unrelated structured output is considered before correlation. Input digest and mode checks reduce acceptance risk, but they do not make the protocol state machine deterministic.

Missing tests:

- Interleaved account, login, item, thread, and turn notifications.
- Unrelated `/completed` notifications do not end the active turn.
- Only output bound to the started thread/turn is accepted.
- Child closure or timeout after `turn/start` preserves local state and rejects partial output.

### MEDIUM-4 — The browser flow does not demonstrate system-browser opening or validate returned URLs

`src/App.tsx:779` renders the App Server `auth_url` as a `target="_blank"` anchor. The named Tauri setup at `src-tauri/src/commands.rs::run` has no explicit system-opener command/plugin, and `src-tauri/src/codex.rs::parse_login_start`, lines 844-870, accepts arbitrary URL strings without scheme/host validation.

This does not prove the required system-browser behavior on packaged Windows or macOS. A malformed or unexpected App Server URL also reaches the renderer unchecked.

Required external gates:

- Packaged Windows and macOS tests with a fake App Server verify that the returned browser URL is opened through the approved system-browser boundary.
- URL validation allows only the documented secure authentication and device-verification forms.
- No real account or network login is needed for these tests.

### MEDIUM-5 — `account/read` can infer authentication from account type alone

`src-tauri/src/codex.rs::parse_account_status`, lines 783-827, defaults `authenticated` to true when `auth_mode == "chatgpt"` if none of the recognized Boolean fields is present. There is no signed-out/session-expiry protocol test for this shape.

If an App Server version returns a ChatGPT account descriptor without one of the recognized status Booleans, the core can treat it as authenticated. The subsequent rate-limit call is not a substitute for explicit authentication state.

Missing tests:

- Signed-out `account/read` shapes.
- Expired-session and refresh-failure shapes.
- Unknown account types and incomplete ChatGPT account objects fail closed.

### LOW-1 — JSONL input size is checked after `BufRead::lines` has allocated the line

`src-tauri/src/codex.rs::spawn_line_reader`, lines 1097-1120, applies `MAX_JSONL_LINE_BYTES` only after `lines()` returns a complete `String`. A defective or compromised child can force allocation beyond the stated bound before rejection.

The transport also has no direct framing tests for split writes, blank lines, invalid UTF-8/JSON, oversized lines, EOF, or timeout.

## Data-leak and containment assessment

Observed safeguards:

- No OpenAI API-key field or provider fallback appears in the named product UI or core logic. The API-key string in `src-tauri/src/codex.rs:1339-1344` is a negative account-type unit test, not a product path.
- No model override is sent in `thread/start` or `turn/start`.
- `src-tauri/src/codex.rs:549-558` supplies read-only restricted sandboxing with no readable project roots.
- `turn/start` sets `approvalPolicy: "never"` and includes the checked-in `outputSchema` at lines 351-371.
- Input briefs, constraints, evidence paths, evidence hashes, duplicate references, and credential-shaped excerpts receive deterministic checks at `src-tauri/src/codex.rs:426-522`.
- `src-tauri/src/migrations.rs:42-160` rejects a bounded list of Codex account/credential fields from project state.
- The persisted `CodexAnalysisRecord` has no token, email, account ID, plan, usage, or rate-limit field.

Outstanding leak risks:

- Absolute local project roots and internal scan IDs are included in model prompts and analysis records (HIGH-3).
- Model output is not screened for credential- or account-shaped content before display or possible rendering (MEDIUM-1).
- No test serializes project state, setup plans, maintenance plans, and locks and scans them for account/token fields.
- `CodexAnalysisRecord` at `src-tauri/src/models.rs:970-994` lacks `deny_unknown_fields`; malformed incoming records with extra sensitive-looking fields are not explicitly rejected at that type boundary, even if normal reserialization would omit them.
- Full live account email and plan type are transiently available in `CodexAccountStatus` and React state as allowed by the design, but no test proves they are cleared on logout/session expiry and excluded from persistence and diagnostics.

## Protocol and state-machine risks

- The global session mutex serializes a 120-second login wait with fallback and logout.
- Command errors are collapsed to `null`, erasing cancellation, usage, interruption, schema, and logout-failure states.
- Turn events are not correlated to the started thread/turn.
- Restart is reactive only on the next command and has no command-level replacement test.
- No explicit normal-exit child teardown is registered.
- Browser opening and returned URL validation are not owned by a tested native boundary.
- `account/read` parsing is permissive when the explicit authentication Boolean is absent.

## Acceptance criteria that fail or remain unproven

Fail in the named implementation:

- Usage limits block new planning.
- Signed-out recovery and managed removal are reachable in the core product UI.
- Only previewed/approved inputs are transmitted.
- Persisted Codex records contain only the documented bounded fields.
- Required semantic proposal sets are complete before confirmation/planning.
- Remote logout failure remains visible while local cleanup succeeds.
- Browser failure can transition promptly to device-code fallback or cancellation.
- Local output validation fully enforces the checked-in schema and rejects credential-shaped output.

Implemented directionally but unproven by current tests:

- Initialize-before-account/thread order.
- Browser login success and account update wait.
- Device-code login.
- Login cancellation/failure/timeout.
- Child shutdown, crash detection, and restart.
- OutputSchema attachment and broad rejection matrix.
- Read-only, no-project-root semantic turns.
- Draft/scan preservation on every failure.
- No token/account data in state, plan, lock, journal, logs, crash output, or project files.
- Deterministic rejection of all invalid proposed identifiers and paths.
- User confirmation before descriptor preview and planning.
- No Codex write, transaction approval, conflict resolution, or readiness authority.
- Recovery, rollback, backup inspection, and removal while signed out.

Observed in the named code, subject to regression tests:

- Browser login request uses `type: "chatgpt"` and the documented hosted-success parameters.
- Device login request uses `chatgptDeviceCode`.
- Account state uses `account/read` with refresh disabled.
- Logout invokes `account/logout`.
- No normal OpenAI API-key fallback or hardcoded model path is present.
- Analysis uses a dedicated `thread/start`, read-only/no-root sandboxing, `approvalPolicy: "never"`, and the checked-in schema.
- Installation approval, conflict resolution, apply, rollback, and readiness are separate deterministic commands rather than Codex actions.

## Exact missing tests and external gates

Add fake-transport or fake-child integration coverage for:

1. Startup, initialize response, initialized notification, account read, framing, close, and restart ordering.
2. Existing signed-in ChatGPT account, signed-out account, unsupported account type, expiry, and rate-limit-read failure.
3. Browser login start parameters, system-browser handoff, success notifications, cancellation, failure, timeout, and process exit.
4. Device-code start fields, success, failure, cancellation, and immediate fallback after browser failure.
5. Logout success and remote failure, including local child/analysis/evidence cleanup and visible UI state.
6. Usage-limited analysis, preview, Create/Import/Update/Repair planning, plus signed-out/limited recovery availability.
7. `turn/start` thread correlation, exact read-only sandbox, no roots, no model override, never-approve policy, and exact output schema.
8. Output acceptance/rejection matrix, required proposal sets, duplicate keys, invalid identifiers/tags/folders, evidence binding, and credential-shaped output.
9. Exact input-preview parity, redaction, absolute-root omission, input/output digest binding, and bounded evidence.
10. Confirmation replay, altered fields, stale session, post-logout record, preview gate, plan gate, and update reanalysis binding.
11. Serialization sweeps over state, plan, lock, journal, error strings, and UI diagnostics for forbidden token/account fields.
12. Fresh signed-out UI recovery, rollback, journal inspection, staging discard, backup inspection, and managed removal.

External release gates that do not require real credentials or network:

- Packaged Windows and macOS runs against a deterministic fake App Server child.
- Exact supported Codex CLI startup-argument compatibility is verified against the declared minimum/packaged test version.
- Native system-browser behavior is verified with a non-sensitive fake URL.
- Forced child termination and desktop exit leave no supervised App Server child running.

## Bounded recommended parent sequence

1. Fix the planning guard so usage-limited is checked before authenticated success; preserve signed-out recovery routes.
2. Add an explicit fresh-launch Recovery/Remove entry and decouple local removal from any required semantic record.
3. Split model-visible analysis input from core-only root/scan bindings; omit absolute roots from prompts and persisted records, and make the UI preview exact.
4. Define and enforce mode-specific required proposal keys, then bind confirmation to the complete validated/editable result.
5. Replace the blocking login wait with a cancellable state machine that permits immediate device fallback and logout.
6. Preserve typed command errors through the TypeScript bridge; expose usage, cancellation, interruption, schema, and remote-logout-failure states distinctly.
7. Harden output validation/redaction, account parsing, URL validation, turn correlation, line framing, and explicit application-exit shutdown.
8. Add the route-level fake App Server, UI, serialization, and signed-out recovery tests listed above, then have the parent rerun the repository-owned test commands and packaged platform gates.

The parent should treat the Codex integration completion gate as open until these failures are resolved and the missing interruption/failure tests are demonstrated.
