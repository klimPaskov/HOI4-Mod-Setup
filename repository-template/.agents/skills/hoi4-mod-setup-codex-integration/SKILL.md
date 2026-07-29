---
name: hoi4-mod-setup-codex-integration
description: Use when implementing or changing provider authentication, model profiles, Codex App Server lifecycle, structured semantic analysis, usage-limit handling, or the boundary between AI proposals and deterministic project generation.
---

# HOI4 Mod Setup Codex Integration

Use this skill for the provider-neutral semantic layer of HOI4 Mod Setup. The user selects a provider and model at the start; Codex is the default profile.

## Product rule

Create, Import, Update, and Repair planning require an authenticated, selected AI capability. Codex uses the official local `codex app-server` interface and ChatGPT-managed sign-in. Other providers use only their verified adapter profile, explicit user endpoint where supported, and an OS-vault credential where required. Do not silently switch providers or invent a provider route.

Recovery, rollback, backup inspection, and managed removal remain locally usable while signed out.

## Required contract

- persist the selected provider, model, and optimization profile in typed state; never infer them from a display label
- derive endpoint, protocol, credential environment, limits, and supported platform from a checked-in provider profile
- launch `codex app-server` as a child process for Codex
- use stdio JSONL transport
- complete the initialize handshake before other requests
- use `account/read` for current state
- use `account/login/start` with `type: chatgpt` for the browser flow
- use `chatgptDeviceCode` only as the fallback flow
- open returned HTTPS login and device-code URLs through the typed fixed-path system-browser command
- wait for login and account update notifications
- use `account/logout` for sign-out
- expose a cancellation command that can interrupt a pending login wait before device-code fallback or logout
- never read or copy Codex token storage
- never serialize account identity, tokens, plan details, rate limits, or usage into a mod project or installation lock
- do not use experimental externally managed ChatGPT tokens
- store non-Codex provider keys only in Windows Credential Manager or macOS Keychain under a provider-keyed opaque reference; bind the non-secret reference scope to the selected provider and reject unscoped or cross-provider reuse
- inject a non-Codex key only as the adapter's approved environment/header input for the bounded request; never put it in project state, plan, lock, or logs
- treat local-model access as an explicit loopback route and do not claim hosted authentication for it

## Analysis boundary

The selected provider owns semantic proposals. Deterministic Rust owns facts, validation, rendering, transactions, and readiness. The common `codex-analysis` schema is the boundary even when the selected provider is not Codex.

Use the selected provider for project description interpretation, display name, project ID, script prefix, namespace, descriptor tags, folder profile, `AGENTS.md` adaptation, component recommendations, existing-project purpose, and conflict explanation. The optimization profile changes the prompt and presentation convention, not deterministic safety rules.

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
- require the complete ten-key proposal set before confirmation or planning
- return concise proposal reasons, not hidden reasoning

For existing-project analysis, the core accepts only evidence references and
excerpt hashes emitted by a completed deterministic read-only scan in the
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

## Current implementation boundary

The Rust implementation is in `src-tauri/src/ai.rs` and `src-tauri/src/codex.rs`. It starts only an absolute
`codex` executable discovered on the reviewed PATH, clears the child environment,
passes a small non-secret environment allowlist, and communicates with bounded
JSONL lines. `src-tauri/src/commands.rs` owns the process singleton and exposes
typed account, browser/device login, fixed-path system-browser opening, logout,
analysis, and confirmation commands
through `src/lib/tauri.ts`; React never invokes the low-level Tauri bridge.

Before reuse, the core polls the supervised child transport and replaces an
exited App Server. Logout always tears down the local process and clears
pending analyses and approved evidence even when the remote logout request
returns an error.

Codex `turn/start` receives the checked-in `docs/schemas/codex-analysis.schema.json` as its
`outputSchema`, uses `approvalPolicy: never` and a restricted read-only sandbox
with no project-readable roots, and the core rejects extra fields, duplicate
proposal keys, incomplete proposal sets, invalid evidence references, incorrect
excerpt hashes, unsafe identifiers, malformed tags, unsafe folder profiles, and
credential/account-shaped output before the result can reach planning. The
absolute project root and scan ID are core-only binding inputs: they are omitted
from the model prompt and serialized analysis record. The core keeps the
pending typed analysis and binding in a bounded session store and binds
confirmation to its exact analysis ID and complete proposal set; the renderer
cannot forge the immutable record fields. A plan or maintenance plan must carry
a confirmed `CodexAnalysisRecord`; it stores only digests, proposal keys,
confirmation time, and `account_identity_persisted: false`.

Folder proposals and final folder state share a core validator that rejects
application-managed roots such as `.git`, `.codex`, `.agents`,
`.hoi4-mod-setup`, `paradox_wiki`, and `chatgpt_project_sources`.

The UI shows a collapsed input preview before a turn, labels the result “Suggested
by Codex,” and requires an explicit confirmation action. Drafts and scan findings
remain unchanged on process, login, usage-limit, or schema failure. The new-project
renderer separately validates the user-confirmed launcher filename, descriptor
agreement, and replaceable PNG placeholder; these are deterministic Rust checks.

## Failure handling

Missing selected provider capability, signed-out state, cancelled login, usage limits, App Server or HTTP adapter exit, malformed output, or rejected proposals must preserve the local draft and scan. No failure may start a project transaction.

Do not silently switch to another provider, an unverified endpoint, heuristic-only identity generation, or direct model writes.

## Tests

Cover:

- process startup and shutdown
- initialize ordering
- existing ChatGPT session
- browser login success, cancellation, and failure
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
- Codex-only flatten visibility, mapping, collision rejection, secret rejection, and recommendation copy

## Update this skill when

Update this skill in the same change when provider profiles, authentication behavior, App Server methods, analysis schemas, process lifecycle, model-optimization policy, redaction rules, usage-limit handling, flattening behavior, or the AI-to-renderer boundary changes.
