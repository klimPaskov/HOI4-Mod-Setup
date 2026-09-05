# Codex subscription, ChatGPT authentication, and provider adapters

## Decision

Codex is the default setup-time semantic provider. HOI4 Mod Setup uses the user's Codex
access through their ChatGPT account for the Codex route. The application does
not use an application-owned OpenAI API key, does not request an API key for
Codex, and does not implement a separate token service. Users may instead
select a bounded non-Codex provider profile. Known hosted providers fill their
verified model and address automatically and ask only for a provider API key,
which is stored in the OS vault. Advanced users may review or change those
defaults. The
bounded non-Codex registry uses Claude, Kimi, GLM, DeepSeek, local, and
`custom` profiles. This selection chooses only the assistant used by HOI4 Mod
Setup for semantic planning. It does not choose or restrict the AI client the
user later uses for Agentic HOI4 Modding.

The integration boundary is the official local `codex app-server` process. The desktop application launches it as a child process and communicates over its default stdio JSONL transport. On Windows, Codex children are created with `CREATE_NO_WINDOW`, so the packaged app works from a shortcut or Start menu without a visible terminal. This is the Codex interface intended for product integrations that need authentication, threads, approvals, and streamed events.

Account reads, login start/wait, logout, descriptor preview capability checks,
and semantic analysis run through the desktop blocking-work dispatcher and a
bounded user-facing error mapper. A malformed JSONL transport response drops
the supervised session and requires a new initialize handshake; a transport-
valid response whose proposal is rejected keeps the initialized session and
approved scan available for retry.

Model-catalog, installation-plan authentication, and maintenance-plan
authentication failures use the same closed renderer error map. Provider or
App Server protocol, serialization, filesystem, and account detail is never
forwarded as raw planning text.

The core supervises the child transport before reusing a session and starts a
fresh App Server when the prior child has exited. Logout always drops the local
process and clears pending analysis and approved scan state even if the App
Server's remote logout request fails; the error remains visible so the user can
retry the Codex-owned sign-out flow.

Small App Server requests such as initialize, account state, and turn start use
a 15-second response limit. The completed analysis notification wait remains
separately bounded at 120 seconds. A stalled handshake therefore returns the UI
to a retryable state instead of leaving the first screen waiting for minutes.

Verified OpenAI references:

- Codex App Server: `https://developers.openai.com/codex/app-server/`
- Using Codex with a ChatGPT plan: `https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan`

## Core prerequisite

A compatible official Codex installation with `app-server` support is a blocking prerequisite for Create, Import, Update, and Repair planning. The app checks capability by starting `codex app-server`, completing the initialize handshake, and reading account state.

The app may show official installation or update guidance when Codex is absent or incompatible. It must not download, bundle, or replace a Codex executable unless a later release introduces a separately verified and licensed distribution design.

Recovery, rollback, backup inspection, and local removal of managed files remain available without ChatGPT sign-in. These operations must never depend on an online model.

### Non-Codex provider route

Claude uses an Anthropic messages envelope. Kimi, GLM, DeepSeek, local, and
the `custom` provider use the OpenAI-compatible envelope. Known hosted routes
use checked-in provider defaults and a provider-keyed OS-vault reference. Their
fixed account link opens the provider's official API-key page. Custom hosted
routes accept a user-supplied HTTPS address. Local models accept only a
user-supplied loopback HTTP address and are described as configured local
adapters, not hosted accounts. The app does not claim that an API key is an
OAuth login, and it does not invent provider URLs, package names, commands, or
model names. The first schema-validated request is the capability check.

## Authentication flow

1. Start `codex app-server` with stdio transport.
2. Send the required initialize request and wait for the initialized state.
3. Call `account/read` with token refresh disabled for the first state check.
4. Accept core setup access only when the reported account type is `chatgpt`.
5. When signed out, call `account/login/start` with:

```json
{
  "type": "chatgpt"
}
```

The App Server owns the managed ChatGPT login experience. Keep the request to
the required `type` field because optional client-brand and hosted-success-page
extensions vary between App Server schemas. Starting a new attempt clears a
stale availability message before the pending state is shown, so the interface
never presents an old failure alongside an active sign-in.

6. Open the returned `authUrl` through the typed Rust system-browser command, which accepts only a validated HTTPS URL and a fixed OS-owned opener.
7. Bind the pending UI state to the returned `loginId`. Cancellation calls
   `account/login/cancel` with that exact ID and stops only the matching local
   wait. A retry receives independent cancellation state.
8. Wait for `account/login/completed` and `account/updated`.
9. Recheck `account/read` before starting analysis.
10. When the browser callback fails or is unsuitable, offer
    `chatgptDeviceCode`. Display the returned verification URL and user code.
11. Use `account/logout` for sign-out.

The normal product UI must not expose App Server API-key login. API-key authentication does not satisfy the product requirement to use the user's ChatGPT Codex subscription.

## Credential ownership

Codex owns the ChatGPT OAuth flow, token persistence, and refresh. HOI4 Mod Setup must not:

- inspect Codex token files
- copy access or refresh tokens
- place tokens in application state
- place tokens in the mod project
- place tokens in the installation lock
- put tokens in logs, crash reports, analytics, command arguments, or environment dumps
- implement the experimental externally managed token mode

The app may display the live signed-in email and plan type returned by `account/read`. These values are transient UI data. Do not write the full email, account ID, plan type, usage details, or rate-limit data into the project or its lock file. A usage-limited authenticated account is blocked before descriptor preview or planning; recovery remains available.

## Usage and limits

Before a semantic analysis turn, read the current account and rate-limit state. A reached usage limit blocks new semantic analysis and produces a clear resumable state. It does not damage the current scan or transaction plan. If the rate-limit lookup itself fails, show only a bounded retry message; never forward the App Server method, error code, account identity, or protocol detail to the interface.

Do not silently switch providers, endpoints, models, or credential routes. The user can retry after the selected provider becomes available. Recovery and rollback remain usable.

Renderer-bound errors preserve sanitized categories: cancellation stays a
cancelled login, signed-out state asks for ChatGPT sign-in, usage exhaustion
preserves the draft and gives a retry-later action, private-looking input asks
the user to remove secrets, and protocol details remain hidden.

## Semantic responsibilities

The selected setup assistant is used for:

- interpreting a new-mod description
- proposing the display name
- proposing the stable project ID
- proposing the script prefix and namespaces
- producing the normalized project description
- proposing descriptor tags
- selecting the initial folder profile
- proposing project-specific `AGENTS.md` adaptations that render without setup-provider attribution
- recommending skills and subagents
- interpreting an existing project's purpose and conventions
- explaining conflicts and migration choices that require semantic judgment
- creating concise review summaries from deterministic scan evidence

The deterministic Rust core remains authoritative for:

- path existence and containment
- file hashes
- descriptor parsing
- PNG decoding
- identifier syntax and collision checks
- namespace frequency counts
- encoding detection
- Git state
- file ownership
- manifest dependency checks
- platform support
- transaction safety
- readiness evidence

No provider can override a deterministic failure. It cannot create files during analysis, approve a transaction, resolve a conflict automatically, or mark readiness as passed.

Provider/model/reasoning/profile values are retained only as setup-analysis
provenance. They must not be emitted into generated `AGENTS.md` or README files,
must not select or remove development-client components, and must not control
Open in Codex or flattened ChatGPT source packaging.

## Turn contract

Use a dedicated App Server thread for each Codex setup session. Start every semantic turn with:

- fetch the live model catalog through `model/list`; retain the checked-in
  `gpt-5.6-luna`/`xhigh` option when the catalog is empty or temporarily
  unreachable, without describing the fallback as a live result
- show only the selected model's advertised reasoning-effort values and bind the reviewed model and effort to both `thread/start` and `turn/start`
- `sandbox: read-only` on `thread/start`
- `sandboxPolicy: { type: readOnly, networkAccess: false }` on `turn/start`
- an empty temporary working directory, with no target-project path supplied
- explicit approved normalized evidence summaries and hashes
- a task-specific `outputSchema`
- a bounded prompt that asks for proposals and short rationale, not hidden reasoning

Non-Codex adapters use the same approved-input manifest and output schema. The
Rust core extracts their native response envelope, validates it, binds the
provider/model/profile to the confirmation record, and never gives the adapter
write access to the project.

The response must validate against `schemas/codex-analysis.schema.json`. The
schema binds scalar keys to string values and `descriptor_tags` and
`folder_profile` to arrays of strings before the turn can complete. Reject
malformed, incomplete, mistyped, credential-shaped, account-shaped, or extra
fields. The response must include the complete ten-key semantic proposal set.
Store only:

- analysis schema version
- analysis ID
- input digest
- output digest
- accepted proposal keys
- confirmation time
- exact resolved source revision
- exact source-manifest SHA-256

Do not store the App Server thread, turn history, account identity, tokens, hidden reasoning, absolute project roots, scan IDs, or raw unapproved project text in the installation lock. The core retains the root and scan binding only in its bounded in-memory pending-analysis session.

The Rust core retains a bounded pending analysis session and binds confirmation to the exact analysis ID, digests, and returned proposal keys. The renderer cannot manufacture a confirmed record by supplying arbitrary immutable fields. Descriptor and thumbnail previews use the same confirmed-session gate as installation planning.

## New-project identity sequence

1. Collect the user's plain-language brief.
2. Verify the selected provider, or ChatGPT authentication for Codex.
3. Send only the brief, approved relative evidence, and explicit wizard constraints to the selected provider; the absolute project root and scan ID remain core-only bindings.
4. Receive the complete schema-constrained proposal set for name, ID, prefix, namespace, description, tags, folder profile, `AGENTS.md` profile, localisation convention, and documentation convention.
5. Run deterministic validation over every proposed field.
6. Show the proposal with concise rationale.
7. Let the user edit and confirm each field.
8. Render `descriptor.mod`, the external launcher `.mod`, `thumbnail.png`, and the scaffold deterministically from confirmed data.

Codex proposes identity. The renderer owns final bytes.

## Existing-project analysis sequence

1. Run the bounded deterministic scan.
2. Build an input manifest of approved normalized finding/conflict summaries
   and their core-bound hashes. Inventory does not approve raw file text.
3. Show the manifest before transmission.
4. Verify the selected provider and available usage.
5. Send the approved evidence to a read-only provider turn with the output schema.
6. Validate the response.
7. Show semantic suggestions separately from detected facts.
8. Require confirmation before the installation plan can be created.

## Failure states

| State | Setup effect | Local recovery effect |
| --- | --- | --- |
| Codex missing | Block Create, Import, Update, and Repair planning | Recovery remains available |
| Signed out | Show ChatGPT sign-in gate | Recovery remains available |
| Login cancelled | Keep wizard inputs locally and stay at sign-in | Recovery remains available |
| Login failed | Show retry and device-code options | Recovery remains available |
| Usage limited | Preserve scan and draft, block new analysis | Recovery remains available |
| App Server exits | Mark session interrupted, preserve the approved scan, and allow restart | No project mutation |
| Malformed analysis | Reject response and retry with same approved input | No project mutation |
| User rejects proposal | Return to editable brief or review | No project mutation |
| Provider endpoint or key missing | Preserve the draft and block new analysis | Recovery remains available |
| Provider response too large, malformed, or redirected | Reject response and preserve the draft | No project mutation |

## Readiness

The final readiness report includes blocking checks for:

- the selected provider configuration, or compatible Codex App Server
- ChatGPT-managed authentication verified during a Codex setup session
- required semantic analysis completed
- all provider proposals deterministically validated
- every required proposal confirmed by the user
- no account identity, provider key, or token stored in project artifacts

Optional 3D state remains independent of this core provider gate.
Open in Codex is a separate development-client handoff gated by core readiness
and the installed Codex project integration, not by the setup assistant.
