# Codex integration full re-audit — 2026-07-29

## Audit status and scope

This is a read-only audit handoff for commit
`bcfe329dd9ab0ae0d86e48b1a46ed21c83e36603`. It is not a completion claim.
The parent owns implementation, final review, and release-gate decisions.

The audit covered the requested App Server, authentication, semantic-analysis,
provider, credential, process, bridge, schema, and current test files. No
application source or schema was changed. Test suites were not executed because
the audit was constrained to writing only this handoff; findings below are
static evidence from current HEAD. A read-only `codex app-server --help` check
confirmed that the installed CLI accepts the current `--stdio` argument.
Unrelated source edits and additional handoffs appeared in the shared worktree
after evidence collection; they were not inspected, modified, or included in
this commit-pinned assessment.

The current official protocol baseline used for wire-shape comparison is:

- [App Server initialization](https://learn.chatgpt.com/docs/app-server#initialization)
- [App Server auth endpoints](https://learn.chatgpt.com/docs/app-server#auth-endpoints)
- [Start a turn](https://learn.chatgpt.com/docs/app-server#start-a-turn)
- [Turn events](https://learn.chatgpt.com/docs/app-server#turn-events)
- [Restricted read-only sandbox access](https://learn.chatgpt.com/docs/app-server#sandbox-read-access-readonlyaccess)

## Executive assessment

The current integration is not ready to satisfy the Codex authentication and
analysis acceptance gate. Two independent critical defects prevent the normal
semantic route from working through the packaged UI:

1. the account parser does not recognize the current documented ChatGPT
   `account/read` response; and
2. both analysis bridge wrappers pass the Rust command argument at the wrong
   nesting level.

Even after those are corrected, the App Server turn-event state machine can
lose the active turn ID and stop on an intermediate `item/completed`. The core
also cannot prove that the user approved the transmitted input subset or
confirmed the exact values later rendered. Existing containment is materially
better: the Codex turn uses an empty-root restricted read-only sandbox,
`approvalPolicy = never`, no model override, bounded JSONL, no Codex API-key
route, and no observed ChatGPT token persistence.

## Severity-ordered findings

### Critical C-01 — Current ChatGPT accounts are parsed as signed out

**Impact:** A valid managed ChatGPT session is rejected. Browser login and
device-code login can receive both success notifications and still return
`authenticated = false`. The rate-limit request is consequently skipped, and
Create, Import, Update, Repair, preview, planning, and readiness cannot pass for
Codex.

**Evidence:**

- Current official `account/read` uses a non-null account object with
  `type = "chatgpt"` and optional display metadata; it does not document
  `authenticated`, `isLoggedIn`, or `loggedIn`.
- `parse_account_status` derives authentication only from those three
  undocumented booleans and defaults to false:
  `src-tauri/src/codex.rs:1030-1058`.
- `account_read` requests rate limits only after that false boolean gate:
  `src-tauri/src/codex.rs:242-256`.
- `wait_for_login_with_cancel` re-reads the same account state after both
  notifications, so successful browser/device flows still fail:
  `src-tauri/src/codex.rs:285-334`.
- Planning requires `authenticated && auth_mode == "chatgpt"`:
  `src-tauri/src/commands.rs:341-386`.
- Tests encode a non-protocol fixture containing `authenticated: true`:
  `src-tauri/src/codex.rs:1705-1713`,
  `src-tauri/src/codex.rs:1729-1751`,
  `src-tauri/src/codex.rs:1950-1980`, and
  `src-tauri/src/codex.rs:2037-2069`.

**Missing tests:** Exact current wire fixtures for signed out, API-key account,
and ChatGPT account without an authentication boolean; browser and device-code
completion followed by the exact current account shape; rate-limit read after
that shape; expired-session transition.

**Acceptance impact:** ANA-02, ANA-03, ANA-04, ANA-17, and RDY-06 fail for the
normal Codex route.

### Critical C-02 — Renderer analysis calls do not match the Rust Tauri argument contract

**Impact:** Both Codex and non-Codex semantic analysis can fail command
deserialization before reaching the Rust validators. This blocks the provider
proposal prerequisite from the packaged UI independently of C-01.

**Evidence:**

- Rust commands each have one named struct argument, `request`:
  `src-tauri/src/commands.rs:661-692` and
  `src-tauri/src/commands.rs:696-744`.
- The bridge command map declares the argument as the request body itself:
  `src/lib/tauri.ts:47-61`.
- `invokeCommandResult` forwards its `args` object unchanged:
  `src/lib/tauri.ts:109-129`.
- `runCodexAnalysis` and `runAiAnalysis` pass the request body directly rather
  than `{ request }`: `src/lib/tauri.ts:198-204`.
- Other one-argument struct commands correctly use the named wrapper, for
  example `{ state: ... }`: `src/lib/tauri.ts:326-335`.
- `src/lib/tauri.test.ts:21-78` has no assertion for either analysis wrapper.

**Missing tests:** Bridge contract tests asserting the exact invocation payload
for `codex_analyze` and `ai_analyze`, plus one packaged-command smoke test that
deserializes the real Rust argument.

**Acceptance impact:** ANA-09, ANA-13, ANA-17, AI-01, AI-02, and UI-13 are not
reachable through the current bridge.

### High H-01 — Turn correlation and completion do not follow current App Server event shapes

**Impact:** Once account and bridge defects are fixed, a semantic turn can be
reported as missing schema output, can stop on the first intermediate completed
item, or can be correlated only by thread instead of the exact turn. Failed and
interrupted terminal states are not explicitly distinguished.

**Evidence:**

- Current `turn/start` returns the ID as `turn.id`; current `turn/started` and
  `turn/completed` documentation describes `{ turn }`.
- The implementation extracts only keys named `turnId` or `turn_id` from the
  start response: `src-tauri/src/codex.rs:420-430` and
  `src-tauri/src/codex.rs:506-523`.
- `event_matches_turn` requires a `threadId` and never recognizes `turn.id`:
  `src-tauri/src/codex.rs:546-559`.
- With or without a turn ID, `drain_notifications` treats any correlated method
  ending in `/completed` as terminal. That includes `item/completed`, not only
  `turn/completed`: `src-tauri/src/codex.rs:466-501`.
- The analysis then searches only the prematurely collected messages:
  `src-tauri/src/codex.rs:431-441`.
- `app_server_item_notifications_can_carry_schema_output` tests extraction from
  an uncorrelated item only: `src-tauri/src/codex.rs:1921-1932`.
- `unrelated_turn_notifications_do_not_match_the_active_turn` invents
  top-level `threadId`/`turnId` fields for `turn/completed` rather than using the
  documented `{ turn }` shape: `src-tauri/src/codex.rs:1935-1946`.
- The malformed-output test does not exercise a documented start response or
  notification sequence: `src-tauri/src/codex.rs:2037-2069`.

**Missing tests:** Exact generated-wire fixtures for `turn/start`,
`turn/started`, input-item completion, agent-item completion, `turn/completed`,
failed, interrupted, missing output, and unrelated concurrent thread events.
There is also no semantic-turn cancellation/`turn/interrupt` path or test.

**Acceptance impact:** ANA-13 and ANA-17 fail as an end-to-end protocol
guarantee.

### High H-02 — Core state does not prove input approval or bind confirmation to rendered values

**Impact:** A renderer bug or bypass can transmit any current scan-known
evidence without first invoking the approval command. After confirmation, it
can replace an edited field value while reusing the same confirmed record.
Preview and plan gates prove that a proposal-key set was confirmed, but not that
the exact transmitted inputs or final rendered values were confirmed.

**Evidence:**

- A completed scan automatically populates `CODEX_APPROVED_EVIDENCE` with all
  scan-known references before user approval:
  `src-tauri/src/commands.rs:953-1025`.
- `approve_scan_evidence` validates a supplied subset but records no approved
  subset, approval nonce, or digest: `src-tauri/src/commands.rs:748-791`.
- `validate_codex_evidence_approval` checks the analysis request against the
  full scan-populated store, so callers can skip `approve_scan_evidence`:
  `src-tauri/src/commands.rs:794-846`.
- `confirm_codex_analysis` accepts only a record and proposal-key names:
  `src-tauri/src/commands.rs:859-882`.
- `confirm_analysis_record` binds the analysis digest and key set, not the
  user-edited final values: `src-tauri/src/codex.rs:1593-1643`.
- Descriptor preview checks for a confirmed record, then reads identity values
  separately from renderer state:
  `src-tauri/src/commands.rs:1526-1533`,
  `src-tauri/src/commands.rs:1986-2040`, and
  `src-tauri/src/commands.rs:2262-2322`.
- New-project records are exempt from project-root session binding:
  `src-tauri/src/commands.rs:2334-2365`.

**Missing tests:** Skipping the approval command must be rejected; approving a
subset must prohibit the rest of the scan; changing approved bytes must
invalidate the digest; changing any final value after confirmation must block
preview and planning; switching a new-project root must invalidate the pending
confirmation.

**Acceptance impact:** ANA-10 and UI-13 fail at the authoritative boundary. The
requirements for user confirmation before rendering and exact root/session
binding also fail.

### High H-03 — Secret-bearing evidence filenames and reference text are not comprehensively excluded

**Impact:** A scan-known path such as a credential-shaped JSON file, key-named
configuration file, or other secret-bearing filename can pass to the provider.
Excerpt redaction reduces content risk but is not a substitute for excluding
the file. The model-visible input includes both evidence paths and reference
strings.

**Evidence:**

- Input includes the evidence objects unchanged:
  `src-tauri/src/codex.rs:710-724` and
  `src-tauri/src/codex.rs:764-772`.
- Request validation redacts the brief, constraints, and excerpt content, but
  does not apply credential-shape redaction to `reference` or `path`:
  `src-tauri/src/codex.rs:611-665`.
- `forbidden_evidence_path` blocks only exact path segments such as `auth`,
  `credential`, `secret`, and `token`, plus a small extension set:
  `src-tauri/src/codex.rs:675-697`.
- Common secret-bearing basename variants with suffixes or extensions are not
  covered by that exact-segment logic.
- The only path test is traversal plus lowercase-hash behavior:
  `src-tauri/src/codex.rs:1901-1918`.

**Missing tests:** Credential/token/key basename variants, common vault/config
filenames, dotfiles, case variants, secret-shaped reference IDs, approved
binary paths, and redaction-detector false negatives.

**Acceptance impact:** ANA-11 fails.

### High H-04 — Proposed paths, profiles, tags, and component identifiers are not fully deterministic

**Impact:** Schema-valid model output can contain a normalized but reserved
folder such as `.git`; the final generator can then plan a managed marker below
that reserved root. Profile-like values and component recommendation IDs are
accepted as arbitrary non-empty strings rather than checked against core-owned
registries.

**Evidence:**

- The analysis schema leaves `proposal.value` unconstrained and gives
  `component_id` only `type: string`:
  `docs/schemas/codex-analysis.schema.json:47-71` and
  `docs/schemas/codex-analysis.schema.json:84-124`.
- `AgentsProfile`, `LocalisationConvention`, and
  `DocumentationConvention` receive only generic string checks:
  `src-tauri/src/codex.rs:930-961`.
- Descriptor tags receive length/content checks but no core-owned allowed-value
  validation: `src-tauri/src/codex.rs:978-997`.
- Folder proposals are normalized and deduplicated but reserved roots are not
  rejected: `src-tauri/src/codex.rs:998-1017`.
- Final folder state repeats normalization without a reserved-root policy:
  `src-tauri/src/commands.rs:2120-2159`.
- The generated destination is `{folder}/.gitkeep`:
  `src-tauri/src/commands.rs:2611-2619`.
- Component recommendation IDs are checked only for non-empty text:
  `src-tauri/src/codex.rs:862-884`.

**Missing tests:** Reserved transaction, Git, Codex, and application metadata
roots; nested reserved roots; platform path aliases; supported tag/profile
registries; manifest-known component IDs; duplicates and case-folded
collisions.

**Acceptance impact:** ANA-16 fails. ANA-14 remains fail-closed for direct
provider authority, but this gap lets an accepted proposal influence a later
deterministic write to an unsafe reserved location.

### High H-05 — Non-Codex adapters do not send the current output schema

**Impact:** The non-Codex provider is told to match a “supplied” schema that is
never actually supplied. Rust post-validation safely rejects malformed output,
but the alternate-provider happy path is not a schema-constrained request and
is unlikely to be interoperable across the advertised profiles.

**Evidence:**

- `ai::analyze` computes the prompt and post-validates the extracted response:
  `src-tauri/src/ai.rs:242-310`.
- `request_provider` sends only model, system text, and messages. It sends
  neither the JSON Schema nor a provider-native structured-output parameter:
  `src-tauri/src/ai.rs:313-400`.
- The prompt says “matching the supplied output schema” but contains only the
  approved input and digest:
  `src-tauri/src/codex.rs:764-772`.
- By contrast, the Codex turn includes the exact schema as `outputSchema`:
  `src-tauri/src/codex.rs:396-419`.
- Provider tests cover profiles, endpoints, and response envelope extraction
  only: `src-tauri/src/ai.rs:441-539`.

**Missing tests:** Fake HTTP adapters that inspect the outbound request and
require the exact current schema; native Anthropic and OpenAI-compatible
structured-output envelopes; unsupported-schema behavior; malformed, extra,
oversize, redirect, timeout, and interruption paths while preserving the
draft.

**Acceptance impact:** The shared-schema contract in
`docs/30_codex_chatgpt_authentication.md:134-139` and
`docs/31_ai_provider_profiles_and_chat_sources.md:34-46` fails. ANA-13 has
authoritative post-validation but not transport-level schema enforcement for
non-Codex profiles.

### High H-06 — Login cancellation has a reset race and does not use the managed cancel method

**Impact:** An immediate retry or device-code fallback can clear the global
cancellation flag before the old wait observes it. The old wait retains the
single session mutex and can continue for the full bounded wait while the new
login start blocks. The App Server's pending login is not explicitly cancelled
by login ID.

**Evidence:**

- `with_codex_session` holds the session mutex for the whole callback:
  `src-tauri/src/commands.rs:295-329`.
- `codex_login_start` clears the global cancellation flag before attempting to
  acquire that session: `src-tauri/src/commands.rs:576-597`.
- `codex_login_wait` polls the same global flag and resets it after return:
  `src-tauri/src/commands.rs:600-608`.
- `codex_login_cancel` only sets the flag:
  `src-tauri/src/commands.rs:611-615`.
- The current official protocol provides `account/login/cancel` with the
  `loginId`; no implementation symbol sends that method.
- The command test explicitly proves only the in-memory flag:
  `src-tauri/src/commands.rs:4647-4656`.

**Missing tests:** Concurrent cancel then immediate browser retry; cancel then
device fallback; exact `account/login/cancel` request; server cancellation
notification; cancel while the child exits; logout during a pending wait;
preservation of wizard input throughout.

**Acceptance impact:** ANA-04 and ANA-17 fail for interruption handling.

### Medium M-01 — Persisted analysis schemas do not require all identity and containment fields

**Impact:** Schema validation alone accepts analysis metadata that omits
provider/profile binding or the explicit no-account-persistence assertion.
Rust runtime validation closes several of these gaps, but schema-backed plans
and locks are not independently authoritative as required.

**Evidence:**

- The plan schema requires `engine`, `auth_mode`, IDs, hashes, fields, and time,
  but does not require `provider`, `model`, `optimization_profile`, or
  `account_identity_persisted`:
  `docs/schemas/installation-plan.schema.json:308-383`.
- The lock schema requires `account_identity_persisted`, but not `auth_mode`,
  `provider`, `model`, or `optimization_profile`:
  `docs/schemas/installation-lock.schema.json:289-361`.
- The Rust model defaults the omitted fields:
  `src-tauri/src/models.rs:1104-1134`.
- `validate_confirmed_record` is stricter at runtime:
  `src-tauri/src/codex.rs:1515-1587`.

**Missing tests:** Schema-negative fixtures for every omitted binding field,
cross-provider/profile records, missing no-account assertion, and plan-to-lock
round trips checked by both JSON Schema and Rust validation.

**Acceptance impact:** ANA-20 is not satisfied as a schema-level guarantee.
The exact allowed metadata list is also inconsistent across ANA-20,
`docs/30_codex_chatgpt_authentication.md:139-150`, and the current schemas; the
parent should resolve that contract before changing fields.

### Medium M-02 — Codex does not override the turn model, but persists a synthetic model named “default”

**Impact:** The App Server correctly remains in control of model selection, but
plans, locks, state, and generated documentation can claim a concrete model
label that was neither selected nor observed. This weakens provenance and can
be mistaken for a hardcoded Codex model configuration.

**Evidence:**

- The Codex `thread/start` and `turn/start` requests omit a model override:
  `src-tauri/src/codex.rs:396-419`.
- Missing model state becomes the literal `default`:
  `src-tauri/src/commands.rs:2183-2189` and
  `src-tauri/src/models.rs:390-392`.
- That value is persisted in the plan and carried into later locks:
  `src-tauri/src/commands.rs:2395-2402`,
  `src-tauri/src/commands.rs:2871-2876`, and
  `src-tauri/src/commands.rs:4068-4073`.
- Generated provider documentation includes a model line:
  `src-tauri/src/commands.rs:1931-1935` and
  `src-tauri/src/commands.rs:2238-2259`.
- Tests encode `default` as the Codex model:
  `src-tauri/src/commands.rs:4572-4576` and
  `src/App.test.tsx:28-39`.

**Missing tests:** Codex plan/lock/README fixtures proving no model identity is
claimed when App Server owns selection; non-Codex fixtures proving the explicit
user-selected model remains bound.

**Acceptance impact:** ANA-19 fails for persisted and displayed provenance,
although the actual App Server turn does not hardcode a model override.

### Low L-01 — Transient account metadata is unnecessarily returned to the core in state-bearing calls

**Impact:** No persistence leak was found in the scoped plan/lock generation,
but a full renderer `WizardState` can carry transient account display metadata
back across IPC for descriptor preview and plan construction. This expands the
crash/debug exposure surface without being needed by those commands.

**Evidence:**

- `stateForCore` strips only the Meshy password draft:
  `src/lib/tauri.ts:326-341`.
- The state-bearing bridge test verifies only that Meshy field:
  `src/lib/tauri.test.ts:67-76`.
- Generated project state stores only a coarse signed-in/configured status and
  `account_values_persisted: false`:
  `src-tauri/src/commands.rs:2671-2708`.
- `CodexAnalysisRecord` deliberately skips project root and scan ID:
  `src-tauri/src/models.rs:1124-1134`; its serialization test excludes account,
  token, root, and scan fields:
  `src-tauri/src/codex.rs:2072-2112`.

**Missing tests:** Strip transient account status and pending login display data
from every state-bearing core call; serialize real plans, locks, recovery
records, error reports, and generated state and assert account/token field
absence.

**Acceptance impact:** ANA-08 is not shown to fail persistence, but the current
bridge does not meet a strict least-data boundary.

## Controls that held in the scoped evidence

- **Startup, framing, shutdown, restart implementation:** The process uses an
  absolute reviewed executable, argument array, cleared environment, piped
  stdio, bounded 2 MiB JSONL, liveness polling, and process-tree termination:
  `src-tauri/src/codex.rs:1255-1491`. Dead sessions are replaced and initialized
  before use: `src-tauri/src/commands.rs:295-329`.
- **Initialize order:** Production session creation calls `initialize` before
  account or thread work: `src-tauri/src/commands.rs:310-316`.
  `initialize` then sends `initialized`:
  `src-tauri/src/codex.rs:227-239`.
- **Browser/device requests:** Only `chatgpt` and `chatgptDeviceCode` are emitted:
  `src-tauri/src/codex.rs:260-271`.
- **No Codex API-key fallback:** Codex has no credential requirement,
  credential store/remove rejects Codex, and `ai_analyze` rejects the Codex
  provider: `src-tauri/src/ai.rs:35-43`,
  `src-tauri/src/commands.rs:534-564`, and
  `src-tauri/src/commands.rs:696-700`.
- **Logout containment:** `account/logout` is used; the local process, pending
  analyses, and scan evidence are cleared even if the remote request fails:
  `src-tauri/src/codex.rs:337-340` and
  `src-tauri/src/commands.rs:638-657`.
- **Read-only analysis:** Both thread and turn use `approvalPolicy = never` and
  a restricted read-only sandbox with no readable roots and no writable roots:
  `src-tauri/src/codex.rs:396-419` and
  `src-tauri/src/codex.rs:699-708`. Project root and scan ID are omitted from
  model-visible input: `src-tauri/src/codex.rs:710-724`.
- **Codex output schema and post-validation:** The exact checked-in schema is
  passed as `outputSchema`, and Rust rejects extra fields, wrong mode/hash,
  incomplete key sets, unknown evidence references, malformed values, and
  account/credential-shaped proposal values:
  `src-tauri/src/codex.rs:407-419` and
  `src-tauri/src/codex.rs:780-1020`.
- **Provider endpoint and credential containment:** Hosted endpoints require
  explicit HTTPS; local endpoints are loopback HTTP; redirects and response
  sizes are bounded; provider credentials are vault-scoped:
  `src-tauri/src/ai.rs:108-197`,
  `src-tauri/src/ai.rs:313-400`, and
  `src-tauri/src/credentials.rs:226-319`.
- **No provider apply/readiness authority:** Analysis returns proposals and a
  record only. Installation approval, conflict resolution, apply, and
  readiness remain separate Rust commands. No scoped provider adapter writes
  project files.
- **Signed-out recovery paths:** Rollback, journal read, interrupted-transaction
  discovery, resume, and staging discard have no AI authentication gate:
  `src-tauri/src/commands.rs:4336-4435`. Managed removal explicitly bypasses
  the planning-auth gate: `src-tauri/src/commands.rs:3419-3448`.

These are implementation controls, not end-to-end proof. Startup/restart,
logout failure, exact current wire events, and signed-out recovery lack the
integration coverage listed below.

## Data-leak risks

1. **High:** H-03 permits secret-bearing filenames/reference text into the
   model-visible manifest despite excerpt redaction.
2. **Medium:** `component_recommendation.component_id` is neither
   credential-shape checked nor validated against a known component registry;
   only its reason text is redacted:
   `src-tauri/src/codex.rs:862-884`.
3. **Low:** L-01 resends transient account display state to unrelated
   state-bearing core commands.
4. **Contained in current evidence:** No code path requests ChatGPT tokens,
   uses external token mode, stores a Codex API key, or serializes account
   display values into the analysis record. Provider API keys are read from the
   OS vault only for the selected non-Codex request.

No credentials, full account addresses, tokens, or private project excerpts
are reproduced in this handoff.

## Protocol and state-machine risks

- Valid ChatGPT account state cannot cross the current parser (C-01).
- Both renderer analysis calls can fail before Rust command entry (C-02).
- Turn ID extraction and terminal-event handling do not match the documented
  wire shape (H-01).
- Login cancellation can be reset by a retry while the old wait owns the
  session mutex (H-06).
- There is no semantic-turn interrupt command; an interrupted/failed turn is
  not represented as a distinct resumable state.
- Restart is implemented but untested. A child exit during initialize,
  account read, login notification wait, or turn drain has no fake-process
  supervisor integration fixture.
- Logout remote failure behavior is documented and implemented, but the only
  command test covers the no-remote-session case:
  `src-tauri/src/commands.rs:5357-5410`.
- Account/session expiry is rechecked before analysis, preview, and planning in
  principle, but no exact expired-session transition test exists.

## Missing failure and interruption tests

The following are release-gate gaps, grouped to keep the parent fix bounded:

1. **Exact protocol fixtures**
   - initialize once per transport; pre-initialize request rejection; repeated
     initialize rejection;
   - current `account/read` signed-out, API-key, ChatGPT, and expired-session
     shapes;
   - browser and device-code start, completion, cancellation, failure, timeout,
     and process exit;
   - current `turn/start`/item/turn notification sequence, final structured
     output, failed, interrupted, timeout, oversized JSONL, and unrelated
     threads.
2. **Process supervision**
   - child exits before and after initialize;
   - child exits during account read, login wait, and turn drain;
   - one clean restart with a new initialize handshake;
   - shutdown kills descendants and does not preserve pending analysis from an
     interrupted turn.
3. **Bridge contracts**
   - exact `{ request }` argument shape for both analysis commands;
   - packaged Tauri command deserialization smoke tests;
   - strip account/login transient state from state-bearing calls.
4. **Approval and confirmation**
   - bypassed approval, approved subset, stale scan, changed excerpt, changed
     root, changed endpoint/model/profile;
   - edited final values bound to the confirmation digest;
   - post-confirmation mutation rejected before preview and plan.
5. **Schema and deterministic validation**
   - every proposal type, required key, duplicate, unknown field, sensitive
     value, invalid profile/tag/component ID, reserved path, case collision,
     traversal, and platform path alias;
   - plan/lock negative schema fixtures for missing provider/auth/profile and
     no-account fields;
   - fuzz corpus assertions, not only a parser entry point:
     `fuzz/fuzz_targets/codex_analysis.rs:1-12`.
6. **Non-Codex adapters**
   - outbound exact schema, provider-native structured output, scoped
     credential header, no redirect, no response-body disclosure on error,
     bounded response, timeout, disconnect, and malformed envelope.
7. **Signed-out recovery**
   - rollback, journal/backup inspection, resume, staging discard, and managed
     removal while Codex is missing, signed out, usage-limited, expired, and
     disconnected.
8. **UI failure states**
   - full browser success and failure-to-device-fallback;
   - full device-code success/cancel/timeout;
   - input manifest preview and explicit approval;
   - detected/suggested/confirmed labels and exact edited-value confirmation;
   - draft preservation after auth, usage, process, and schema failures.

## Acceptance criteria that fail or remain partial

| Criterion | Audit result | Primary reason |
| --- | --- | --- |
| ANA-02 | Fail | Valid Codex account cannot pass C-01; semantic bridge also fails C-02. |
| ANA-03 | Fail | Managed ChatGPT request exists, but the documented account response is rejected. |
| ANA-04 | Fail | Browser/device requests exist, but completion and cancellation are not reliable. |
| ANA-05 | Static pass; UI test gap | Core has no Codex API-key route; no explicit Codex UI regression test asserts field absence. |
| ANA-06 / ANA-07 | Static pass | No token inspection, copying, external-token mode, or Codex token persistence found. |
| ANA-08 | Partial | No project/lock persistence found; unnecessary transient account-state retransmission remains. |
| ANA-09 | Fail | Analysis commands are unreachable through the current bridge. |
| ANA-10 | Fail | Approval command does not establish authoritative approved-subset state. |
| ANA-11 | Fail | Secret-bearing path/reference exclusions are incomplete. |
| ANA-12 | Static pass; integration test gap | Empty-root restricted read-only sandbox and no writable roots are set. |
| ANA-13 | Partial | Codex sends the schema and Rust validates; turn state is faulty and non-Codex requests omit the schema. |
| ANA-14 | Static pass | No provider write/apply/conflict/readiness command authority found. |
| ANA-16 | Fail | Reserved paths and core-owned profile/tag/component registries are not fully enforced. |
| ANA-17 | Fail | Account, bridge, turn, and cancellation interruption paths are not reliable or fully tested. |
| ANA-18 | Static pass; test gap | Recovery and removal commands are not auth-gated; backup-inspection E2E proof is absent. |
| ANA-19 | Fail | Turn has no model override, but synthetic `default` is persisted and displayed. |
| ANA-20 | Fail/contract drift | Plan/lock schemas do not require all bindings; allowed metadata lists disagree. |
| ANA-21 | Static pass; HTTP test gap | Endpoint, provider-scoped vault, no-redirect, and response bounds are implemented. |
| AI-01 / AI-02 | Partial | Registry and binding exist, but non-Codex bridge/schema request defects block proof of functional routes. |
| AI-03 | Static pass; transition test gap | Provider/model/profile/endpoint checks reject stale records, but UI clearing is not proven here. |
| RDY-06 | Fail for valid Codex users | Readiness correctly blocks, but C-01 prevents valid authentication from satisfying it. |
| UI-13 | Fail at core boundary/test gap | Exact input approval and exact edited-value confirmation are not authoritative or tested. |

## Bounded recommended fix order

1. **Restore basic reachability.** Parse the documented account object as the
   authentication fact, retain the ChatGPT type gate, then fix both Tauri
   wrappers to pass `{ request }`. Add exact wire and bridge tests before other
   refactoring.
2. **Replace the turn state machine with exact generated protocol shapes.**
   Read `turn.id`, correlate exact thread/turn fields, collect agent structured
   output, terminate only on the active `turn/completed`, and surface failed or
   interrupted status. Add `turn/interrupt` if analysis cancellation is a
   required UI action.
3. **Make approval and final confirmation core-owned.** Store the explicitly
   approved input manifest/digest and require it in analysis. Confirm a
   core-validated final-value object/digest, not only proposal-key names, and
   bind new-project confirmation to the selected root/session.
4. **Close data and path policy gaps.** Centralize secret-bearing path
   rejection, reserved-root rejection, supported profile/tag registries, and
   manifest component-ID validation. Apply the same policy to proposal output
   and final renderer state.
5. **Supply the exact schema to every provider adapter.** Prefer
   provider-native structured output where verified; otherwise include the
   exact bounded schema in the request while retaining authoritative Rust
   validation.
6. **Make login cancellation attempt-specific.** Track the active login ID and
   generation, call `account/login/cancel`, prevent retries from clearing an old
   wait's cancellation state, and test logout/cancel/process-exit races.
7. **Resolve persistence provenance.** Align ANA-20, docs, Rust models, and both
   schemas; require all accepted provider/profile/no-account fields. Represent
   Codex model selection as no override rather than a synthetic model name.
8. **Run the focused release gate.** Execute Rust unit/integration tests,
   renderer bridge/UI tests, schema-negative fixtures, the Codex analysis fuzz
   target with assertions/corpus, and Windows/macOS fake-process interruption
   scenarios. Re-run this auditor only after the parent has reviewed those
   results.

The parent should treat C-01, C-02, H-01, H-02, H-03, H-04, H-05, and H-06 as
blocking the Codex integration completion gate.
