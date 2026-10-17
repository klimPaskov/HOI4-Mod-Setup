---
name: hoi4-mod-setup-codex-integration
description: Use when implementing or changing provider authentication, model profiles, Codex App Server lifecycle, structured semantic analysis, usage-limit handling, or the boundary between AI proposals and deterministic project generation.
---

# HOI4 Mod Setup Codex Integration

Use this skill for the provider-neutral semantic layer of HOI4 Mod Setup. The user selects a setup assistant, model, and model-supported reasoning effort at the start; the Claude account route (`claude_account`, Claude Haiku 5.5 at medium effort) is the default profile and Codex remains first-class. This choice is never the later Agentic HOI4 Modding development-client selection.

## Product rule

Create, Import, Update, and Repair planning require an authenticated, selected AI capability. Codex uses the official local `codex app-server` interface and ChatGPT-managed sign-in. Known hosted providers use checked-in, officially verified model/address defaults plus a provider-keyed OS-vault credential; model and address are editable under Advanced. Local and custom routes require explicit user configuration. Do not silently switch providers or invent a provider route or OAuth flow.

Dispatch account reads, login start/wait, logout, descriptor preview capability
checks, and analysis through the desktop blocking-work boundary and bounded
user-facing error mapping. Do not hold the UI event loop during App Server or
hosted-provider I/O.
Use that closed error map for model catalogs and Create/Import/Update/Repair
capability checks as well; never send raw provider protocol or serialization
detail to the renderer.

Analysis input preparation keeps typed failures through root/evidence/source
binding and applies the closed provider error map before returning through IPC.
Maintenance preparation retains `AppError` through the blocking builder and
maps it exactly once at the asynchronous command boundary, retaining redacted
technical diagnostics separately. Never wrap already sanitized strings as
transaction errors: sign-in, usage-limit, stale-review, source, and scan states
must keep their distinct next steps. Test real command failure paths and typed
I/O, account-gate, and reanalysis failures through that async dispatcher, not
only mapper helpers.
Bounded flatten reads must retain typed open/read errors rather than converting
OS text into renderer-visible `InvalidInput`; unsupported coding-environment
messages must not echo unvalidated input values.
Non-update rescans use their completed installed-state snapshot for the same
plan request without overwriting user choices or creating semantic authority;
Update retains its already confirmed scan-bound reanalysis.

Update reanalysis accepts an explicit bounded `update_source` selection. Its
component registry and core-owned record bind the selected target revision and
manifest hash, not the installed pin. Planning receives the same `updateSource`
and rejects changed source evidence before downloads. Omitted selections retain
legacy lock-policy behavior. UI source edits invalidate proposals, confirmation,
scan-review state, and plans; they never edit the installed lock directly.

Fetch Codex models through App Server `model/list` and hosted-provider models
through the authenticated official Models API. Bind model plus reasoning effort
to the analysis record, plan, and lock. The checked-in fallback defaults are
Claude Haiku 5.5/`medium` for both Claude routes, `gpt-5.6-luna`/`xhigh` for
Codex, `gemini-3.8-flash` for Gemini, `grok-4.7` for hosted Grok, and
`deepseek-flash` for DeepSeek. Claude Code's 14-entry catalog is built-in and
keeps legacy Haiku 4.5; a bounded exact Claude model ID may be entered under
Advanced. Persist these only as setup-analysis provenance; do not write them
into generated AGENTS/README guidance or use them to select development
components. Experimental account routes are separate: Gemini uses
`gemini-3.8-flash`, while Grok's account CLI uses `grok-build`, not the hosted
API model.

A model-catalog timeout, error, or empty result must not remove the model
control. Keep the checked-in verified default selectable for Codex and every
known hosted profile, then merge a successful live catalog into those options.
If a fresh, untouched Codex fallback is absent from the live catalog, select
the App Server's advertised default (or first advertised model) and its effort.
Retain explicit model choices and show a model-availability warning when one
disappears; a signed-in account must not be told to sign in for this condition.
Until live per-model metadata is available, expose only the fallback model's
verified default reasoning effort. Claude's built-in catalog is labelled
`built-in`, not live. An exact Claude model ID that is not in that catalog stays
usable after identifier validation, but no reasoning effort is assumed or
forwarded for it. Keep a concise visible distinction between a live result, an
empty result, and a failed refresh.
For Local and Custom, keep the model name editable and expose successfully read
endpoint models as suggestions. Never label a checked-in fallback as a live
result, and never invent a fallback model for Local or Custom.

Components and development-client integrations remain independent of the setup
assistant unless their manifest explicitly declares a real runtime provider
dependency. `workflow.super_events` is an ordinary
manifest component for every selected provider; its selection, recommendation,
file plan, and non-blocking readiness state must not create a Codex-only route.
The same rule applies to `mcp.hoi4_agent_tools` and `workflow.3d`: the app-owned
bootstrap and health paths remain usable for every provider, while only
`codex.config` is Codex-specific.

`Open in Codex` requires the installed Codex configuration and freshly
recomputed local readiness. It never reads the setup provider's live account,
App Server session, or credential-vault entry, and it must not trust a cached
lock hash after managed project files change.

The planning-ready gate is checked in both layers: the React start gate keeps
Create/Import/Update/Repair unavailable until the selected provider is
available, authenticated, and not usage-limited, while Rust rechecks the
capability before building a plan. Codex additionally requires
`auth_mode = chatgpt`.

The Description primary action enters one visible pending state while semantic
analysis runs, disables duplicate submission, and always resolves to either the
Identity screen or an announced actionable error. Provider unavailability is a
failure, never a successful navigation result, and the name and brief remain
editable after failure.

That pending state exposes one provider-neutral progress region. Its stage
changes only at real lifecycle boundaries: approved input preparation,
provider generation, and deterministic result checking. Since providers do
not report an exact total, the visible percentage and time remaining are
explicit elapsed-time estimates within those stages.

Recovery, rollback, backup inspection, and managed removal remain locally usable while signed out.

## Claude account route

`claude_account` runs the user's own installed, unmodified Claude Code through
`src-tauri/src/claude_code.rs`; the full boundary is in
`docs/31_ai_provider_profiles_and_chat_sources.md`.

- Discover `claude` from PATH, `~/.local/bin`, and (macOS) Homebrew prefixes,
  then require the Anthropic signature before any run. Never download, bundle,
  or patch it; link to `https://code.claude.com/docs/en/setup` when missing.
- Probe `claude --help` for every isolation flag in `REQUIRED_PRINT_FLAGS`.
  A missing flag is a "needs an update" state, never a weaker invocation.
- Sign-in is `claude auth login --claudeai` with closed stdin, cancellable,
  ten-minute timeout. The renderer never receives a URL, code, or token.
- Status reads only `loggedIn`, a bounded `authMethod`, and `apiProvider`.
  Only `authMethod = "claude.ai"` with the first-party provider is the Claude
  account route (`ClaudeSignInSummary::is_claude_plan`); Console/API-key and
  third-party sessions get distinct messages.
- Parse Claude output raw (`run_reviewed_tool(..., raw_stdout = true)`) and
  validate before any redaction; never redact a structured reply before
  parsing. `analyze_with_runner` is the injectable loop for tests.
- Pass Claude Code the analysis schema without `$schema`/`$id`
  (`claude_output_schema`): its validator rejects the 2020-12 declaration and
  the run exits with empty stdout. Empty stdout is a Process error.
- Retry (via `correctable_output_error`) only one completed response that fails
  deterministic serialization, type/limit, identifier/value, or path rules.
  Never retry private/account-shaped output, timeouts, truncation, unreadable
  envelopes, transport, sign-in, authentication, or usage-limit failures. For
  Codex, an incomplete turn is a timeout and a corrective turn requires the
  completed turn's ID.
- Analysis is `--print --output-format json --json-schema <schema> --model
  <model> --tools "" --strict-mcp-config --safe-mode --no-session-persistence
  --system-prompt <bounded>`, prompt on stdin, cwd a fresh empty temp
  directory. Read `structured_output`; map `is_error` results to sanitized
  sign-in, usage-limited, model, or generic categories without forwarding raw
  result text.
- Read `src/claude-models.json` for the 14 built-in choices. Forward Claude's
  `--effort` only when the selected ID is in that catalog and advertises the
  effort; hide the selector for one-effort entries and forward no effort for an
  unknown exact ID.
- Persist provider `claude_account`, engine/integration `claude_code_cli`, auth
  mode `claude_account`. A legacy record with no provider still means Codex.
- `claude_logout` clears pending proposals and approved scan evidence.

Claude Code exposes no remaining-usage reading; describe usage as coming from
the user's Claude plan and surface a usage-limit result only after it occurs.

## Experimental provider-account CLI routes

`gemini_account` and `grok_account` are separate experimental routes, not
hosted API profiles and not a release claim. Read the accepted boundary in
`docs/34_provider_account_cli_architecture.md` and the implementation in
`src-tauri/src/account_cli.rs` before changing them.

- Download only after an explicit **Sign in** action. Read the package URL,
  npm SRI, archive member, and entry digest from
  `src-tauri/resources/account-cli-pins.json`; verify every value before
  execution. Never run npm lifecycle scripts, a package manager, redirects, or
  a PATH shim.
- The official provider CLI owns OAuth/login, token storage, refresh, and
  logout. The app never reads, copies, logs, persists, or displays provider
  tokens, authorization codes, account identity, email, plan, usage, or rate
  limits. Provider-owned session persistence is allowed only inside the
  app-private provider data root for approved setup inputs and never enters the
  target project, plans, locks, logs, crash output, or source packages.
- Use a link-free app-private home and an empty workspace with no target path.
  Disable tools, extensions, MCP, skills, hooks, plugins, telemetry, web
  search, and write-capable behavior according to the provider policy. Gemini
  requires a publisher-verified Node runtime; Grok runs its pinned provider
  binary. Re-hash the runtime immediately before every spawn.
- Gemini uses the official **Sign in with Google** flow. Grok uses `grok login`
  and `grok logout`; its account worker uses `grok-build`, distinct from hosted
  Grok `grok-4.7`. Capability checks are bounded and non-identifying; they do
  not become an API-key route. Missing preparation, cancellation, timeout,
  unsupported model/platform, sign-in, or usage failure preserves the draft and
  blocks new semantic planning.
- The current interactive terminal route is verified only on Windows and
  Linux. Report macOS unavailable until a verified runtime and terminal route
  exists, even when a package pin is present. Account records use
  `provider_cli`/`provider_account` and remain provider-neutral in generated
  project guidance.

## Required contract

- persist the selected provider, model, and optimization profile in typed state; never infer them from a display label
- derive endpoint, protocol, credential environment, limits, and supported platform from a checked-in provider profile
- launch `codex app-server` as a child process for Codex; on Windows apply
  `CREATE_NO_WINDOW` so shortcut and Start-menu launches do not open a console
- use stdio JSONL transport
- complete the initialize handshake before other requests
- enforce initialization in the protocol object so account, login,
  cancellation, logout, thread, and turn requests fail before a successful
  initialize response plus `initialized` notification
- use `account/read` for current state
- use `account/login/start` with `type: chatgpt` for the browser flow
- use `chatgptDeviceCode` only as the fallback flow
- open returned HTTPS login and device-code URLs through the typed fixed-path system-browser command
- wait for login and account update notifications
- bound and redact login-failure notification text before it can reach the UI;
  replace account-shaped failure detail with a generic login error
- use `account/logout` for sign-out
- bind every pending login to the App Server `loginId`; cancellation must target
  only that attempt with `account/login/cancel` and then interrupt its local
  wait, without resetting or cancelling another attempt
- route reviewed login and external links through typed system-browser commands;
  never let the renderer invoke an opener or navigate an arbitrary URL
- never read or copy Codex token storage
- never serialize account identity, tokens, plan details, rate limits, or usage into a mod project or installation lock
- do not use experimental externally managed ChatGPT tokens
- store non-Codex provider keys only in Windows Credential Manager, macOS Keychain or Linux Secret Service under a provider-keyed opaque reference; bind the non-secret reference scope to the selected provider and reject unscoped or cross-provider reuse
- inject a non-Codex key only as the adapter's approved environment/header input for the bounded request; never put it in project state, plan, lock, or logs
- treat local-model access as an explicit loopback route and do not claim hosted authentication for it
- use native structured-output request formats for Anthropic, Gemini, and Grok;
  API requests contain no tools, provider output is bounded, and redirects are
  disabled

## Analysis boundary

The selected provider owns semantic proposals. Deterministic Rust owns facts, validation, rendering, transactions, and readiness. The common `codex-analysis` schema is the boundary even when the selected provider is not Codex.

Use the selected provider for project description interpretation, display name, project ID, script prefix, namespace, descriptor tags, folder profile, `AGENTS.md` adaptation, component recommendations, existing-project purpose, and conflict explanation. The optimization profile changes the prompt and presentation convention, not deterministic safety rules.

Provider-visible summaries, reasons, and warnings are user-facing mod guidance.
Reject internal planning language about schemas, constraints, evidence fields,
operating systems, platforms, and Workshop ID rules. Warnings exist only for
an action or decision the user must take. Descriptor-tag proposals are limited
to the deterministic official HOI4 Workshop-category allowlist.

Use the scanner and validators for paths, hashes, descriptors, PNGs, encodings, Git state, identifier syntax, collisions, manifest checks, and file ownership.

Every analysis turn must:

- use a dedicated setup thread
- use read-only sandboxing
- expose no writable project root
- contain only user-approved inputs
- exclude secrets, binaries, Git objects, and credential stores
- require the current `codex-analysis` output schema for every adapter; Codex
  sends it as `outputSchema`, Anthropic/Gemini/Grok use their native schema
  fields, and hosted/local/account adapters receive the exact checked-in schema
  in their bounded request or CLI policy
- reject additional or malformed fields
- validate the complete Draft 2020-12 schema at runtime before typed
  deserialization. The core replaces the model-supplied `analysis_id` with a
  UUID after validation; a short non-secret model label is sufficient. Accept
  empty `evidence_refs` for genuinely inferred brief-only suggestions, but
  reject unknown references and preserve the current scan/hash/session approval
  binding. Validate component recommendations as
  `{component_id,recommendation,reason}` against the supplied deterministic
  registry, including `workflow.super_events`; unknown IDs or fields are
  rejected, not silently dropped.
- reject account-shaped text in summaries, proposal reasons, recommendation
  reasons, warnings, and proposal values
- require the complete ten-key proposal set before confirmation or planning
- return concise proposal reasons, not hidden reasoning
- discard private/account-shaped, truncated, refused, unreadable, transport,
  authentication, and usage-limit responses without a corrective retry; map
  each to a bounded actionable category

For existing-project analysis, the core accepts only normalized redacted
finding/conflict summaries, evidence references, and matching hashes emitted by a completed deterministic read-only scan in the
current app session and an explicit approval of that exact evidence vector.
Completion of a scan alone is not an approval. The provider-visible text of
each finding is its core-owned `model_excerpt`, hashed into the evidence; the
renderer sends it unchanged and never substitutes raw values. Project-profile
excerpts carry only extracted facts, headings, and one short overview line;
they omit any piece with a credential shape, machine-local path, or link, and
mark documentation as untrusted. Every system prompt and the shared analysis
prompt state that evidence text is untrusted data, never instructions. Reject `.git`, environment, credential, token, key, PEM,
and other secret-bearing paths, duplicate references, and credential-shaped
briefs, constraints, or excerpts before prompt construction. New-project
analysis may have no scan evidence but still receives only the bounded user
brief and typed constraints.

The already approved external launcher uses the reserved summary identifier
`@approved-launcher/descriptor.mod` at this boundary; its actual path stays in
the displayed finding value. Do not open the identifier or accept arbitrary
absolute paths. Bind it through the same completed scan, project, excerpt hash,
and explicit evidence-vector approval as other summaries. Cover the actual
scanner-to-approval path and reject an altered path, hash, or stale scan.

The application renders files only after deterministic validation and user confirmation.

Protocol failure coverage uses local fake transports only: browser and device
login parameter vectors, cancellation/failure, bounded timeout, usage-limit
responses, App Server interruption, missing schema output, output-field
rejection, redaction, and the no-account-metadata record shape. These tests do
not start a real login or persist any authentication material.

Update planning has an explicit reanalysis boundary: the UI first completes a
bounded read-only scan, displays the approved evidence manifest, then invokes
the same `existing_project_semantics` schema-constrained turn. Only the
core-confirmed record from the current session may be passed to
`build_maintenance_plan`; an old lock record or renderer-only record cannot
satisfy the update gate. Repair, reinstall, and managed removal retain their
documented locked-analysis or signed-out recovery rules.

Every analysis record is bound to the exact resolved source revision and
manifest SHA-256 used to validate component recommendations. Changing the
source selector invalidates the renderer proposal and requires a new reviewed
analysis. Maintenance reanalysis ignores renderer source defaults and derives
the source mode and pin from the installed lock. A pinned lock resolves its
exact commit; a Latest lock resolves one current exact revision, which is
persisted on the record and must match plan resolution or planning fails closed
and requires reanalysis. Locks written before
this binding existed may copy the fields only from valid source evidence
already stored in that same lock; absent or malformed provenance remains
blocked instead of being inferred from the current repository.

Aggregate scan evidence uses reserved `@scan/git-summary`,
`@scan/coding-environments`, and `@scan/absolute-paths` identifiers instead of
`.` or raw `.git` paths. These identifiers and `@approved-launcher/descriptor.mod`
are summaries, never filesystem inputs. They remain bound to the core scan ID,
canonical project root, finding/conflict reference, excerpt hash, and explicit
evidence approval. Do not relax the generic absolute-path or `.git` rejection.

## Optional ChatGPT sources

Source flattening remains independent of the setup assistant and non-blocking.
Show it as a minor install check; do not promote source status or a planning
recommendation on Ready or completion. The manifest installs the reusable
`.tools/package_chatgpt_project_sources.py` and Windows launcher for later
user-invoked export. See `docs/33_chatgpt_source_packaging.md` and the UI/source
skill supplements for the presentation and packaging boundaries. This export
uses no provider login, model, or credentials.

## Current implementation boundary

The Rust implementation is in `src-tauri/src/ai.rs`, `src-tauri/src/codex.rs`,
`src-tauri/src/claude_code.rs`, and the experimental
`src-tauri/src/account_cli.rs`. It starts only an absolute
`codex` executable discovered on the reviewed PATH, clears the child environment,
passes a small non-secret environment allowlist, and communicates with bounded
JSONL lines. `src-tauri/src/commands.rs` owns the process singleton and exposes
typed account, browser/device login, provider-account login/cancel/logout,
fixed-path system-browser opening, analysis, and confirmation commands
through `src/lib/tauri.ts`; React never invokes the low-level Tauri bridge.
`open_codex_login_url` accepts only the validated URL returned by Codex, while
`open_external_url` accepts the fixed reviewed source, product repository,
and Ready-screen portrait URLs. Both routes use the
platform-owned browser executable and argument array. The Ready-screen opener
launches the verified Codex executable with `codex app <project-root>`; do not
substitute the interactive CLI `--cd` route.

Before reuse, the core polls the supervised child transport and replaces an
exited App Server. Replacing or discarding a dead transport clears pending
analyses and approved scan evidence before a fresh initialize handshake.
Ordinary App Server request/response pairs use a 15-second limit; only the
separate analysis-completion and login waits retain their explicit longer
bounds. Keep these limits separate so a stalled account check cannot make the
first screen appear frozen.
Logout always tears down the local process and clears
pending analyses and approved evidence even when the remote logout request
returns an error.

Browser and device-code login attempts use a bounded per-`loginId` cancellation
registry. A direct cancellation request is sent to App Server when the session
is available; otherwise the bounded login wait observes the same token and
sends the managed cancellation before it returns. Login retries never clear a
different attempt's cancellation state. Reject missing, malformed, duplicate,
or excessive active login IDs.

Codex `thread/start` uses `sandbox: read-only`, `approvalPolicy: never`, and an
empty temporary working directory that contains no target-project path. Codex
`turn/start` receives the checked-in `docs/schemas/codex-analysis.schema.json`
as its `outputSchema` and the selected reasoning effort as `effort` on both
initial and corrective turns. Do not copy the model catalog's
`supportedReasoningEfforts[].reasoningEffort` field name into `turn/start`:
that method accepts `effort`, and a missing override can leave the user's
global effort setting in effect. Cover this distinction with a fake-transport
test using a non-default selection on both turns. The turn uses the current
App Server policy shape
`{type: readOnly, networkAccess: false}`. Do not add legacy `readOnly.access`
fields: current App Server schemas reject them. The schema itself binds the
eight scalar proposal keys to string values and `descriptor_tags` and
`folder_profile` to arrays of strings; keep this aligned with the defensive
Rust validator. The core rejects extra fields, duplicate
proposal keys, incomplete proposal sets, invalid evidence references, incorrect
excerpt hashes, unsafe identifiers, malformed tags, unsafe folder profiles, and
credential/account-shaped output before the result can reach planning. The
absolute project root and scan ID are core-only binding inputs: they are omitted
from the model prompt and serialized analysis record. The core keeps the
pending typed analysis and binding in a bounded session store and binds
confirmation to its exact analysis ID and complete proposal set; the renderer
cannot forge the immutable record fields. A plan or maintenance plan must carry
a confirmed `CodexAnalysisRecord`; it stores only digests, proposal keys,
confirmation time, the exact non-secret source revision and manifest digest,
and `account_identity_persisted: false`.

The deterministic AGENTS adapter appends the optional Super Events guidance
only when `workflow.super_events` is selected. It never appends the setup
provider, model, reasoning effort, or optimization profile. An unselected workflow must not
leave Super Events instructions, skill references, or equivalent guidance in
the generated project `AGENTS.md`.

Starting an approved installation passes only the core-owned plan ID and project root. Do not serialize the confirmed analysis record back through the renderer during apply; the Rust plan session already owns and validates that record.

Folder proposals and final folder state share a core validator that rejects
application-managed roots such as `.git`, `.codex`, `.agents`,
`.hoi4-mod-setup`, `paradox_wiki`, and `chatgpt_project_sources`.

The UI shows a collapsed input preview before a turn, labels the result “Suggested
by the selected setup assistant,” and requires an explicit confirmation action. Drafts and scan findings
remain unchanged on process, login, usage-limit, or schema failure. The new-project
renderer separately validates the user-confirmed launcher filename, descriptor
agreement, and replaceable PNG placeholder; these are deterministic Rust checks.

Core confirmation hashes the complete identity, including project and launcher
paths and advanced metadata. Editing any identity value must clear the
renderer confirmation while preserving the analysis record, expose the
existing Confirm action, and block previews and navigation until the user
reconfirms. Reconfirmation binds the edited values to the same core analysis;
it does not require another provider turn. Do not weaken the core digest or
silently reconfirm path edits. Test both path fields after confirmation.

## Corrective retry

Every adapter (Codex App Server, provider API, Claude Code, and the experimental
account CLI) allows
`ANALYSIS_ATTEMPTS` = 2 turns. When `validate_analysis_output` rejects a
response with a serialization error, one corrective turn is sent with
`corrective_analysis_prompt` (the validator's bounded reason plus the input
hash; Codex reuses the same thread). Validation is never relaxed, other error
categories are not retried, and the record binds only the accepted response.
Private/account-shaped, truncated, refused, unreadable, transport,
authentication, and usage-limit failures are discarded without a corrective
turn. The runtime schema and deterministic validator remain authoritative even
when a provider advertises native structured output.
Validator messages name the failing proposal and rule so live probes can
diagnose rejections; the renderer still sees only sanitized categories.

## Failure handling

Missing selected provider capability, signed-out state, cancelled login, usage limits, App Server or HTTP adapter exit, malformed output, or rejected proposals must preserve the local draft and scan. No failure may start a project transaction.

Map renderer-bound failures by sanitized category. Keep cancellation, signed
out, usage-limited, and private-looking-input states distinct; do not collapse
usage or input rejection into a sign-in error. Raw App Server method, error
code, parameter, account identity, credential, and protocol details never reach
React, including when the rate-limit lookup fails. A dead or malformed App
Server session is discarded, clears session-scoped proposals, and must complete
a fresh initialize handshake before reuse. Preserve the approved scan-evidence
binding across that restart so the user can retry without another filesystem
scan; explicit sign-out still clears both proposals and approved evidence.

Do not silently switch to another provider, an unverified endpoint, heuristic-only identity generation, or direct model writes.

## Tests

Cover:

- process startup and shutdown
- initialize ordering
- initialize response compatibility and platform metadata
- existing ChatGPT session
- browser login success, cancellation, and failure
- exact `account/login/cancel` method and `loginId` payload, including isolated
  concurrent-attempt cancellation and cancellation while the session is busy
- device-code fallback
- logout
- rate-limit state
- App Server crash and restart
- output schema acceptance and rejection
- prompt input manifest and redaction
- no token or account data in logs, state, lock, crash output, or project files
- deterministic rejection of invalid Codex identifiers
- recovery access while signed out
- provider-profile/model optimization binding and non-Codex credential isolation, including cross-provider reference rejection
- the shared 14-entry Claude catalog, Haiku 5.5/medium default, legacy
  one-effort Haiku 4.5, bounded exact custom Claude IDs, and no effort assumption
  for unknown IDs
- native Anthropic/Gemini/Grok schema payloads without tools or credential
  values, runtime Draft 2020-12 validation, core-assigned analysis IDs, empty
  evidence references for inference, and unknown-reference/recommendation-ID
  rejection
- experimental account-CLI package verification, private homes, empty
  workspaces, isolation policy, provider-owned session boundary, cancellation,
  logout, unsupported macOS, and no-secret persistence using fake processes
- provider-neutral optional component recommendations, including the
  `workflow.super_events` registry ID/schema path and selected-vs-unselected
  AGENTS guidance
- setup-assistant-independent flatten visibility, mapping, collision rejection, secret rejection, and optional setup-check presentation

`pnpm test:codex-live` is an opt-in Windows smoke using the signed installed
Codex executable named by `HOI4_CODEX_EXECUTABLE`. It reads only the current
account type, then starts and cancels browser and device-code login attempts in
a temporary Codex home. It does not print account or login values, open returned
URLs, or wait for user input. It proves login-start/cancel transport only;
manual browser and device-code completion remains a separate release gate.

## Update this skill when

Update this skill in the same change when provider profiles, authentication behavior, App Server methods, analysis schemas, process lifecycle, model-optimization policy, redaction rules, usage-limit handling, flattening behavior, or the AI-to-renderer boundary changes.
