---
name: hoi4-mod-setup-codex-integration
description: Use when implementing or changing provider authentication, model profiles, Codex App Server lifecycle, structured semantic analysis, usage-limit handling, or the boundary between AI proposals and deterministic project generation.
---

# HOI4 Mod Setup Codex Integration

Use this skill for the provider-neutral semantic layer of HOI4 Mod Setup. The user selects a setup assistant, live-catalog model, and model-supported reasoning effort at the start; Codex is the default profile. This choice is never the later Agentic HOI4 Modding development-client selection.

## Product rule

Create, Import, Update, and Repair planning require an authenticated, selected AI capability. Codex uses the official local `codex app-server` interface and ChatGPT-managed sign-in. Known hosted providers use checked-in, officially verified model/address defaults plus a provider-keyed OS-vault credential; model and address are editable under Advanced. Local and custom routes require explicit user configuration. Do not silently switch providers or invent a provider route or OAuth flow.

Fetch Codex models through App Server `model/list` and provider models through
the authenticated official Models API. Bind model plus reasoning effort to the
analysis record, plan, and lock. The checked-in fallback defaults are
`gpt-5.6-luna`/`xhigh` for Codex and `deepseek-v4-flash` for DeepSeek. Persist
these only as setup-analysis provenance; do not write them into generated
AGENTS/README guidance or use them to select development components.

A model-catalog timeout, error, or empty result must not remove the model
control. Keep the checked-in verified default selectable for Codex and every
known hosted profile, then merge a successful live catalog into those options.
Until live per-model metadata is available, expose only the fallback model's
verified default reasoning effort. Keep a concise visible distinction between a
live result, an empty result, and a failed refresh.
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
- store non-Codex provider keys only in Windows Credential Manager or macOS Keychain under a provider-keyed opaque reference; bind the non-secret reference scope to the selected provider and reject unscoped or cross-provider reuse
- inject a non-Codex key only as the adapter's approved environment/header input for the bounded request; never put it in project state, plan, lock, or logs
- treat local-model access as an explicit loopback route and do not claim hosted authentication for it

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
  sends it as `outputSchema`, while hosted/local adapters receive the exact
  checked-in schema in their system request
- reject additional or malformed fields
- validate component recommendations as the checked-in
  `docs/schemas/codex-analysis.schema.json` objects
  `{component_id,recommendation,reason}`; `component_id` must be a
  deterministic registry ID, including `workflow.super_events`, and unknown
  IDs or fields are rejected
- reject account-shaped text in summaries, proposal reasons, recommendation
  reasons, warnings, and proposal values
- require the complete ten-key proposal set before confirmation or planning
- return concise proposal reasons, not hidden reasoning

For existing-project analysis, the core accepts only normalized redacted
finding/conflict summaries, evidence references, and matching hashes emitted by a completed deterministic read-only scan in the
current app session and an explicit approval of that exact evidence vector.
Completion of a scan alone is not an approval. Reject `.git`, environment, credential, token, key, PEM,
and other secret-bearing paths, duplicate references, and credential-shaped
briefs, constraints, or excerpts before prompt construction. New-project
analysis may have no scan evidence but still receives only the bounded user
brief and typed constraints.

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

## Current implementation boundary

The Rust implementation is in `src-tauri/src/ai.rs` and `src-tauri/src/codex.rs`. It starts only an absolute
`codex` executable discovered on the reviewed PATH, clears the child environment,
passes a small non-secret environment allowlist, and communicates with bounded
JSONL lines. `src-tauri/src/commands.rs` owns the process singleton and exposes
typed account, browser/device login, fixed-path system-browser opening, logout,
analysis, and confirmation commands
through `src/lib/tauri.ts`; React never invokes the low-level Tauri bridge.
`open_codex_login_url` accepts only the validated URL returned by Codex, while
`open_external_url` accepts the fixed reviewed source, product repository,
ChatGPT handoff, and Ready-screen portrait URLs. Both routes use the
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
as its `outputSchema` and uses the current App Server policy shape
`{type: readOnly, networkAccess: false}`. Do not add legacy `readOnly.access`
fields: current App Server schemas reject them. The core rejects extra fields, duplicate
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

## Failure handling

Missing selected provider capability, signed-out state, cancelled login, usage limits, App Server or HTTP adapter exit, malformed output, or rejected proposals must preserve the local draft and scan. No failure may start a project transaction.

Map renderer-bound failures by sanitized category. Keep cancellation, signed
out, usage-limited, and private-looking-input states distinct; do not collapse
usage or input rejection into a sign-in error. Raw App Server method, error
code, parameter, account identity, credential, and protocol details never reach
React, including when the rate-limit lookup fails. A dead or malformed App
Server session is discarded, clears session-scoped proposals and evidence, and
must complete a fresh initialize handshake before reuse.

Do not silently switch to another provider, an unverified endpoint, heuristic-only identity generation, or direct model writes.

## Tests

Cover:

- process startup and shutdown
- initialize ordering
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
- provider-neutral optional component recommendations, including the
  `workflow.super_events` registry ID/schema path and selected-vs-unselected
  AGENTS guidance
- setup-assistant-independent flatten visibility, mapping, collision rejection, secret rejection, and recommendation copy

## Update this skill when

Update this skill in the same change when provider profiles, authentication behavior, App Server methods, analysis schemas, process lifecycle, model-optimization policy, redaction rules, usage-limit handling, flattening behavior, or the AI-to-renderer boundary changes.
