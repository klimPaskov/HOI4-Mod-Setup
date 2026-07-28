# Provider and flattened Chat sources audit

Audit date: 2026-07-28
Scope: current provider-neutral analysis, Codex App Server/authentication boundary, Codex-only flattened Chat sources, persistence, readiness, and the named transaction/integration tests. The working tree was already modified; this audit changed no application source.

## Verdict

**Fail for the audited provider/flatten acceptance gate.** The nominal Codex App Server flow, common output validation, read-only sandbox, Codex-only UI visibility, and normal transaction stages are present, but the current snapshot has credential-containment, exact-input-consent, executable-identity, link-race, rollback, binding, maintenance, and persistence gaps. This is not a statement that the overall product is complete or incomplete.

## Findings

### High — Non-Codex vault references are renderer-controlled and not bound to a provider

`save_ai_provider_key` creates every hosted-provider key in the same generic namespace, and reference validation proves only the platform/name/UUID shape; it does not prove provider ownership (`src-tauri/src/credentials.rs:162-170`, `src-tauri/src/credentials.rs:193-198`). `ai_account_read` and `ai_analyze` accept a renderer-supplied opaque reference in preference to the provider-keyed core map (`src-tauri/src/commands.rs:512-526`, `src-tauri/src/commands.rs:701-724`). Removal deletes the supplied vault item before checking whether it belongs to the named provider (`src-tauri/src/commands.rs:549-570`). The reference also crosses into React state and back over the bridge (`src/types.ts:144-149`, `src/types.ts:387-400`, `src/App.tsx:374-375`, `src/App.tsx:417-423`, `src/App.tsx:999-1005`).

Impact: a stale or substituted valid reference can disclose one provider's key to another user-entered HTTPS endpoint, or delete another provider's key. This contradicts the provider-keyed vault boundary and the rule that key references never enter React state. Fails ANA-05 and ANA-21.

### High — The displayed/edited scan input is not the input transmitted to the provider

The bridge stores the scanner's immutable original value as `evidenceExcerpt` (`src/lib/tauri.ts:252-261`, `src/lib/tauri.ts:286-300`). The findings UI previews and edits `finding.value` (`src/App.tsx:1149-1151`), while transmission and digesting use `finding.evidenceExcerpt` whenever present (`src/App.tsx:343-349`). The type comment confirms that edits are review-only (`src/types.ts:151-160`). The disclosure is also hard-coded as “Codex input preview” on non-Codex routes and does not show the complete request manifest (`src/App.tsx:1151`; request construction at `src/App.tsx:406-423`).

Impact: a user can edit apparent private text or identifiers before continuing, yet the unedited original excerpt is sent and hashed. There is no exact, provider-neutral preview/approval of the bytes about to leave the application. This is a direct private-project-data disclosure risk. Fails ANA-10, ANA-11, ANA-17, and UI-13.

### High — A `PATH` filename is treated as the official credential-owning Codex executable

Discovery accepts the first ordinary `codex.exe`/`codex` file on inherited `PATH` (`src-tauri/src/codex.rs:1437-1456`), and `with_codex_session` immediately labels and starts it as official (`src-tauri/src/commands.rs:294-315`). The child receives account-storage locations including `HOME`, `USERPROFILE`, `APPDATA`, and `CODEX_HOME` (`src-tauri/src/codex.rs:1280-1300`). There is no canonical core-owned executable identity or user-reviewed executable path. This conflicts with the security skill's explicit prohibition on PATH shims.

Impact: a PATH-prepended lookalike can receive approved project input and inspect Codex-owned account storage. This threatens the no-token-read/copy boundary even though application code does not directly parse tokens. Fails ANA-06 and ANA-07.

### High — Replacing an existing generated/flattened file records the wrong rollback semantics

A modified generated artifact selected as `replace` remains `OperationAction::Generate` (`src-tauri/src/commands.rs:2473-2499`), and every `Generate`/`Rename` is assigned `RollbackAction::RemoveCreated` even when the destination already exists (`src-tauri/src/commands.rs:2552-2560`). The transaction nevertheless backs up the existing file (`src-tauri/src/transaction.rs:880-959`). Rollback construction interprets `RemoveCreated` as a desired absent result (`src-tauri/src/transaction.rs:2219-2263`); the parent rollback restores the backup (`src-tauri/src/transaction.rs:2648-2695`) but records the rollback child as absent (`src-tauri/src/transaction.rs:2482-2486`). An inverse rollback then rejects the restored original as a post-rollback user-created file (`src-tauri/src/transaction.rs:2632-2636`).

Impact: a user-approved replacement of a flattened file can be rolled back once, but the advertised inverse rollback is not available. Fails AI-07, TX-07, and TX-08.

### High — Flattened fallback/extras reads remain vulnerable to link-swap races

Fallback `AGENTS.md`/`README.md` are link-checked and then reopened by path (`src-tauri/src/flatten.rs:42-53`). Extras are checked before and after `fs::read`, but neither metadata check is tied to the opened file identity (`src-tauri/src/flatten.rs:95-149`). `safe_join` performs path checks before the later open and therefore does not close this race (`src-tauri/src/security.rs:104-134`).

Impact: a concurrent symlink/junction swap can copy an otherwise unapproved readable file into `chatgpt_project_sources/`, where it becomes transaction-managed content. Fails ANA-11 and AI-06.

### Medium — Existing-import analysis is not bound to its project root and scan at planning time

The common request validator requires a root and scan ID only for maintenance reanalysis, not `existing_project_import` (`src-tauri/src/codex.rs:566-590`). Core evidence approval likewise checks root/scan only for maintenance (`src-tauri/src/commands.rs:749-775`). The pending analysis retains the binding (`src-tauri/src/commands.rs:687-695`), but confirmation does not verify it (`src-tauri/src/commands.rs:811-834`), persisted-record comparison deliberately removes it (`src-tauri/src/commands.rs:2107-2124`), and `build_plan` reads the current project root independently without comparing it to the pending analysis (`src-tauri/src/commands.rs:2154-2160`). Update reanalysis has the missing comparison (`src-tauri/src/commands.rs:2671-2737`), demonstrating the intended boundary.

Impact: a confirmed import analysis can be reused after the selected project root changes in the same core session. The later dry run reduces mutation risk but does not restore analysis provenance. Fails AI-03 and the core-owned scan-binding requirement behind ANA-10/ANA-14.

### Medium — App Server turn correlation accepts events with no correlation identifiers

`event_matches_turn` returns true when an event contains neither thread nor turn IDs (`src-tauri/src/codex.rs:541-555`). `drain_notifications` accepts such queued/live events and stops on schema-shaped output or any `*/completed` notification (`src-tauri/src/codex.rs:461-498`). The current tests reject explicitly different IDs but intentionally accept identifier-free item notifications.

Impact: stale or unrelated schema-shaped output can terminate and satisfy the active turn. Output-schema validation still runs, but it cannot prove the result belongs to the requested thread/turn. This is a protocol state-machine risk for ANA-13 and AI-03.

### Medium — Flattened output is not regenerated or explicitly validated during update/readiness

Initial setup builds flat artifacts from current prepared/generated inputs (`src-tauri/src/commands.rs:2452-2459`). Update instead carries every old generated lock entry forward unchanged (`src-tauri/src/commands.rs:2910-2922`) and creates a maintenance plan with no generated artifacts (`src-tauri/src/commands.rs:3159-3177`). Thus an updated skill, adapted `AGENTS.md`, or `README.md` can diverge from its old flat copy. Neither transaction readiness nor installed readiness has a flatten completeness/freshness check (`src-tauri/src/transaction.rs:1494-1735`, `src-tauri/src/readiness.rs:261-610`), while Ready displays “ChatGPT Chat sources prepared” solely from renderer state (`src/App.tsx:1282-1341`).

Impact: update can succeed and readiness can present stale or incomplete Chat sources as prepared. Fails AI-05, AI-07, and AI-08's honest-final-state requirement.

### Medium — Provider/profile persistence and migration are permissive and incomplete

Project state is generated only for a new project or when portrait interest is selected (`src-tauri/src/commands.rs:2390-2439`), so a normal existing import can leave provider/model/profile state absent or stale. The project-state schema does not require `ai`, and the plan/lock schemas do not require any provider/model/profile/flatten fields (`schemas/project-state.schema.json:7-16`, `schemas/installation-plan.schema.json:7-20`, `schemas/installation-lock.schema.json:7-19`). Rust silently defaults absent fields to Codex (`src-tauri/src/models.rs:653-664`, `src-tauri/src/models.rs:762-773`), and lock migration infers a profile from a possibly absent provider (`src-tauri/src/migrations.rs:217-248`). `validate_plan` validates the analysis record but does not cross-check its provider/model/profile against the plan (`src-tauri/src/transaction.rs:23-61`).

Impact: schema-valid or migrated artifacts can silently acquire Codex defaults or contain inconsistent top-level and analysis metadata. Fails AI-02, AI-03, and ANA-19.

## Confirmed controls

- Codex remains the default and its UI has no OpenAI API-key fallback (`src/App.tsx:77-90`, `src/App.tsx:1022-1026`).
- Initialize precedes account/thread requests; account read, browser/device login, cancellation, rate limits, logout, liveness restart, and local-state clearing are implemented (`src-tauri/src/codex.rs:229-341`, `src-tauri/src/commands.rs:294-328`, `src-tauri/src/commands.rs:575-698`).
- Codex turns use `approvalPolicy: "never"`, read-only restricted sandboxing, no readable project roots, and the checked-in output schema (`src-tauri/src/codex.rs:378-458`, `src-tauri/src/codex.rs:680-688`).
- Non-Codex endpoint scheme/userinfo/query validation, no redirects, and bounded response parsing are present (`src-tauri/src/ai.rs:108-164`, `src-tauri/src/ai.rs:280-368`).
- Nominal flatten mapping includes `<skill>.md`, selected subagent TOML, adapted `AGENTS.md`, `README.md`, and user extras (`src-tauri/src/flatten.rs:19-180`).
- Recovery/rollback/removal commands do not depend on ChatGPT sign-in; the signed-out recovery entry remains visible (`src/App.tsx:1028`, `src-tauri/src/commands.rs:3393-3492`).

## Missing failure/interruption tests

- Cross-provider reference injection/reuse/deletion and proof that no provider credential reference enters renderer state.
- Exact preview-versus-transmitted input, user redaction/exclusion, digest agreement, and non-Codex preview labeling.
- Rejection of PATH shims plus real child startup, graceful/forced shutdown, crash/restart, mid-turn exit, and session-expiry behavior.
- Existing-import root/scan replay and identifier-free stale notification rejection.
- Hosted fake-server tests for auth-header isolation, redirect refusal, timeout, oversized/chunked responses, malformed status bodies, and provider/model/profile switching.
- Required-file and extra-file link-swap races, file/aggregate limits, binary secret-shaped content, and all selected skills/subagents/extras.
- Generated-file replacement followed by rollback and inverse rollback.
- Update-time flat regeneration, skipped/conflicted flat files, readiness freshness, and false-ready UI states.
- Existing-import provider state persistence; missing/mismatched provider/model/profile migration and schema rejection.
- Browser callback failure followed by device-code fallback, desktop system-browser interruption, macOS vault/process behavior, and signed-out recovery desktop E2E.

## Tests run

- `pnpm test`: passed, 2 files / 18 tests.
- `pnpm validate`: passed, 12 integrity groups.
- `cargo test --workspace --all-features` under the x64 Visual Studio environment: passed, 130 tests. The first invocation exhausted its 60-second compile budget as the test process started; the immediate rerun after compilation passed and is the valid result.
- Not run: real-account/provider network tests, desktop E2E, system-browser interaction, macOS execution, or adversarial filesystem race tests.

## Data-leak and protocol summary

The concrete leak paths are: submitting another provider's vault key to a selected endpoint, executing a PATH lookalike with access to Codex account locations and approved prompts, transmitting immutable scan text after the user apparently edited it, and importing an outside-root file during a flatten link swap. The principal state-machine risks are import-analysis replay across roots/scans, accepting uncorrelated App Server events, inverse rollback metadata disagreement, and stale flat artifacts surviving update/readiness.

## Bounded recommended fix order

1. Make provider references core-only and provider-bound; reject renderer references and verify ownership before read/delete. Establish one canonical reviewed Codex executable identity.
2. Render and approve the exact provider-neutral input object; make exclusions/redactions alter the core-approved bytes and digest before transmission.
3. Require project-root/scan correlation for every existing-project analysis and require positive thread/turn correlation for terminal App Server events.
4. Derive rollback action from destination existence/action, then add generated replacement → rollback → inverse fault tests.
5. Read flatten inputs through no-follow, identity-checked handles; regenerate flat artifacts during update and add an explicit completeness/freshness readiness check.
6. Persist provider/model/profile state for imports, require AI/flatten fields in current schemas, validate record-to-plan/lock binding, and add legacy/mismatch migrations.
7. Add the missing fake-process, fake-provider, race, desktop, and macOS interruption tests before rerunning this audit gate.
