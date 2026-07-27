---
name: hoi4-mod-setup-codex-integration
description: Use when implementing or changing ChatGPT sign-in, Codex App Server lifecycle, structured semantic analysis, usage-limit handling, or the boundary between Codex proposals and deterministic project generation.
---

# HOI4 Mod Setup Codex Integration

Use this skill for the core subscription-backed semantic layer of HOI4 Mod Setup.

## Product rule

Create, Import, Update, and Repair planning require a ChatGPT-managed Codex session. The application uses the official local `codex app-server` interface. Do not add an OpenAI API key field or a provider fallback for core setup.

Recovery, rollback, backup inspection, and managed removal remain locally usable while signed out.

## Required contract

- launch `codex app-server` as a child process
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

## Analysis boundary

Codex owns semantic proposals. Deterministic Rust owns facts, validation, rendering, transactions, and readiness.

Use Codex for project description interpretation, display name, project ID, script prefix, namespace, descriptor tags, folder profile, `AGENTS.md` adaptation, component recommendations, existing-project purpose, and conflict explanation.

Use the scanner and validators for paths, hashes, descriptors, PNGs, encodings, Git state, identifier syntax, collisions, manifest checks, and file ownership.

Every analysis turn must:

- use a dedicated setup thread
- use read-only sandboxing
- expose no writable project root
- contain only user-approved inputs
- exclude secrets, binaries, Git objects, and credential stores
- set `outputSchema` to the current Codex analysis schema
- reject additional or malformed fields
- require the complete ten-key proposal set before confirmation or planning
- return concise proposal reasons, not hidden reasoning

For existing-project analysis, the core accepts only evidence references and
excerpt hashes emitted by a completed deterministic read-only scan in the
current app session. Reject `.git`, environment, credential, token, key, PEM,
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

The Rust implementation is in `src-tauri/src/codex.rs`. It starts only an absolute
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

`turn/start` receives the checked-in `schemas/codex-analysis.schema.json` as its
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

The UI shows a collapsed input preview before a turn, labels the result “Suggested
by Codex,” and requires an explicit confirmation action. Drafts and scan findings
remain unchanged on process, login, usage-limit, or schema failure. The new-project
renderer separately validates the user-confirmed launcher filename, descriptor
agreement, and replaceable PNG placeholder; these are deterministic Rust checks.

## Failure handling

Missing Codex, signed-out state, cancelled login, usage limits, App Server exit, malformed output, or rejected proposals must preserve the local draft and scan. No failure may start a project transaction.

Do not silently switch to API access, another model provider, heuristic-only identity generation, or direct model writes.

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

## Update this skill when

Update this skill in the same change when App Server methods, authentication behavior, analysis schemas, process lifecycle, model-selection policy, redaction rules, usage-limit handling, or the Codex-to-renderer boundary changes.
