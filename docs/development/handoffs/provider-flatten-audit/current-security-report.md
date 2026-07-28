# Provider and flattened-source current security audit

Date: 2026-07-28
Audit type: read-only current-worktree review
Baseline HEAD: `ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70`
Worktree state: dirty; this report evaluates the working-tree bytes, not HEAD alone.

## Verdict

**FAIL**

The current snapshot has strong baseline controls for OS-vault credential storage, provider scoping, bounded HTTP responses, exact source revisions and hashes, path syntax, process argument arrays, output redaction, App Server protocol framing, and least-privilege GitHub Actions permissions. It is not ready to claim the requested security properties because three high-severity boundaries remain unsafe or unproven:

1. executable trust is derived from mutable `PATH`/environment resolution rather than an independently trusted executable identity;
2. flattened-source reads do not establish race-resistant containment for ancestor directories;
3. the 3D health check hashes a project script and later asks Python to open that path again while carrying `MESHY_API_KEY`.

There are also four medium findings concerning approved-evidence provenance, incomplete secret-pattern coverage, over-broad Meshy reference persistence, and release signing/notarization gates.

No race-proof claim is made in this report. The two race findings are code-review findings that remain unproven by adversarial tests; absence of those tests is itself a release-blocking evidence gap.

## Threat model and scope

### Protected assets

- hosted-provider API keys;
- `MESHY_API_KEY`;
- Codex/ChatGPT tokens and account metadata owned by Codex;
- project and installation-plan/lock confidentiality;
- files outside the selected project root;
- exact source revision and payload integrity;
- release signing, notarization, and publication secrets.

### Considered attackers and failures

- a compromised or malicious renderer sending crafted command arguments;
- a malicious existing mod tree containing links, junctions, reparse points, collisions, oversized files, or credential-shaped content;
- concurrent local mutation of project paths during validation or read/execute;
- a normal-file executable shim earlier on `PATH`, or an untrusted executable selected through mutable process environment;
- a malicious or malformed hosted-provider/source response, including redirects and oversized bodies;
- stale or renderer-created semantic-analysis evidence;
- pull requests or release jobs attempting to obtain broader GitHub permissions or release secrets.

### Files reviewed

The review covered the user-named sources and directly relevant App Server, persistence, schema, manifest, test, and workflow evidence:

- `AGENTS.md`, `SECURITY.md`, `GOAL_PROMPT.md`;
- `docs/13_security_model.md`, `docs/30_codex_chatgpt_authentication.md`, `docs/31_ai_provider_profiles_and_chat_sources.md`;
- `.agents/skills/hoi4-mod-setup-security/SKILL.md`;
- `src-tauri/src/security.rs`, `credentials.rs`, `ai.rs`, `flatten.rs`, `mcp.rs`, `process.rs`, `source.rs`, `commands.rs`;
- directly relevant `codex.rs`, `models.rs`, `migrations.rs`, and `transaction.rs` boundaries;
- installation plan, lock, project-state, remote-manifest, and Codex-analysis schemas;
- `source-manifest/hoi4-mod-setup.manifest.json`;
- relevant Rust tests and release/security workflow scripts;
- `.github/workflows/ci.yml`, `security.yml`, and `release.yml`.

The Git command implementation and support-bundle implementation were not named in the parent scope and were not source-reviewed. The all-feature Rust run did execute the repository's Git tests, but that is not enough evidence to pass hooks, config, remotes, or support-bundle redaction. No live provider, GitHub, OS-vault, code-signing, notarization, or adversarial race test was performed.

## Findings by severity

### High H-01 — executable allowlisting is self-referential, not identity-based

Status: **Fail**

The core accepts the first normal, link-free `codex.exe`/`codex` found on inherited `PATH`; it does not verify an app-owned installation root, publisher signature, package identity, or immutable hash (`src-tauri/src/codex.rs:1470-1490`). `with_codex_session` then calls that binary “official,” starts it, and completes the App Server initialize handshake (`src-tauri/src/commands.rs:295-316`).

This process receives `HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `CODEX_HOME`, and XDG data/config locations after `env_clear` (`src-tauri/src/codex.rs:1293-1333`). Those variables are needed by real Codex, but they make executable identity part of the authentication boundary. A merely regular PATH entry is not sufficient identity evidence.

The generic resolver has the same weakness: it accepts the first regular, link-free named file on inherited `PATH` (`src-tauri/src/process.rs:260-299`). The 3D check resolves `python.exe`/`python` this way, then constructs its allowlist from that same resolved path and injects `MESHY_API_KEY` (`src-tauri/src/commands.rs:1193-1212`). The `ProcessSpec` comparison proves only that the process equals the just-selected path, not that the selected interpreter is trusted (`src-tauri/src/process.rs:34-77`).

The dormant MCP route also resolves `cmd.exe` and Node through the generic PATH resolver (`src-tauri/src/mcp.rs:168-190`). The current checked-in manifest does not provide the required MCP executable hash/size, so `manifest_target` fails closed and `mcp::tests::manifest_target_requires_the_source_declared_rule` passes (`src-tauri/src/mcp.rs:41-96`, `src-tauri/src/mcp.rs:325-330`). That current unavailability limits immediate exposure but does not make the implementation safe if immutable wrapper evidence is later added.

Windows browser and process-tree helpers derive `explorer.exe` and `taskkill.exe` from inherited `SystemRoot` and check only file/link properties (`src-tauri/src/process.rs:203-227`, `src-tauri/src/process.rs:303-332`). This is weaker than a fixed OS-owned identity.

Impact:

- an untrusted normal-file Codex shim can impersonate App Server and receive approved prompts while running with access to Codex authentication locations;
- an untrusted Python shim can receive `MESHY_API_KEY`;
- enabling the MCP route would still leave interpreter/runtime identity dependent on PATH.

Required remediation:

- resolve Codex from a core-owned, platform-specific installation record and verify its publisher/package identity or an immutable approved digest before every start;
- resolve Python from a user-reviewed canonical tool record whose identity is persisted and revalidated, not from ambient PATH at secret-bearing execution time;
- use fixed, canonical OS locations plus platform identity checks for `explorer.exe`, `taskkill.exe`, and `cmd.exe`;
- require immutable identity evidence for every executable in an MCP launch chain, including Node where applicable;
- keep argument arrays, bounded roots, timeouts, and output limits unchanged.

Required regression tests:

- place a regular, non-linked `codex.exe` earlier on PATH and prove account/login/analyze/open routes reject it before spawn;
- place a regular, non-linked Python shim earlier on PATH and prove it never receives `MESHY_API_KEY`;
- alter the executable after review and prove launch fails closed;
- on Windows, prove `SystemRoot` and PATH manipulation cannot redirect browser, termination, or MCP interpreter selection;
- retain the current test proving MCP remains unavailable when immutable executable evidence is missing.

### High H-02 — flattened reads are leaf-no-follow but not proven ancestor-race-resistant

Status: **Fail; race resistance unproven**

The flatten implementation has useful controls:

- 512 files, 8 MiB per file, and 64 MiB aggregate limits (`src-tauri/src/flatten.rs:20-23`, `src-tauri/src/flatten.rs:165-179`);
- path normalization and contained joins;
- pre/post link-component checks;
- Unix `O_NOFOLLOW` and Windows `FILE_FLAG_OPEN_REPARSE_POINT` on the leaf open;
- metadata and size checks on the same open handle (`src-tauri/src/flatten.rs:273-318`);
- secret-like path rejection and case-insensitive output collision detection (`src-tauri/src/flatten.rs:335-397`).

These controls block ordinary leaf symlinks/reparse points. They do not establish race-resistant ancestor containment. Skill and subagent directories are enumerated through path-based `read_dir`, `symlink_metadata`, and later leaf opens (`src-tauri/src/flatten.rs:196-269`). The before/after component walk is also path-based (`src-tauri/src/security.rs:488-505`). A transient ancestor substitution during the open can therefore resolve a different object even if the path is restored before the post-check. `O_NOFOLLOW` and `FILE_FLAG_OPEN_REPARSE_POINT` apply to the final component, not every ancestor.

The flatten tests cover expected mapping, missing inputs, traversal input, collisions, a labeled secret, binary input, and `.env` rejection (`src-tauri/src/flatten.rs:412-558`). They do not contain a symlink/junction test or an adversarial ancestor-swap test.

Impact:

- content outside the selected root could be copied into `chatgpt_project_sources` and later persisted or committed.

Required remediation:

- implement handle-relative traversal from a verified project-root directory handle, opening every ancestor without following links/reparse points;
- on Unix, use `openat`/`openat2`-style component traversal with no-follow/beneath semantics where available;
- on Windows, open and validate each directory handle with reparse-point controls and verify final-handle ancestry/volume/file identity;
- enumerate directories through verified handles rather than reopening names;
- fail closed where the platform cannot provide the required containment.

Required regression tests:

- ordinary file symlink and directory symlink rejection on macOS;
- file reparse point and directory junction rejection on Windows;
- a coordinated concurrent ancestor swap during enumeration/open, with an outside-root canary, proving no outside bytes appear in output;
- repeated stress execution of that adversarial test.

Until those tests pass, behavior must be described as link-aware and leaf-no-follow, not race-proof.

### High H-03 — 3D script verification and secret-bearing execution use different opens

Status: **Fail; race resistance unproven**

The 3D health command correctly requires Windows, a selected installed workflow, a re-resolved exact locked manifest, one manifest command target, project containment, a lock-tracked file, expected size/hash, an opaque Meshy reference, scoped environment injection, a ten-minute timeout, and a 2 MiB output bound (`src-tauri/src/commands.rs:1106-1212`).

The integrity check and execution are nevertheless separate operations. The core hashes `target_path` (`src-tauri/src/commands.rs:1170-1178`) and later passes that path to Python (`src-tauri/src/commands.rs:1199-1212`). Python performs a new path lookup/open after `MESHY_API_KEY` has been placed in its environment. A concurrent replacement between those operations can therefore invalidate the checked-bytes-to-executed-bytes binding.

This report does not claim that the race was reproduced. No adversarial replacement/junction test exists, so race-proof behavior is not established.

Impact:

- different project bytes from those verified against the installation lock could execute with `MESHY_API_KEY`.

Required remediation:

- execute a private, core-owned immutable copy created from a no-follow read of the verified bytes;
- create that copy with exclusive creation in application-owned storage, verify its hash after the copy, prevent link traversal in every ancestor, and remove it after supervised completion;
- alternatively use a platform facility that binds execution to a verified handle/identity without reopening a mutable project path;
- revalidate the lock, source revision, script identity, and interpreter identity immediately before spawn.

Required regression tests:

- coordinated regular-file replacement between verification and spawn;
- coordinated ancestor junction/reparse swap;
- mutation after verification but before interpreter open;
- assertions that the replacement never executes and never observes `MESHY_API_KEY`;
- interruption/timeout cleanup of the private execution copy.

### Medium M-01 — renderer-created evidence hashes can be admitted as core-approved scan evidence

Status: **Fail**

The scanner initially stores a core-computed hash of each completed finding value, bound to finding ID and evidence path (`src-tauri/src/commands.rs:939-1011`). `approve_scan_evidence` then accepts renderer-provided excerpt bytes after self-consistency validation and, when the finding ID/path exists, appends any new excerpt hash to the approved set (`src-tauri/src/commands.rs:740-775`). Later analysis checks only that the submitted path/hash is present in that expanded set (`src-tauri/src/commands.rs:780-832`).

`validate_analysis_request` proves that the hash matches the renderer-provided excerpt and rejects forbidden paths, oversized content, and currently recognized secret patterns (`src-tauri/src/codex.rs:610-671`). It does not prove that the excerpt came from the core scan. This violates the core-owned evidence boundary: a compromised renderer can turn arbitrary text into “approved scan evidence” for an existing finding path.

Required remediation:

- approve only exact core-stored scan excerpts/hashes; or
- model user redaction/editing as a typed operation applied by the core to core-stored bytes, with the core deriving and presenting the final approved excerpt/hash.

Required regression tests:

- an arbitrary renderer excerpt with a self-consistent hash and valid finding/path must be rejected;
- a stale scan ID or different project root must remain rejected;
- a permitted core-derived redaction must be bound to the exact displayed bytes;
- confirmation must fail after any evidence byte changes.

### Medium M-02 — runtime and Git secret-pattern coverage is incomplete

Status: **Fail**

`redact_secrets` replaces exact known values and recognizes bearer values, labeled token/code assignments, token query parameters, labeled API keys, labeled Meshy keys, and `msy_` keys (`src-tauri/src/security.rs:393-422`). This is effective for captured Meshy child output because the exact vault value is supplied to redaction (`src-tauri/src/process.rs:164-170`), but flatten and general semantic-input checks call it without known values (`src-tauri/src/flatten.rs:150-160`, `src-tauri/src/flatten.rs:335-350`, `src-tauri/src/codex.rs:615-662`).

Common raw hosted-provider key shapes and unlabeled credential strings are not recognized. Consequently, an API key accidentally placed in a model, endpoint path, evidence excerpt, or flattened text can pass the pattern test. Model and endpoint are then copied into plans/locks as non-secret configuration.

The test named `provider_configuration_rejects_secret_shaped_model_and_endpoint` uses a hosted provider with no credential reference (`src-tauri/src/ai.rs:470-480`). `validate_config` requires that reference later (`src-tauri/src/ai.rs:166-173`), so the test passes even if the raw `sk-...` text was not detected. It does not prove the intended redaction behavior.

The repository scanner passed on this worktree, but its patterns cover private keys, GitHub, AWS, Meshy, and a small assignment list (`scripts/check_committed_secrets.py:40-48`). It does not cover the hosted-provider credential family, scans the current filesystem rather than Git history (`scripts/check_committed_secrets.py:60-88`), and skips binary screenshots/artifacts. Therefore the successful output cannot establish absence from Git history, screenshots, crash artifacts, or all fixtures.

Required remediation:

- add direct, provider-aware credential detectors for every supported hosted profile and high-confidence generic API-key forms;
- test the detector directly rather than relying on a later missing-credential error;
- validate model/endpoint/evidence/flattened bytes with those detectors before serialization or network use;
- add a pinned, reviewed Git-history secret scanner with an explicit placeholder allowlist;
- add crash-report, screenshot, fixture, and support-bundle redaction gates before persistence.

Required regression tests:

- use a valid provider-scoped reference and prove raw provider-key forms are rejected in model, endpoint path, prompt, evidence, provider output, and flattened files;
- prove exact known values are redacted from stdout, stderr, command errors, protocol errors, plans, locks, state, fixtures, and support bundles;
- scan all reachable Git history and fail on seeded historical secrets while allowing documented placeholders;
- test binary/screenshot metadata and crash payload handling.

### Medium M-03 — Meshy opaque references are persisted outside the selected 3D workflow scope

Status: **Fail**

Hosted-provider references are reconstructed inside the Rust core and are not returned by credential commands (`src-tauri/src/commands.rs:514-557`, `src-tauri/src/commands.rs:688-714`). That provider-key scope is a pass.

Meshy references are non-secret opaque UUID references and are valid project metadata when the 3D workflow is selected (`src-tauri/src/credentials.rs:193-223`). The current plan builder, however, collects the in-memory Meshy reference without first checking that `workflow.3d` is selected and writes it into generated project state and the installation plan (`src-tauri/src/commands.rs:2529-2573`, `src-tauri/src/commands.rs:2734-2757`).

Lock construction then assigns the first Meshy reference to every optional workflow entry, including unrelated workflows such as flattened Chat sources (`src-tauri/src/transaction.rs:2049-2075`). Schemas and migration checks constrain the name but accept a broad `credential://...` string rather than the exact Meshy UUID namespace (`schemas/installation-plan.schema.json:134-145`, `schemas/installation-lock.schema.json:199-230`, `schemas/project-state.schema.json:73-100`, `src-tauri/src/migrations.rs:136-179`).

No secret value is exposed, but the reference reveals cross-project credential presence and is bound to unrelated workflow state, violating least-scope persistence.

Required remediation:

- include a Meshy reference only when `workflow.3d` is selected;
- write it only to the `workflow.3d` lock entry;
- enforce `^credential://meshy_api_key/<uuid>$` in plan/state/lock schemas and migration validation;
- remove unrelated or stale references during maintenance migration.

Required regression tests:

- no 3D selection means no Meshy reference in plan, state, or lock even when the vault contains a key;
- selecting flatten/Lora/MCP never acquires a Meshy reference;
- only `optional_workflows["workflow.3d"]` may hold the reference;
- malformed namespaces, non-UUID values, wrong providers, and cross-platform references fail closed.

### Medium M-04 — release publication conditions do not prove signing or notarization

Status: **Fail**

Workflow secret isolation is otherwise sound:

- default workflow permissions are `contents: read` (`.github/workflows/release.yml:9-10`);
- release is triggered only by tags or manual dispatch, not pull requests (`.github/workflows/release.yml:3-7`);
- the only `contents: write` permission and `GH_TOKEN` use are in the protected `release` environment draft job (`.github/workflows/release.yml:89-104`);
- CI and security pull-request workflows use `contents: read` and do not reference repository secrets (`.github/workflows/ci.yml:3-10`, `.github/workflows/security.yml:3-23`);
- third-party actions are pinned to full commit SHAs.

The build jobs receive no signing/notarization secrets and do not use a protected release environment. `release_build.mjs` records signing as configured solely from `HOI4_MOD_SETUP_SIGNING_CONFIGURED` (`scripts/release_build.mjs:87-96`). `release_verify.mjs` enforces signing only when `HOI4_MOD_SETUP_REQUIRE_SIGNING=1` (`scripts/release_verify.mjs:71-73`), but the workflow sets only `HOI4_MOD_SETUP_REQUIRE_TAURI=1` (`.github/workflows/release.yml:75-79`). The draft job trusts mutable repository variables saying publishing/signing are configured (`.github/workflows/release.yml:89-95`) without verifying the downloaded artifacts' platform signatures or notarization evidence.

This does not expose a current signing secret—none is supplied—but it allows the release path to treat unsigned/unnotarized artifacts as signing-configured based on metadata/variables rather than artifact evidence.

Required remediation:

- place platform signing/notarization jobs in protected, platform-specific release environments;
- expose each secret only to the minimum signing step and never to PR/fork jobs;
- set `HOI4_MOD_SETUP_REQUIRE_SIGNING=1` for release verification;
- verify actual Windows Authenticode and macOS signing/notarization/stapling evidence before upload and before draft creation;
- make draft/publication depend on those verified outputs, not a repository variable.

Required regression tests:

- unsigned Windows and macOS packages fail the release workflow;
- a metadata-only “configured” flag cannot satisfy the gate;
- valid signatures/notarization are checked against expected identities;
- fork PRs and ordinary CI runs receive no release environment or signing secrets;
- release-environment denial prevents draft creation.

## Pass evidence and residual risk

| Surface | Result | Evidence and residual risk |
|---|---|---|
| Hosted-provider key scope | Pass | OS-vault account is deterministically provider-scoped (`credential://ai_provider_api_key/{provider}`), and cross-provider reuse is rejected (`src-tauri/src/credentials.rs:114-134`, `src-tauri/src/credentials.rs:225-277`; tests at `src-tauri/src/credentials.rs:419-444`). The renderer receives only boolean success/status. |
| Meshy value storage | Pass for value storage | The manifest declares only the name `MESHY_API_KEY` and OS vault storage (`source-manifest/hoi4-mod-setup.manifest.json:5062-5069`). Only that name can be placed into `ScopedSecretEnvironment` (`src-tauri/src/credentials.rs:311-339`). Reference scoping still fails M-03. |
| Scoped process environment | Pass except executable identity/race | Child execution uses argument arrays, `env_clear`, declared environment names, timeout, output cap, and known-value redaction (`src-tauri/src/process.rs:90-172`). Tests prove the Meshy value is visible to the approved child and redacted from captured output (`src-tauri/src/process.rs:398-442`). H-01 and H-03 remain blocking. |
| Hosted endpoint validation | Pass by code review | Models/endpoints are bounded; hosted routes require HTTPS; local routes require literal loopback HTTP; userinfo, query, and fragment are rejected (`src-tauri/src/ai.rs:108-174`). |
| Provider response control | Pass by code review; tests missing | Redirects are disabled, timeout is 120 seconds, responses are capped at 4 MiB using both declared and streamed length, non-success bodies are not returned, and JSON is schema-validated later (`src-tauri/src/ai.rs:290-378`). No fake-server redirect/chunk/slow-response integration test exists. |
| Path syntax and collisions | Pass for deterministic checks | Relative paths reject absolute forms, traversal, ADS, reserved names, excessive depth/length, and canonical case collisions (`src-tauri/src/security.rs:11-102`). Manifest destinations and flattened output names are case-normalized. Race resistance remains failed under H-02/H-03. |
| Archive limits | Not applicable to the current route | `SourceKind` permits only `file`, `tree`, and `generated` (`src-tauri/src/models.rs:98-104`); no archive extraction route was found. Archive count/ratio/depth controls must be implemented before any archive source kind is added. |
| Manifest and source trust | Pass for the current selective-download route | Latest/release modes resolve to an exact verified commit; manifest and file requests use that revision; redirects are capped at three approved HTTPS endpoints; final URLs and body sizes are checked (`src-tauri/src/source.rs:80-152`, `src-tauri/src/source.rs:182-371`). Manifest revision equality, source identity, component paths, duplicate destinations, file evidence, 20,000-file/1 GiB aggregate limits, and per-file hashes/sizes are enforced (`src-tauri/src/source.rs:515-610`, `src-tauri/src/source.rs:1004-1193`). Download bytes are reverified after cache return (`src-tauri/src/commands.rs:2361-2371`). |
| App Server protocol boundary | Pass except executable identity | Initialize precedes account/thread calls; login uses only `chatgpt` or `chatgptDeviceCode`; logout uses `account/logout` (`src-tauri/src/codex.rs:230-339`). Analysis requires a ChatGPT account, no usage limit, read-only sandbox, `approvalPolicy=never`, approved inputs, and the current output schema (`src-tauri/src/codex.rs:380-417`, `src-tauri/src/codex.rs:698-723`). |
| No Codex API-key/token-file/external-token route | Pass by source search | No Codex token-file read exists in `codex.rs`; the only Codex account API-key occurrences are rejection/validation tests and provider-record compatibility, not a Codex fallback. Login tests verify no `apiKey` parameter (`src-tauri/src/codex.rs:1766`). H-01 means an impersonating executable would still violate the intended boundary. |
| Protocol logging/redaction | Pass by code review | App Server stderr is discarded, JSONL lines are capped at 2 MiB, and request errors expose method/code rather than raw payloads (`src-tauri/src/codex.rs:342-372`, `src-tauri/src/codex.rs:1293-1299`, `src-tauri/src/codex.rs:1388-1447`). No raw protocol persistence was found. |
| Account metadata persistence | Pass | Email/plan/usage are transient account-status values (`src-tauri/src/codex.rs:1029-1073`). Analysis records skip project root and scan ID during serialization and require `account_identity_persisted=false` (`src-tauri/src/models.rs:1051-1082`, schemas). Migrations reject account/token keys (`src-tauri/src/migrations.rs:183-235`). |
| Signed-out recovery | Pass | Removal skips provider authentication (`src-tauri/src/commands.rs:3207-3236`), while rollback, journal inspection, interrupted-transaction discovery, resume, and staging recovery are local commands (`src-tauri/src/commands.rs:3976-4065`). |
| GitHub Actions fork/permission boundary | Pass | CI/security PR jobs have read-only contents permission and no secret references; release write permission is job-local and environment-protected. M-04 blocks signing assurance. |
| Git hooks/config/remotes | Insufficient scoped evidence | The all-feature suite passed Git tests including no-push and remote-preservation behavior, but the Git implementation source was outside the parent-named file set. Do not infer a full Git-safety pass from those tests. |
| Support bundles | Insufficient scoped evidence | No parent-named support-bundle implementation or test was reviewed. Redaction of bundle logs, paths, account metadata, and screenshots remains an explicit missing test. |

## Credential leakage checks

- **Current worktree pattern scan:** pass — `python scripts/check_committed_secrets.py` reported `No committed secret patterns found.`
- **Provider vault use:** pass — hosted keys and Meshy values are stored through OS keyring APIs; command responses do not return values.
- **Plans/locks/state:** direct provider values were not found; account fields are schema/migration-rejected. Meshy references are opaque but over-propagated under M-03.
- **Manifest:** contains only the Meshy environment-variable name and storage policy, not a value.
- **Process output:** exact Meshy value is supplied to redaction before stdout/stderr are returned.
- **Provider HTTP errors:** response bodies are not included in errors; only HTTP status is exposed.
- **App Server logs:** raw JSONL and stderr are not persisted by the named implementation.
- **Fixtures/current files:** only the limited pattern scanner was run; this is not proof for all provider-key formats.
- **Git history:** not established. The current scanner walks current files and skips `.git`; it is not a history scan.
- **Crashes/screenshots/support bundles:** not established by the named source or tests.

## Filesystem and process checks

Deterministic containment checks are generally well designed: reserved Windows names, alternate data streams, absolute paths, traversal, depth, segment length, case collisions, link/junction metadata, root-relative joins, working-root validation, command arrays, timeout limits, output bounds, and environment clearing are present.

The security result is still a fail because:

- path checks followed by path-based opens cannot be described as race-proof without handle-relative traversal and adversarial tests;
- hashing a path and later asking an interpreter to reopen it does not bind execution to the verified bytes;
- an allowlist populated from the same mutable PATH resolution does not establish executable trust.

The external-action plan correctly shows executable description, arguments, working directory, environment-variable names, network/write/privilege declarations, risk, and approval (`src-tauri/src/commands.rs:1645-1719`). `contains_secret=false` means no secret value is serialized, while `environment_names` shows scoped injection. Renaming that field to `contains_serialized_secret_value` would reduce ambiguity, but no secret value was observed in the action object.

No privilege-elevation route was found in the named process code. `network_access`, `expected_writes`, `privilege`, and rollback boundary are often `not_declared`; that is honest but incomplete dry-run evidence and should remain visibly incomplete rather than inferred safe.

## Supply-chain and workflow checks

The source route passes the core immutable-source properties:

- no full repository clone;
- latest resolves the default branch to an exact commit before manifest download;
- pinned commits and release tags resolve to verified commit objects;
- one revision is used for manifest/tree/files;
- only manifest-selected files are fetched;
- final bytes are checked against manifest SHA-256 and size;
- cache entries are hash/size checked, and returned bytes are reverified before plan use;
- unapproved redirect targets and truncated trees fail closed;
- unsupported manifest majors, duplicate destinations, missing evidence, platform mismatch, and checksum mismatch fail closed.

The checked-in manifest is itself bound to `generated_for_revision` at `source-manifest/hoi4-mod-setup.manifest.json:1-12`; its working-tree SHA-256 is recorded below. The MCP command intentionally remains unavailable because its current manifest validation rule lacks immutable executable hash/size evidence (`source-manifest/hoi4-mod-setup.manifest.json:1846-1897`).

Workflow permissions and fork behavior pass, but actual release signing/notarization evidence fails M-04. The secret scanner also needs Git-history and hosted-provider coverage under M-02.

## Missing tests required before a passing audit

1. Trusted executable identity tests for Codex, Python, `cmd.exe`, Node, browser opener, and process-tree termination.
2. Regular-file PATH shim rejection before Codex App Server or Meshy-bearing process spawn.
3. Windows junction/reparse and macOS symlink tests for every flattened ancestor and leaf.
4. Coordinated ancestor-swap flatten test with an outside-root canary.
5. Coordinated 3D script replacement/junction tests between hash and interpreter open.
6. Core-owned evidence provenance test rejecting a renderer-created excerpt/hash.
7. Provider endpoint integration tests with a fake server for redirects, chunked oversized bodies, false/missing content length, slow responses, non-success body suppression, and invalid JSON/schema output.
8. Direct raw-provider-key detection tests with an otherwise valid provider credential reference.
9. Serialization property tests proving no secret/account corpus appears in plans, locks, state, journals, errors, logs, crash payloads, screenshots, fixtures, or support bundles.
10. Meshy reference tests proving it appears only for selected `workflow.3d` and only in that lock entry.
11. Full Git-history secret scanning with seeded historical-secret and placeholder cases.
12. Release tests that inspect real Authenticode and macOS signing/notarization evidence and prove fork jobs cannot access release environments.
13. A negative schema/runtime test proving archive source kinds remain rejected until archive count, size, ratio, depth, and path limits exist.
14. Git hook/config/remote safety tests against the implementation source, including malicious local config and unsafe command arguments.

## Validation performed

| Command/check | Result |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml --all-features -- --test-threads=1` | Pass: 144 passed, 0 failed, 0 ignored; 49.73 seconds |
| `python scripts/check_committed_secrets.py` | Pass: no current-worktree pattern matches |
| `python scripts/validate_repository_templates.py` | Pass: 12 integrity groups |
| local `release:build`, followed by `HOI4_MOD_SETUP_REQUIRE_TAURI=1` release verification | Verification failed closed because the local artifact was frontend-only; this is not signing/notarization evidence |
| `git diff --check` | Pass; one CRLF normalization warning, no whitespace error |

Not run:

- adversarial filesystem race tests;
- live OS credential manager/keychain integration;
- live hosted-provider or App Server login;
- live GitHub source resolution;
- Windows/macOS package signing or notarization;
- full Git-history secret scan;
- support-bundle/crash/screenshot redaction.

## Snapshot hashes

These hashes identify the most security-relevant working-tree bytes reviewed:

| File | SHA-256 |
|---|---|
| `src-tauri/src/security.rs` | `d6d977a76f5c34fb60227a787bfd4ef72ec1ca4e846f039e4c6c9c17bea6819a` |
| `src-tauri/src/credentials.rs` | `466750630a0f4e997ab821974afef6bd6b98189be2d11716846999cf5d25d6ab` |
| `src-tauri/src/ai.rs` | `b01027dd092bda01f112c64fadbfd5f5413ce34e66342356f2beade30ecf3429` |
| `src-tauri/src/flatten.rs` | `1a9b6f24f7c27615b0f7bc9f7cda8d1ab8792ac18626dd30bf839f946712ac31` |
| `src-tauri/src/codex.rs` | `f54d4362de38054a9e601e84f11d56dbe2eb9f079c59636fb055cdcf9f03d95c` |
| `src-tauri/src/mcp.rs` | `e0c140dd01df2489289ee445e23864498eae6b2cd8a6c20d1c4d7fdc213a0764` |
| `src-tauri/src/process.rs` | `3531cc1778a79bcce0e49d763556674f47cfec6c8184c6b967ed7200525714f3` |
| `src-tauri/src/source.rs` | `64d613f5b3220bbd3023440e2db656da894d33d02b72fbacb329aac2028e7125` |
| `src-tauri/src/commands.rs` | `555d94501a1a40453a6791d5f918539d6fd2bb9037d215705fb5192dac8a3f20` |
| `src-tauri/src/transaction.rs` | `cf7c5a95cb1a45fd4e36905c0e4226a4b0223c342c042b22e0b55b97d21b053e` |
| `schemas/installation-plan.schema.json` | `eb758c5e5b41d250bc6a566011bb008023c36b6d318da36c2776478487ca2918` |
| `schemas/installation-lock.schema.json` | `6ace833bacfcda541ab78cec49aa464a9b30e4ca507f8fed6bb427109800628e` |
| `schemas/project-state.schema.json` | `be305e260ec5cb5cc3d8f58e2c3de2c8c27e538df10707017b7f606d9dd51270` |
| `source-manifest/hoi4-mod-setup.manifest.json` | `cddb7ece7235d033888d85508455c255ffe320f0f28bc924999e8f4ddd1c19b5` |
| `.github/workflows/release.yml` | `24817011e82ec74a0094f7b690e332f923c0781a24f10dcb9761e3fc3293f10f` |

## Recommended remediation order

1. Replace ambient executable discovery with independently verified executable identities, starting with Codex and the Meshy-bearing Python route.
2. Bind 3D execution to verified immutable bytes.
3. Rework flattened reads around verified directory handles and add adversarial Windows/macOS race tests.
4. Make semantic evidence provenance core-owned.
5. Expand runtime/Git secret detection and correct the false-positive provider configuration test.
6. Scope Meshy references only to the selected 3D workflow and tighten schemas/migrations.
7. Add real signing/notarization verification and protected signing jobs.
8. Complete the missing Git, support-bundle, crash, screenshot, and history evidence before changing the verdict.
