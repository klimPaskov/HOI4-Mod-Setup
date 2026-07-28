# Provider credential and flattened Chat source security audit

Audit date: 2026-07-28
Verdict: **Fail for the bounded provider/flatten security gate.**
Workspace note: the audited files already contained uncommitted and untracked work. This audit changed no product source and wrote only this report.

## Threat model and scope

The scoped threat surfaces were:

- renderer-controlled provider, model, endpoint, and credential-reference command inputs;
- non-Codex API keys crossing from the renderer into the Rust core and OS credential vault;
- provider-key lookup, HTTP-header injection, response handling, and persistence;
- untrusted existing-project files selected for `chatgpt_project_sources/`;
- path normalization, root containment, links/junctions, case collisions, secret detection, and size/count limits;
- plan, state, lock, and analysis-record serialization;
- the in-scope Codex App Server command boundary: no API-key route, no account metadata persistence, approved analysis input, and signed-out recovery.

Inspected source was limited to:

- `src-tauri/src/credentials.rs`
- `src-tauri/src/ai.rs`
- `src-tauri/src/flatten.rs`
- `src-tauri/src/commands.rs`
- `src-tauri/src/models.rs`
- `schemas/`
- `README.md`, `SECURITY.md`
- `docs/13_security_model.md`
- `docs/30_codex_chatgpt_authentication.md`
- `docs/31_ai_provider_profiles_and_chat_sources.md`
- the mirrored security and Codex-integration skills

The implementation of shared path/redaction helpers, Codex JSONL transport, transaction execution, UI bridge, support bundles, Git, archives, updater, CI, and release workflows was outside the parent-provided file boundary. Conclusions about those surfaces are therefore limited to their named call sites and schemas.

## Findings by severity

### High — Provider scope is mutable metadata, allowing cross-provider key use or deletion

The OS vault account is keyed only by the generic opaque URI. `OsCredentialStore::save` creates a reference with no provider scope and stores the key under `reference.reference` (`src-tauri/src/credentials.rs:64-84`). `save_ai_provider_key` adds `provider_id` only after the vault write (`src-tauri/src/credentials.rs:223-231`). Validation then trusts the caller-provided `provider_id` field rather than binding the provider to the vault account (`src-tauri/src/credentials.rs:164-200`).

The command surface exposes the full serializable reference to the renderer and accepts renderer references back:

- store returns `CredentialReference` (`src-tauri/src/commands.rs:532-548`);
- account status prefers the supplied reference over the core map (`src-tauri/src/commands.rs:511-529`);
- analysis prefers the supplied reference and passes it to the vault-backed adapter (`src-tauri/src/commands.rs:703-734`);
- deletion validates and deletes the supplied reference before proving it is the core map entry for that provider (`src-tauri/src/commands.rs:551-573`).

Because the reference URI and `provider_id` are separately mutable serialized fields (`src-tauri/src/models.rs:986-993`), a valid reference can be re-labeled for another configured provider. The key can then be sent to that provider's user-entered HTTPS endpoint or deleted.

The existing unit test checks an unchanged Claude reference against DeepSeek, but does not mutate the serialized `provider_id` or exercise the command routes (`src-tauri/src/credentials.rs:342-351`).

Remediation:

- make provider references core-only;
- return only a non-secret connected/disconnected status to the renderer;
- remove credential-reference parameters from account, analysis, and deletion commands;
- resolve and delete only the exact provider-keyed core entry;
- bind provider identity into the vault account/namespace or equivalent authenticated core metadata;
- reject legacy unscoped references until reconnect.

Regression tests must attempt cross-provider read, analysis, and deletion with a re-labeled reference and prove that no AI-provider reference appears in renderer payloads, plans, locks, state, logs, or analysis output.

### High — Flatten reads are vulnerable to link/junction swap races

Fallback `AGENTS.md` and `README.md` are checked with `symlink_metadata` and then reopened by path (`src-tauri/src/flatten.rs:42-53`). Additional files are checked before and after `fs::read`, but neither metadata check is tied to the opened file identity (`src-tauri/src/flatten.rs:111-149`).

A project-local leaf or ancestor can be swapped between validation and open, allowing an outside-root readable file to become a transaction-managed flattened artifact. The second metadata check does not close the race because the path can be restored before that check.

The current flatten tests cover nominal mapping, traversal, different-content case collisions, and UTF-8 secret text, but no leaf or ancestor link-swap race (`src-tauri/src/flatten.rs:239-386`).

Remediation:

- traverse from a trusted root handle without following links/reparse points;
- open the final file with no-follow semantics;
- read from that same handle;
- verify the opened file identity, type, containment, and size before accepting bytes;
- fail closed on any parent-component identity change.

Add adversarial symlink tests on macOS and reparse-point/junction tests on Windows, including swaps before open, during read, and before final verification.

### Medium — Required flattened files bypass the per-file bound and can allocate before rejection

The module declares an 8 MiB per-file limit (`src-tauri/src/flatten.rs:15-17`), but applies it only to user extras (`src-tauri/src/flatten.rs:118-139`). Fallback `AGENTS.md` and `README.md` are read without a pre-read size bound (`src-tauri/src/flatten.rs:42-53`). Aggregate limits are evaluated only after all output bytes are resident (`src-tauri/src/flatten.rs:152-166`).

An untrusted existing project can therefore cause an oversized required file to be allocated before the 64 MiB aggregate check, creating a memory-exhaustion path. Prepared/generated skill and subagent inputs also receive no explicit final per-file check in this module.

Remediation: apply the per-file bound to every required, skill, subagent, and extra source before allocation; use bounded streaming reads; recheck the opened handle's size and actual byte count.

### Medium — Non-UTF-8 required, skill, or subagent content bypasses secret detection

`insert_output` scans content only when the entire byte sequence is valid UTF-8 (`src-tauri/src/flatten.rs:183-194`). Invalid UTF-8 is accepted as a binary generated artifact and retains its original bytes (`src-tauri/src/flatten.rs:168-179`). The lossy scan used for extras is not applied to fallback required files or prepared/generated skill and subagent inputs.

Consequently, a text-class source containing an invalid byte can bypass the secret-shaped-content rejection and be copied into the Chat source folder. The user must still choose to use that folder, but the product explicitly recommends doing so.

Remediation: require valid bounded UTF-8 for `AGENTS.md`, `README.md`, skill Markdown, subagent TOML, and other text-class sources. If binary extras remain supported, run a byte-oriented detector and clearly exclude them from Chat upload guidance.

### Medium — Model and endpoint fields can carry secrets into project artifacts, plans, and locks

Provider configuration treats model and endpoint as non-secret, but validates only length/control characters for the model and URL structure for the endpoint (`src-tauri/src/ai.rs:108-164`). A credential-shaped model or endpoint path is not rejected. The endpoint can be persisted in the plan (`src-tauri/src/commands.rs:2607-2629`), while the model is also rendered into the project `README.md` (`src-tauri/src/commands.rs:2056-2075`) and project state (`src-tauri/src/commands.rs:2421-2458`).

This creates a leakage path when a user accidentally pastes a key into a model field or a token-bearing URL path. Query, fragment, and URL userinfo rejection are good controls but do not cover this case.

Remediation: reject credential-shaped provider configuration before capability checks, plan creation, generated artifacts, logging, or persistence. Add tests for token-shaped model values and endpoint path segments.

### Medium — Existing flattened-file replacement declares unsafe rollback semantics

A modified existing generated artifact remains `OperationAction::Generate` when the user chooses replace (`src-tauri/src/commands.rs:2493-2520`). Every `Generate` or `Rename` operation is then assigned `RollbackAction::RemoveCreated`, even when the destination existed and has a local hash (`src-tauri/src/commands.rs:2545-2582`).

This contradicts the backup/restore contract for replacing a user file. The transaction executor was outside this bounded audit, so the exact data-loss behavior was not re-evaluated here; the plan metadata itself is incorrect and must not pass the gate.

Remediation: derive rollback from pre-apply destination state. Replacing an existing file must restore its verified backup; only a genuinely new destination may use `remove_created`.

### Medium — Current schemas permit forbidden AI references and provider-default ambiguity

The accepted design says AI-provider references never enter project state, plans, or locks. The installation-plan schema nevertheless allows `AI_PROVIDER_API_KEY` and a `provider_id` inside `credential_references` (`schemas/installation-plan.schema.json:129-140`). Project state also permits generic credential references and does not require `ai` (`schemas/project-state.schema.json:7-16`, `schemas/project-state.schema.json:72-100`, `schemas/project-state.schema.json:153-165`).

Plan and lock schemas define provider/model/endpoint/optimization/flatten fields but omit them from the required sets (`schemas/installation-plan.schema.json:7-20`, `schemas/installation-plan.schema.json:32-52`; `schemas/installation-lock.schema.json:7-19`, `schemas/installation-lock.schema.json:27-47`). Rust silently defaults missing provider/model/flatten values (`src-tauri/src/models.rs:643-666`, `src-tauri/src/models.rs:754-775`). Project state is generated only for a new project or when LoRA interest is selected (`src-tauri/src/commands.rs:2421-2459`).

This allows schema-valid or migrated artifacts to lose provider identity, silently become Codex/default, or carry an AI-provider reference contrary to the core-only contract.

Remediation:

- allow only the explicitly supported Meshy opaque reference in plan/project schemas;
- require current-version AI and flatten fields in plan, lock, and project state;
- use explicit legacy migrations instead of serde defaults for security-relevant provider identity;
- cross-check provider/model/profile between state, confirmed analysis, plan, and lock.

### Medium — Existing-import analysis is not bound to its root and scan at the command boundary

Evidence/root/scan binding is enforced only for `maintenance_reanalysis` (`src-tauri/src/commands.rs:756-805`). Persisted-record comparison deliberately removes root and scan (`src-tauri/src/commands.rs:2126-2143`), and initial planning reads the current root separately from the confirmed record (`src-tauri/src/commands.rs:2173-2180`).

This leaves `existing_project_import` analysis reusable after a project-root change in the same session. It weakens the approved-input boundary and provenance of provider recommendations.

The command tests cover maintenance reanalysis binding, not initial-import replay (`src-tauri/src/commands.rs:3797-3893`).

Remediation: require root and current completed scan ID for every existing-project analysis, retain them only in core memory, and compare them again at confirmation and plan creation.

### Medium — Update carries old flattened bytes instead of regenerating and revalidating them

Update planning copies prior generated lock entries into the incoming set unchanged (`src-tauri/src/commands.rs:2931-2944`). The resulting maintenance plan contains no generated artifacts (`src-tauri/src/commands.rs:3180-3199`).

An updated skill, subagent, adapted `AGENTS.md`, or `README.md` can therefore diverge from its flat copy. The update route also does not rerun the current flatten secret, size, and collision checks over those carried bytes.

Remediation: regenerate the complete flattened set from the exact update inputs, compare it with local/base state through normal conflicts, and require readiness to verify completeness and hash freshness.

### Low — Identical case-colliding destinations are silently deduplicated

Destination keys are lowercased, but a collision with identical bytes returns success instead of rejecting the ambiguous mapping (`src-tauri/src/flatten.rs:195-204`). The existing case-collision test uses different bytes (`src-tauri/src/flatten.rs:333-360`).

The design requires collision rejection, not content-based deduplication. Reject every distinct source mapping that resolves to the same normalized platform destination, including identical content.

### Low — The command surface accepts an unnecessary key for the local profile

The local profile declares `requires_credential = false` (`src-tauri/src/ai.rs:72-78`), but `store_ai_provider_credential` rejects only Codex and therefore accepts `local` (`src-tauri/src/commands.rs:532-548`). The key is not sent because analysis checks `requires_credential`, but collecting and retaining an unused secret violates the local no-key route.

Reject credential storage for every profile whose checked-in definition does not require a credential.

## Confirmed controls

- Production provider commands use `OsCredentialStore`; Windows/macOS routes use the keyring service and unsupported platforms fail closed (`src-tauri/src/credentials.rs:61-123`).
- Key values are read only for the bounded provider request and injected into an HTTP header, not a child environment or command argument (`src-tauri/src/ai.rs:209-243`, `src-tauri/src/ai.rs:280-367`).
- Hosted endpoints require HTTPS; local endpoints require loopback HTTP; URL userinfo, query, and fragment are rejected; redirects are disabled; timeout and response-body limits are set (`src-tauri/src/ai.rs:108-164`, `src-tauri/src/ai.rs:280-367`).
- The scoped command surface rejects storing a Codex API key and rejects routing Codex through the generic provider adapter (`src-tauri/src/commands.rs:532-543`, `src-tauri/src/commands.rs:703-707`).
- Codex planning accepts only a live `chatgpt` account mode and blocks usage-limited accounts (`src-tauri/src/commands.rs:340-385`).
- Scoped models contain no token, email, account ID, plan, usage, or rate-limit field in the persisted analysis record; root and scan are skipped during serialization (`src-tauri/src/models.rs:1045-1075`).
- Generated state records `account_values_persisted: false`, and the analysis record requires the corresponding false value (`src-tauri/src/commands.rs:2442-2450`, `src-tauri/src/models.rs:1061-1064`).
- Flattening is rejected for non-Codex providers and its outputs become generated plan operations rather than direct writes (`src-tauri/src/commands.rs:2173-2185`, `src-tauri/src/commands.rs:2473-2591`).
- Installation requires the core-stored plan fingerprint and explicit dry-run approval before the transaction runner is called (`src-tauri/src/commands.rs:3389-3403`).
- Rollback, journal inspection, interrupted-transaction discovery, resume, staging discard, and managed removal have no AI-session prerequisite in their command routes (`src-tauri/src/commands.rs:2759-2773`, `src-tauri/src/commands.rs:3411-3510`).

## Credential leakage checks

- A high-confidence current-snapshot scan covered the named Rust files, schemas, security/provider docs, README, SECURITY, and mirrored skills. It found no apparent real OpenAI, Anthropic, GitHub, AWS, or private-key literal.
- The only Meshy-shaped hit was the explicit rejected placeholder in `src-tauri/src/credentials.rs:208`.
- A bounded Git-history search over the scoped tracked paths found zero commits matching the same high-confidence non-Meshy credential patterns.
- No `println!`, `eprintln!`, `dbg!`, `tracing::`, or `log::` call was present in the named provider/flatten modules.
- Provider request errors do not serialize headers or response bodies in the scoped code. Error statuses expose only the HTTP status code (`src-tauri/src/ai.rs:335-367`).

Limitations:

- crash reporting, support bundles, screenshots, protocol logging, and generic redaction implementation were not in the allowed file set;
- the static scan is not proof against arbitrary or novel secret formats;
- raw App Server logging and token-file access are implemented, if anywhere, outside the scoped files. No such route is exposed by the inspected commands or models.

## Filesystem and process checks

Filesystem:

- positive controls exist for normalized relative extras, output-root recursion rejection, secret-like extra paths, case-folded destination keys, per-extra/aggregate limits, and final transaction destinations;
- the high/medium findings above prevent a pass because checks are not handle-bound and limits/secret checks do not cover every input class;
- archive count, ratio, depth, and extraction limits were not applicable to `flatten.rs` and were not inspected elsewhere.

Process:

- non-Codex provider analysis uses bounded in-process HTTP and flattening starts no process;
- the fixed browser opener call uses an argument array, empty environment-name list, timeout, and output bound (`src-tauri/src/commands.rs:627-644`);
- App Server startup consumes `find_codex_executable()` and describes the result as coming from a reviewed `PATH` (`src-tauri/src/commands.rs:294-315`). The executable-discovery implementation is in excluded `codex.rs`, so this audit cannot confirm the required canonical identity, no-token-file behavior, cleared environment, output bounds, or protocol-log redaction. That boundary remains unverified, not passed.

## Supply-chain and workflow checks

- Flattening consumes prepared source bytes after the in-scope command path requests an exact resolved revision, fetches a declared file, and verifies its download before adaptation (`src-tauri/src/commands.rs:2187-2257`).
- The generated flat folder itself does not fetch packages, execute scripts, clone repositories, or bypass dry-run approval.
- Exact revision/hash resolution internals, redirects/cache, updater trust, Git hooks/config/remotes, GitHub Actions permissions, fork behavior, release environments, and release-secret exposure were outside the bounded file list and were not audited.
- No public vulnerability report was created.

## Tests and evidence run

- `cargo test --manifest-path src-tauri/Cargo.toml credentials::tests -- --test-threads=1`: 4 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml ai::tests -- --test-threads=1`: 5 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml flatten::tests -- --test-threads=1`: 3 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --all-features commands::tests -- --test-threads=1`: 16 passed.
- All 9 schema files parsed with `JSON.parse`.
- Bounded current-snapshot and scoped Git-history credential-pattern checks completed as described above.

No real provider request, real Codex login, real OS-vault mutation, macOS execution, desktop E2E, link-race stress, or transaction fault injection was run.

## Missing security tests

Credential/provider:

- re-labeled cross-provider reference use and deletion through every command;
- proof that provider references remain core-only and never serialize to renderer/state/plan/lock;
- rejection of legacy unscoped references;
- rejection of credentials for the local profile;
- token-shaped model and endpoint-path rejection;
- fake hosted servers for exact auth-header isolation, redirects, timeout, oversized/chunked bodies, malformed envelopes, and sanitized failures;
- Windows Credential Manager and macOS Keychain integration tests.

Flatten/filesystem:

- link/junction swaps for required files, extras, and ancestor directories;
- no-follow handle identity checks on Windows and macOS;
- per-file limits for required, skill, and subagent files before allocation;
- aggregate count/size boundaries and overflow;
- invalid UTF-8 carrying secret-shaped bytes;
- identical-content case collisions, Unicode normalization/case collisions, reserved names, and alternate-data-stream forms;
- update-time regeneration and readiness hash freshness.

Transaction/analysis:

- generated-file replace followed by rollback and inverse rollback;
- initial-import project-root/scan replay rejection;
- signed-out rollback, resume, removal, and backup inspection desktop E2E.

Codex App Server tests outside this file boundary remain required for no API-key fallback, no token-file reads, no external-token mode, redacted protocol logs, canonical executable identity, interruption handling, and browser/device-code recovery.

## Recommended remediation order

1. Remove renderer authority over provider references and bind vault entries to provider identity.
2. Replace flatten check-then-open reads with root-handle, no-follow, identity-verified bounded reads.
3. Apply secret and per-file checks to every flattened input; reject invalid text encodings and all ambiguous destination mappings.
4. Reject credential-shaped model/endpoint configuration before any display or persistence.
5. Correct generated replacement rollback metadata and add rollback/inverse fault tests.
6. Tighten schemas and migrations so provider identity is required and AI-provider references cannot enter artifacts.
7. Bind every existing-project analysis to the current root and completed scan through confirmation and planning.
8. Regenerate and verify flattened artifacts on update/readiness.
9. Complete the out-of-scope App Server process/token/logging audit before accepting the combined provider gate.
