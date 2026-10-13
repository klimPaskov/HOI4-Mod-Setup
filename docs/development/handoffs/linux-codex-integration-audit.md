# Linux Codex integration audit

Date: 2026-10-13  
Branch: `feat/linux-support`  
Scope: read-only audit of `codex.rs`, `claude_code.rs`, `process.rs`, `linux_trust.rs`, `credentials.rs`, the affected command boundary, typed renderer bridge, and relevant fake-transport/unit tests. No real account login, paid model call, or project mutation was performed.

## Result

No unresolved critical or high-severity trust, authentication, analysis-authority, credential-persistence, or account-data leak finding remains in the audited Rust paths after the in-session fixes described below. This is not a release-completion claim: Linux-native execution and Secret Service evidence still have to run on the supported Ubuntu hosts, and one desktop-feature test is currently red.

## Findings

### Medium - desktop error-map regression test fails

`commands::tests::codex_analysis_error_categories_remain_actionable_and_sanitized` fails at `src-tauri/src/commands.rs:13764`. The test uses `include_str!("commands.rs")` and requires an exact indentation string around `with_codex_session(AppServerProtocol::model_list).map_err(provider_analysis_user_error)`. The live code at `ai_model_list` (`commands.rs:1013`) still applies the bounded renderer error map, but formatting changed and the source-text assertion now fails.

Impact: the desktop-feature test suite is not green, so the relevant acceptance evidence cannot be treated as complete. Replace the formatting-sensitive assertion with a behavior/helper test, then rerun the desktop-feature suite.

### Low - Claude Linux publisher documentation is incomplete

`docs/31_ai_provider_profiles_and_chat_sources.md:87` documents Authenticode and macOS Developer ID only. The implementation now accepts Linux Claude Code only after matching exact size and SHA-256 from the official `downloads.claude.ai` version manifest (`linux_trust::verify_account_executable`, `linux_trust.rs:216`). Update the provider document so it does not imply that Linux is unsigned or unsupported. The owning integration skill already includes Linux Secret Service but should also state the Linux publisher-digest route when its implementation-boundary text is next updated.

## Trust and lifecycle observations

- Codex discovery canonicalizes the executable and verifies official published bytes before any `--version` or `app-server` execution. `ProcessJsonlTransport::start`, `codex_executable_version`, and the executable cache now consume the exact digest returned by `verified_executable_sha256` (`codex.rs:2073`, `codex.rs:2355`, `codex.rs:2387`).
- Current standalone, legacy npm, nested `@openai/codex-linux-{x64,arm64}`, and hoisted platform-package paths are considered without executing the JavaScript npm wrapper (`codex_executable_candidates`, `codex.rs:2485`). Version metadata is only a bounded candidate; the downloaded publisher digest remains authoritative (`linux_trust.rs:238`).
- Linux distro tools require canonical `/usr/bin` files, root-owned and non-group/world-writable ancestors, an exact root-owned dpkg receipt entry, and a matching package checksum before a fresh SHA-256 spawn binding (`root_owned_path` and `verify_distro_executable`, `linux_trust.rs:57`, `linux_trust.rs:86`).
- Codex Linux publisher verification fetches the official release `.sigstore`, decodes `rekorBundle.Payload.body`, and compares its declared SHA-256 to the local executable (`linux_trust.rs:308`). This intentionally does not perform complete Sigstore certificate, Rekor signature, or transparency-log verification and must not be described as doing so.
- Claude Linux publisher verification compares both SHA-256 and size against the official version manifest before `--help`, status, login, analysis, logout, or interactive handoff (`linux_trust.rs:273`; `claude_code::find_executable`, `claude_code.rs:240`).
- Linux child environments start from `env_clear`, use `/usr/bin:/bin`, omit credential-shaped ambient variables and loader overrides, and pass only bounded desktop/session variables needed for browser, terminal, and Secret Service integration (`process::add_safe_environment`, `process.rs:655`; `configure_jsonl_platform_environment`, `codex.rs:2209`).
- Linux Codex/Claude project handoff opens a verified distro terminal in the project directory and tells the user which client command to run. It does not automatically execute a mutable user-owned client after its digest check (`open_in_codex` and `launch_in_terminal`, `commands.rs:10896`, `commands.rs:11009`).
- Codex still enforces initialize-before-use; `account/read`; browser `chatgpt` and fallback `chatgptDeviceCode`; per-login cancellation; `account/logout`; usage-limit blocking; supervised restart; read-only/no-network analysis; `outputSchema`; selected `effort`; deterministic proposal validation; and confirmed-record binding. The Linux work did not add an API-key fallback or provider write/approval/readiness authority.
- Account email/plan/usage remain transient UI state. Persisted analysis records retain digests, provenance, confirmed proposal keys, and `account_identity_persisted: false`; the tests continue to reject account- and credential-shaped model output.
- Linux API keys use keyring's synchronous Secret Service backend with no plaintext or mock fallback in the Linux build (`credentials::OsCredentialStore`, `credentials.rs:98`; Linux keyring features in `src-tauri/Cargo.toml`).
- Rollback, journal reads, backup/recovery inspection, and managed local recovery remain independent of account checks. Planning still rechecks the selected provider and Codex still requires `auth_mode = chatgpt`.

## Verification performed

- `cargo test --manifest-path src-tauri/Cargo.toml codex::tests:: --lib`: 56 passed.
- `process::tests::`: 14 passed.
- `claude_code::tests::`: 17 passed, 2 opt-in live tests ignored.
- `credentials::tests::`: 9 passed.
- `linux_trust::tests::` on Windows: 3 platform-neutral parser tests passed; Linux-only tests were not compiled on this host.
- Desktop-feature targeted tests passed for supervised restart/reinitialize, isolated login cancellation, failed-logout state clearing, ready-project clearing, usage-limit blocking, signed-out/wrong-account rejection, and process-detail sanitization. The final error-category test failed as recorded above.
- Read-only publisher metadata probes confirmed the current official Codex `rust-v0.162.1` Linux-musl `.sigstore` uses the expected SHA-256 Rekor payload shape (8,453 bytes), and Claude `2.1.296` exposes 64-character checksums plus sizes for `linux-x64` and `linux-x64-musl`. No client binary was downloaded or executed.

## Missing Linux evidence and limits

- Run the ignored Linux Secret Service round trip in an isolated unlocked desktop session and confirm deletion leaves only the opaque `linux_secret_service` reference.
- Run the ignored Debian/Ubuntu distro-receipt test against the supported `/usr/bin/python3` and `/usr/bin/node` packages, including a negative modified-file fixture if CI can safely provide one.
- Add a Linux-only unit test for Codex candidate enumeration covering standalone, nested npm, and hoisted platform-package layouts; current Windows tests do not exercise those branches.
- Add injectable publisher-fetch tests for redirect-host rejection, oversize bodies, malformed Claude manifests, malformed Sigstore payloads, digest mismatch, and size mismatch. Current parser tests cover version syntax, receipt matching, and digest format only.
- Run the native Ubuntu 22.04/24.04 desktop/package smoke: real official client discovery, App Server initialize/account read without printing identity, browser and device-code start/cancel only, graphical opener environment, terminal handoff, and process-tree interruption.
- Do not treat the metadata probe as full Sigstore verification, a real login test, or proof that every supported desktop/keyring configuration works.

## Recommended fix order

1. Repair the failing desktop source-text assertion and rerun the desktop-feature tests.
2. Update the Claude provider documentation for Linux publisher verification.
3. Run the Linux-only Secret Service, distro trust, client discovery, graphical-session, and package lifecycle gates.
4. Add deterministic fake-fetch and Linux candidate-layout coverage before calling the 0.5.0 integration release-ready.

## Resolution recheck - 2026-10-13

This section supersedes the two open findings and missing-test items above where they overlap. The recheck was read-only and covered the final changed `commands.rs`, `linux_trust.rs`, `codex.rs`, `mcp.rs`, and `docs/31_ai_provider_profiles_and_chat_sources.md`. No Cargo command, real account login, or paid provider call was started during this recheck because the parent was already running the full Windows and WSL suites in separate targets.

### Resolved findings

- **Desktop error-map regression test:** resolved. `commands::tests::codex_analysis_error_categories_remain_actionable_and_sanitized` now isolates the `ai_model_list` handler, removes whitespace, and checks that the Codex branch still maps `with_codex_session(AppServerProtocol::model_list)` through `provider_analysis_user_error` (`commands.rs:13649`, assertion at `commands.rs:13758`). It no longer depends on indentation.
- **Claude Linux publisher documentation:** resolved. `docs/31_ai_provider_profiles_and_chat_sources.md:87-93` now states that Windows uses Anthropic Authenticode, macOS uses the reviewed Developer ID, and Linux requires exact SHA-256 plus size from Anthropic's official version manifest before execution with the digest rebound at spawn.

### Final trust coverage inspected

- `linux_trust::publisher_redirect_allowed` restricts publisher redirects to HTTPS port 443, no userinfo, four hops at most, and the exact reviewed GitHub/Claude hosts (`linux_trust.rs:49`). `read_publisher_body` rejects both declared and streamed oversize bodies (`linux_trust.rs:66`).
- `identity_matches` rejects malformed or unequal SHA-256 values and Claude size mismatches (`linux_trust.rs:37`). `codex_document_digest` accepts only the decoded SHA-256 from the official release document; the implementation still deliberately does not claim full Sigstore verification (`linux_trust.rs:88`).
- Canonical root-owned, non-writable distro paths and exact dpkg receipt checks remain mandatory in `verify_distro_executable` (`linux_trust.rs:149`). Account clients still require publisher HTTPS identity before any version/help/account execution in `verify_account_executable` (`linux_trust.rs:253`).
- `linux_codex_candidates` now has a focused Linux test covering standalone, legacy, nested platform-package, and hoisted npm layouts, and rejects an unknown architecture (`codex.rs:2532`, test at `codex.rs:2889`). App Server and version execution remain bound to the exact digest returned by publisher verification.
- MCP Linux package verification carries a closed symlink map through installed-tree hashing and materializes a private verified runtime copy before the real initialize/tools handshake (`mcp.rs:170`, `mcp.rs:352`, `mcp.rs:798`). The private copy is re-read and rehashed before spawn; undeclared, escaping, or changed links fail closed.
- Native MCP client retargeting continues to preserve unrelated routes and the source-declared idle-exit environment across Codex, Claude Code, Cursor, Qoder, and OpenCode (`commands::retarget_native_mcp_config`, `commands.rs:3817`; test at `commands.rs:13050`).

### Native evidence supplied for this recheck

The security and parent runs report these passing Linux checks:

- publisher safety parsing, redirect, oversize, malformed payload, and digest/size mismatch: 8 tests;
- Codex candidate layouts: 1 test;
- registration parsing and all-five-client retargeting: 1 test each;
- actual protected distro receipt: 1 test;
- actual MCP private-copy handshake with 28 required tools: 1 test;
- isolated Rust-to-Python Secret Service round trip through `scripts/test_linux_credentials.sh`: 1 test;
- native frontend: 157 tests; native browser: 30 tests;
- Windows frontend: 157 tests; Windows browser: 30 tests, with one host socket-buffer retry passing.

This evidence closes the earlier Secret Service, distro receipt, candidate-layout, publisher-parser, and private MCP runtime gaps. The setup browser/device login, cancellation, and logout coverage remains fake-transport only; no real account flow or paid call was performed. Final package build, package smoke, updater, and release validation remain owned by the parent and were still pending at the time of this recheck. This audit therefore does not claim final release completion.
