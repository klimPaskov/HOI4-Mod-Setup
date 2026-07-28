# Codex subscription, ChatGPT authentication, and provider adapters

## Decision

Codex is the default semantic provider. HOI4 Mod Setup uses the user's Codex
access through their ChatGPT account for the Codex route. The application does
not use an application-owned OpenAI API key, does not request an API key for
Codex, and does not implement a separate token service. Users may instead
select a bounded non-Codex provider profile; those routes require an explicit
endpoint and OS-vault credential when the verified profile requires one.

The integration boundary is the official local `codex app-server` process. The desktop application launches it as a child process and communicates over its default stdio JSONL transport. This is the Codex interface intended for product integrations that need authentication, threads, approvals, and streamed events.

The core supervises the child transport before reusing a session and starts a
fresh App Server when the prior child has exited. Logout always drops the local
process and clears pending analysis and approved scan state even if the App
Server's remote logout request fails; the error remains visible so the user can
retry the Codex-owned sign-out flow.

Verified OpenAI references:

- Codex App Server: `https://developers.openai.com/codex/app-server/`
- Using Codex with a ChatGPT plan: `https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan`

## Core prerequisite

A compatible official Codex installation with `app-server` support is a blocking prerequisite for Create, Import, Update, and Repair planning. The app checks capability by starting `codex app-server`, completing the initialize handshake, and reading account state.

The app may show official installation or update guidance when Codex is absent or incompatible. It must not download, bundle, or replace a Codex executable unless a later release introduces a separately verified and licensed distribution design.

Recovery, rollback, backup inspection, and local removal of managed files remain available without ChatGPT sign-in. These operations must never depend on an online model.

### Non-Codex provider route

Claude uses an Anthropic messages envelope. Kimi, GLM, DeepSeek, local, and
another configured provider use the OpenAI-compatible envelope. Hosted routes
accept only a user-supplied HTTPS endpoint and a provider-keyed OS-vault
reference. Local models accept only a user-supplied loopback HTTP endpoint and
are described as configured local adapters, not hosted accounts. The app does
not invent provider URLs, OAuth routes, package names, commands, or model
names. The first schema-validated request is the capability check.

## Authentication flow

1. Start `codex app-server` with stdio transport.
2. Send the required initialize request and wait for the initialized state.
3. Call `account/read` with token refresh disabled for the first state check.
4. Accept core setup access only when the reported account type is `chatgpt`.
5. When signed out, call `account/login/start` with:

```json
{
  "type": "chatgpt",
  "useHostedLoginSuccessPage": true,
  "appBrand": "chatgpt"
}
```

6. Open the returned `authUrl` through the typed Rust system-browser command, which accepts only a validated HTTPS URL and a fixed OS-owned opener.
7. Wait for `account/login/completed` and `account/updated`.
8. Recheck `account/read` before starting analysis.
9. When the browser callback fails or is unsuitable, offer `chatgptDeviceCode`. Display the returned verification URL and user code.
10. Use `account/logout` for sign-out.

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

Before a semantic analysis turn, read the current account and rate-limit state. A reached usage limit blocks new semantic analysis and produces a clear resumable state. It does not damage the current scan or transaction plan.

Do not silently switch providers, endpoints, models, or credential routes. The user can retry after the selected provider becomes available. Recovery and rollback remain usable.

## Semantic responsibilities

The selected provider is used for:

- interpreting a new-mod description
- proposing the display name
- proposing the stable project ID
- proposing the script prefix and namespaces
- producing the normalized project description
- proposing descriptor tags
- selecting the initial folder profile
- adapting `AGENTS.md`
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

## Turn contract

Use a dedicated App Server thread for each Codex setup session. Start every semantic turn with:

- no model override by default, so the user's Codex configuration controls the available model
- read-only sandbox policy
- no project write roots
- a restricted read-only access policy with no project-readable roots
- explicit approved input excerpts
- a task-specific `outputSchema`
- a bounded prompt that asks for proposals and short rationale, not hidden reasoning

Non-Codex adapters use the same approved-input manifest and output schema. The
Rust core extracts their native response envelope, validates it, binds the
provider/model/profile to the confirmation record, and never gives the adapter
write access to the project.

The response must validate against `schemas/codex-analysis.schema.json`. Reject malformed, incomplete, credential-shaped, account-shaped, or extra fields. The response must include the complete ten-key semantic proposal set. Store only:

- analysis schema version
- analysis ID
- input digest
- output digest
- accepted proposal keys
- confirmation time

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
2. Build an input manifest of approved text excerpts and computed findings.
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
| App Server exits | Mark session interrupted and allow restart | No project mutation |
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

Optional 3D and LoRA states remain independent of this core provider gate.
