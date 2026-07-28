# Provider and flatten security audit — current

Date: 2026-07-28
Mode: bounded read-only audit; no network, source patching, publishing, or live credential/signing exercise
Verdict: **Fail for enabling the MCP route or claiming release readiness.** The checked live manifest intentionally lacks executable identity evidence, and the implementation correctly keeps that MCP route `planned_unavailable`. Provider-vault isolation is substantially sound. Flattening still has an active root-containment race and broader disclosure risks.

## Threat model scope

Reviewed the named credential, redaction, filesystem, process, source-trust, Git/release, support-bundle, and Codex App Server boundaries. The attacker model includes a hostile project tree, manifest, provider response, inherited environment/PATH entry, executable or wrapper replacement, symlink/junction race, archive-like source expansion, pull request, and release runner. Account takeover, compromised operating-system credential stores, and compromised GitHub-hosted runners are outside this code audit.

Evidence was inspected in `AGENTS.md`, `SECURITY.md`, `.codex/agents/hoi4setup_security_auditor.toml`, the security skill and model, the named Rust modules and tests, schemas/migrations, release scripts, `.github/workflows`, and the AGENTS-defined live manifest. Tests were inspected but not rerun because this refresh permits writing only this report.

## Findings

### High — H1: executable identity is not bound across the actual process/MCP launch chain

The MCP plan now requires exact wrapper, interpreter, and runtime hashes/sizes (`src-tauri/src/mcp.rs:34-132`, `138-211`) and validates all three before launch (`237-270`). This is a material improvement. However, only `cmd.exe` is re-hashed immediately before the parent spawn (`src-tauri/src/codex.rs:1293-1371`). The previously checked wrapper and Node paths are subsequently reopened by `cmd.exe` and the wrapper, so replacement between check and descendant open is not prevented. The launch therefore proves three path snapshots, not that the descendants executed those verified bytes.

The active Codex route has the related source-trust weakness: it selects the first link-free `codex` candidate from inherited PATH and then hashes that same selected file (`src-tauri/src/codex.rs:1492-1512`, transport enforcement at `1283-1313`). Generic resolution similarly accepts a normal PATH file and relies on caller-supplied hash identity (`src-tauri/src/process.rs:39-58`, `275-316`). This is integrity checking after ambient selection, not independent publisher/package or pinned-source trust.

Current exposure is contained: the live and bundled manifests are byte-identical (SHA-256 `cddb7ece7235d033888d85508455c255ffe320f0f28bc924999e8f4ddd1c19b5`, source revision `27128a7b311d728a959afff7238a9aeeb9987f2b`) and intentionally contain no wrapper/interpreter/runtime identity parameters. Resolution fails closed as unsupported, and command, plan, lock, and readiness paths map that state to non-blocking `planned_unavailable` (`src-tauri/src/commands.rs:1307-1320`, `1349-1439`; `src-tauri/src/transaction.rs:1813-1824`, `2192-2234`; `src-tauri/src/readiness.rs:1089-1110`). It must remain so.

Missing regression evidence: a successful Windows wrapper → `cmd.exe` → Node chain using independently trusted identities; replacement after validation; a regular-file PATH shim with otherwise valid metadata; and descendant/process-tree termination. Existing MCP tests cover absent evidence and unreviewed paths only (`src-tauri/src/mcp.rs:415-452`).

Priority fix: keep MCP unavailable until launch uses independently trusted executable provenance and closes the check/use gap—for example, an immutable private verified copy or equivalent handle-bound execution for every executable component. Bind the active Codex executable to an installer/package/publisher or pinned artifact identity rather than hashing the first PATH result. Retain argument arrays, clean scoped environment, bounded output, timeout, and tree termination already present in `src-tauri/src/process.rs:105-248`.

### High — H2: flatten leaf no-follow checks do not contain ancestor replacement races

`read_regular_file_no_follow` performs pre/post ancestor checks and opens the leaf with Unix `O_NOFOLLOW` or Windows `FILE_FLAG_OPEN_REPARSE_POINT` (`src-tauri/src/flatten.rs:273-322`). Ancestors are still resolved by path during open. A concurrently replaced ancestor directory/junction can redirect the read outside the selected root and be restored before the post-check. The same-handle leaf metadata checks do not bind the ancestor walk.

The test suite has only a Unix static linked-ancestor case (`src-tauri/src/flatten.rs:563-603`). It lacks a Windows junction/reparse case and a coordinated ancestor-swap race.

Priority fix: walk from an opened root using handle-relative, no-follow component opens (or a platform-equivalent containment primitive), and reject any component whose identity changes. Add adversarial Unix symlink and Windows junction/reparse race tests that prove no outside bytes are returned.

### Medium — M1: flatten can collect unselected content and applies some bounds too late

Existing-agent collection enumerates every direct `.agents/skills/*/SKILL.md` and `.codex/agents/*.toml` (`src-tauri/src/flatten.rs:196-270`) rather than an explicit approved/selected source set. This can disclose unrelated local instructions when the flattened folder is shared. Count and aggregate limits are enforced after this enumeration/read phase (`165-180`), so a hostile tree can consume work beyond the advertised 512-file/64-MiB output limits before rejection.

Path denial covers `.env`, key/certificate suffixes, and credential-shaped segments (`src-tauri/src/flatten.rs:388-400`) but not common variant names such as `.env.*`. Output collision checks lowercase paths (`338-363`) without Unicode normalization, leaving canonical-equivalent collisions relevant to macOS/filesystem portability. Secret content scanning has useful provider-specific patterns (`src-tauri/src/security.rs:393-429`) but cannot prove arbitrary unknown-key redaction.

Existing tests cover traversal, lowercase collision, labeled secrets, binary input, `.env`, and basic limits (`src-tauri/src/flatten.rs:415-561`); they do not cover selection isolation, early-abort stress, secret filename variants, Unicode normalization collisions, or unknown provider-key fixtures.

Priority fix: derive a reviewed allowlist of selected source files, enforce count/byte/depth limits before and during traversal/read, normalize portable output names consistently, and expand high-risk filename handling. Add fixtures proving unselected sources never enter output.

### Medium — M2: release gates fail closed but the signed release route is not operational or revision-bound

Workflow permissions are least-privilege by default, pull-request jobs do not receive release secrets, external actions are commit-SHA pinned, and artifact upload depends on verification (`.github/workflows/ci.yml`; `security.yml`; `release.yml:1-119`). Release verification requires Tauri artifacts and platform signing (`release.yml:78-83`) and performs Authenticode or codesign/notarization checks (`scripts/release_verify.mjs:73-129`).

The build jobs do not configure a signing identity, signing/notarization secrets, or a signing command; build metadata therefore records signing as not configured (`scripts/release_build.mjs:96`). Current release builds should fail before upload, which is safe but means the release route is unavailable. In addition, verification accepts a 40-hex metadata revision (`scripts/release_verify.mjs:45-51`) without proving equality to the checked-out HEAD, `GITHUB_SHA`, and tag target.

Priority fix: use protected platform-specific release environments, expose secrets only to the signing/notarization step, verify the signed artifact before upload, and assert tag target = `GITHUB_SHA` = checked-out HEAD = build metadata revision. Add fork/PR denial, missing-secret failure, wrong-signer, unsigned artifact, and revision-mismatch workflow tests.

### Low — L1: provider vault input lacks a defensive size/control-character bound

Provider references are reconstructed inside Rust, scoped to provider and platform, and cross-provider reads are rejected (`src-tauri/src/credentials.rs:114-134`, `225-308`; tests at `420-444`). The secret itself is read from the OS vault only for the request and is sent only in the configured authorization header (`src-tauri/src/ai.rs:219-343`). However, validation rejects only empty/placeholder values and does not impose a reasonable maximum or reject NUL/control characters (`src-tauri/src/credentials.rs:280-290`).

Priority fix: impose a conservative byte limit and reject control/NUL input before vault persistence and header construction; test boundary sizes and malformed values.

## Credential leakage and Codex App Server checks

- `python scripts/check_committed_secrets.py` completed with `No committed secret patterns found.` A separate read-only scan across all nine reachable Git revisions found zero high-signal private-key, GitHub-token, AWS-key, Meshy-key, or hosted-provider-key matches. The repository scanner includes hosted-provider patterns (`scripts/check_committed_secrets.py:45`) but excludes `ui-references` (`18`), so this is pattern evidence rather than proof for screenshots or arbitrary secrets.
- Provider API keys remain in Windows Credential Manager/macOS Keychain; plans, locks, project state, and migrations do not accept provider-key references. Only the optional 3D workflow may carry its exact non-secret Meshy credential reference (`src-tauri/src/commands.rs:2634-2650`; `src-tauri/src/transaction.rs:2239-2269`; relevant JSON schemas).
- Provider HTTP uses HTTPS for hosted endpoints, loopback HTTP only for local providers, rejects URL credentials/query/fragment, disables redirects, bounds timeout/body size, and does not expose response bodies in status errors (`src-tauri/src/ai.rs:108-175`, `290-378`).
- The Codex boundary has no OpenAI API-key fallback, token-file read, or external-token mode. It performs initialize before account/thread operations, accepts `chatgpt`, supports browser/device-code login and logout, validates approved analysis inputs/schema output, discards stderr, bounds JSONL, and does not serialize account metadata or tokens (`src-tauri/src/codex.rs:227-417`, `573-771`, `1029-1073`, `1254-1489`, `1514-1643`). Source tests cover account state, login modes, cancellation, logout, limits/interruption, schema rejection, redaction, and no token/account serialization (`1704-2139`).
- Signed-out recovery, rollback, backup inspection, and managed removal remain local command paths; authentication blocks new semantic planning rather than recovery.
- No support-bundle producer was found in the named implementation. Therefore no claim of support-bundle redaction is made; any future bundle must default-deny secrets/account metadata and have adversarial redaction tests before release.

## Residual risk and release decision

Do not add identity evidence to the current MCP manifest merely to make readiness green. Preserve `planned_unavailable` until H1 is fixed and independently tested. Do not enable flatten sharing/release while H2 remains. Provider-vault behavior is acceptable subject to L1 and native Windows/macOS vault integration tests. Release remains safely fail-closed, not release-ready, until M2 is implemented and demonstrated.

No source files, workflows, manifests, credentials, or external systems were changed by this audit.
