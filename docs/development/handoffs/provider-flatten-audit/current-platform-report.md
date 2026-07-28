# Current platform and release audit

Audit date: 2026-07-28
Repository: `C:\Users\klimp\Documents\Projects\hoi4-mod-setup`
Scope: Windows/macOS platform behavior, native packaging, signing, release workflows, launcher artifacts, Codex App Server discovery/authentication, and implementation-status/validation evidence.

This is a read-only current-worktree snapshot. Only this report was written. The existing worktree changes were preserved.

## Decision

**Public release decision: FAIL / BLOCKED.**

The worktree has code-level and fixture evidence for a Windows/macOS-shaped Tauri application, a local unsigned Windows package, and protocol/descriptor behavior. It does not have a publishable release identity, cryptographic signing/notarization, exact tag-commit enforcement, curated cross-platform artifacts, updater metadata, or clean-machine evidence. The local Windows package must not be described as signed, notarized, or as a stable release. No macOS evidence is claimed from this Windows host.

Status terms below mean:

- **PASS**: directly supported by the inspected file or read-only check.
- **PARTIAL**: implementation or intent exists, but a required gate is missing.
- **FAIL**: the current route contradicts a release requirement or is unsafe to claim.
- **NOT EVIDENCED**: no acceptable platform or clean-machine evidence was present.

## Snapshot identity and version

| Check | Current evidence | Status |
|---|---|---|
| App source branch | `codex/bootstrap-hoi4-mod-setup` | NOT RELEASE IDENTITY |
| App source HEAD | `ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70` | Candidate local revision only |
| Git remote | None returned by `git remote -v` | BLOCKED |
| Release tags | None returned by `git tag --list` | BLOCKED |
| Worktree | 93 modified/untracked status entries, including workflows, release scripts, Tauri config, Rust auth/platform files, and the requested handoff directory | FAIL for immutable release provenance |
| App version | `0.1.0` in `package.json:2-3`, `src-tauri/tauri.conf.json:3-5`, `src-tauri/Cargo.toml:1-7`, and the `hoi4-mod-setup` package in `Cargo.lock:1257-1258` | PASS for current consistency |
| Script version gate | `release_build.mjs:10-21` and `release_verify.mjs:6-21` compare `package.json` with Tauri config and an optional `v...` ref, but do not validate Cargo or the tag target | PARTIAL |
| Formal license | No `LICENSE` file; `README.md:8-18` and `RELEASING.md:63-65` keep public release gated | BLOCKED |
| Requested validation directory | `implementation-status/validation` is absent. The available evidence is `VALIDATION_REPORT.md` and `docs/development/implementation-status.md` | NOT EVIDENCED as a directory |

`BUILD_METADATA.json` in the current local output records `sourceRevision: unresolved-local`, `platform: win32`, `architecture: unresolved-local`, and `signing: not_configured` (`dist/release/BUILD_METADATA.json:1-10`). This is not an exact tagged source identity.

## Platform and artifact matrix

| Platform / architecture | Declared runner and target | Current artifact evidence | Hash/signature evidence | Release status |
|---|---|---|---|---|
| Windows x64 | `windows-latest`, `bundle: nsis` in `.github/workflows/ci.yml:147-157` and `.github/workflows/release.yml:17-27`; Tauri targets include `nsis` in `src-tauri/tauri.conf.json:28-39` | `dist/release/packages/nsis/HOI4 Mod Setup_0.1.0_x64-setup.exe` exists. A raw `target/release/hoi4-mod-setup.exe` also exists but is not an installer. | NSIS SHA-256 `dbec4b57b42d09a485dfaddcf5a473c36021ebde2ba5241b8620569e24d642d3`; `Get-AuthenticodeSignature`: `NotSigned`. | Local unsigned evidence only; not a release |
| Windows x64, extra local output | Not a current workflow target; `msi` is not in the current Tauri target list or release matrix | `dist/release/packages/msi/HOI4 Mod Setup_0.1.0_x64_en-US.msi` exists alongside NSIS | MSI SHA-256 `36558f0dcb65b59c93f438d5a5b2d48f99e6625d701ccd345042c6a77a0e8df5`; `NotSigned` | Output is not curated to the requested NSIS target |
| macOS arm64 | `macos-14`, `bundle: dmg` in `.github/workflows/ci.yml:152-154` and `.github/workflows/release.yml:22-24` | No macOS `.dmg` or `.app` exists in the current local release output | No `codesign`, notarization, stapling, or Gatekeeper evidence | NOT EVIDENCED; no macOS claim |
| macOS x64 | `macos-13`, `bundle: dmg` in `.github/workflows/ci.yml:155-157` and `.github/workflows/release.yml:25-27` | No macOS `.dmg` or `.app` exists in the current local release output | No signing/notarization evidence | NOT EVIDENCED; no macOS claim |

The local `ARTIFACTS.sha256` contains eight entries, including both the NSIS and MSI packages (`dist/release/ARTIFACTS.sha256:1-33`). A read-only recomputation found `AllHashesMatch=True` for all eight entries. This proves local file integrity against that local manifest only. It does not prove source provenance, signing, architecture, or publication integrity.

The current build script clears `dist/release` but copies the entire first existing native bundle directory (`scripts/release_build.mjs:39-57`). Because the current output contains both `msi` and `nsis`, the script does not establish that every copied package belongs to the requested bundle target or current build. The verifier accepts Windows `.msi` or `.exe` and macOS `.dmg` or `.app` (`scripts/release_verify.mjs:47-57`), which is weaker than the documented user-facing Windows `.exe` and macOS `.dmg` contract.

The workflow labels architectures, but `release_verify.mjs:39-46` only rejects unresolved metadata; it does not inspect the package's actual architecture or bind the file name to the matrix architecture. `tauri.conf.json` declares bundle targets and icons, but no architecture-specific artifact contract, signing identity, notarization configuration, updater configuration, or public release naming policy.

## Exact source revision and build identity

The normal tagged workflow checks out the tag event and validates only tag syntax (`.github/workflows/release.yml:30-56`). `release_build.mjs:17-21` compares an optional `GITHUB_REF_NAME` version with `package.json`; `BUILD_METADATA.json` copies `GITHUB_SHA` when present (`scripts/release_build.mjs:79-89`). However, `release_verify.mjs:39-46` accepts any 40-hex `sourceRevision` and never compares it with the tag object, `GITHUB_SHA`, or `git rev-parse HEAD`.

Therefore:

- the current local package fails the release verification gate: `HOI4_MOD_SETUP_REQUIRE_TAURI=1 node scripts/release_verify.mjs` failed with `platform release is missing the exact source revision`;
- the current worktree has no tag to verify and its metadata is `unresolved-local`;
- the release workflow has intent to build a tag checkout, but exact tag-commit identity is not an enforced verifier invariant;
- the release workflow can be manually dispatched on a non-tag ref. Its draft job is tag-gated, but the non-release output is not explicitly marked as non-publishable.

This is a release-provenance blocker, not evidence that the normal GitHub checkout is necessarily wrong.

## Signing, notarization, checksums, provenance, and updater

| Gate | Finding | Status |
|---|---|---|
| Windows code signing | No signing command or certificate route is present. `HOI4_MOD_SETUP_SIGNING_CONFIGURED` only changes the metadata string in `release_build.mjs:79-89`. Current raw EXE, MSI, and NSIS package all report `NotSigned`. | FAIL |
| macOS Developer ID signing | No route or verification command is present; no macOS artifact exists locally. | NOT EVIDENCED / BLOCKED |
| macOS notarization and stapling | No notarization submission, ticket, stapling, `codesign`, `spctl`, or Gatekeeper evidence is present. | BLOCKED |
| Signing release gate | The release job does not set `HOI4_MOD_SETUP_SIGNING_CONFIGURED=1` and does not invoke verification with `HOI4_MOD_SETUP_REQUIRE_SIGNING=1` (`.github/workflows/release.yml:68-82`). | FAIL |
| Publication boolean | The draft job requires repository variables named `HOI4_MOD_SETUP_RELEASE_PUBLISH` and `HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED` (`.github/workflows/release.yml:89-95`), but the latter is not coupled to a cryptographic result. | FAIL as substantive proof |
| SHA-256 | Local artifact manifest generation and hash recomputation are implemented (`release_build.mjs:59-96`, `release_verify.mjs:23-37`) and passed for the current local output. | PASS locally / PARTIAL for release |
| Source/tag provenance | Local metadata is unresolved; no exact tag-to-commit assertion, SBOM, attestation, or final cross-platform manifest exists. | FAIL |
| Updater metadata | No updater plugin, public key, endpoint, signed update bundle, or updater metadata was found in the scoped package/config/workflow files. Project update/repair commands are not an application updater. | BLOCKED / unsupported |
| Draft verification | The workflow creates a draft with `--draft --verify-tag --generate-notes` (`.github/workflows/release.yml:101-115`), but there is no post-create check for draft state, exact asset set, hashes, notes, source archive, architecture coverage, signing, or notarization. No draft was observed. | NOT EVIDENCED |
| Withdrawal/no tag reuse | `RELEASING.md:53-55` and `docs/26_open_source_github_workflow.md:276-278` state the correct policy, but there is no remote, tag, release, or automation proving enforcement. | POLICY PASS / OPERATION NOT EVIDENCED |

Do not call the local NSIS/MSI or raw Windows executable a signed release. The repository's own release skill also states that unsigned builds may only be evidence artifacts (`.agents/skills/hoi4-mod-setup-open-source-release/SKILL.md:70-76`).

## Platform-neutral core and adapter ownership

The intended boundary is documented as a Rust-owned core with explicit Windows and macOS adapters (`docs/16_platform_architecture.md:7-19`, `:66-84`, `:94-104`). The implementation has useful interfaces: `CredentialStore` and `JsonlTransport`, while core modules are exposed from `src-tauri/src/lib.rs:1-22`. Descriptor rendering, hashing, readiness, plans, and transactions are platform-neutral in intent.

The boundary is **PARTIAL**, not fully proven as an adapter architecture:

- OS paths, system-browser resolution, process termination, and credential backends use direct `cfg` branches in shared modules (`src-tauri/src/paths.rs:13-46`, `src-tauri/src/process.rs:203-333`, `src-tauri/src/credentials.rs:91-184`) rather than one explicit platform-adapter module;
- `Platform::current()` treats every non-Windows target as `Macos` (`src-tauri/src/models.rs:31-38`). This does not establish Linux support, but Linux Rust CI can exercise macOS behavior unless target-specific tests prevent that false equivalence;
- no native Windows/macOS integration test evidence was found for the adapter boundary.

### Windows paths and Credential Manager

The implementation maps application data/cache to `%LOCALAPPDATA%\HOI4 Mod Setup` and settings to `%APPDATA%\HOI4 Mod Setup`; project roots are canonicalized and link/junction components rejected (`src-tauri/src/paths.rs:13-91`). The Windows dependency selects `keyring` with `windows-native` (`src-tauri/Cargo.toml:44-46`), and the credential adapter uses the service `com.klimpaskov.hoi4-mod-setup` with opaque references (`src-tauri/src/credentials.rs:9-10`, `:91-151`).

This is implementation evidence, not clean-machine proof. The only credential tests use `MemoryCredentialStore` and redaction/reference checks (`src-tauri/src/credentials.rs:376-444`). No Windows Credential Manager prompt, persistence-across-restart, profile path, long-path, cloud-sync, or junction integration result is present.

### macOS paths and Keychain

The implementation maps application data to `~/Library/Application Support/HOI4 Mod Setup` and cache to `~/Library/Caches/HOI4 Mod Setup` (`src-tauri/src/paths.rs:19-45`). The macOS dependency selects `keyring` with `apple-native` (`src-tauri/Cargo.toml:48-49`), and the same service/reference contract labels the provider `macos_keychain` (`src-tauri/src/credentials.rs:186-223`).

No macOS build, Keychain prompt/persistence test, case-sensitive-volume test, app-bundle/quarantine test, executable-permission test, or real user-profile path test is present. The fallback to a temporary directory when `HOME`, `APPDATA`, or `LOCALAPPDATA` is absent is also not a supported-profile release result (`src-tauri/src/paths.rs:13-45`).

## Unsupported external workflow states

The current state handling is honest and should remain so:

- the 3D health command returns `unsupported_platform` off Windows (`src-tauri/src/commands.rs:1105-1114`);
- the MCP health command returns `unsupported_platform` off Windows (`src-tauri/src/commands.rs:1269-1279`), and selected unsupported components are kept non-blocking in readiness (`src-tauri/src/readiness.rs:1069-1102`);
- missing Meshy credentials leave the optional workflow incomplete without blocking core setup, and LoRA/ComfyUI is interest-only (`README.md:78-88`, `docs/development/implementation-status.md:29-38`);
- the architecture explicitly says not to invent a macOS translation for Windows-only wrappers (`docs/16_platform_architecture.md:98-100`).

This part is a **PASS for honest unsupported-state behavior**. It is not evidence that the external workflows are installed, reversible, signed, or ready on either platform.

## Codex App Server and managed ChatGPT authentication

### Discovery and stdio lifecycle

The current Windows host has a `codex.exe` on `PATH`; the read-only `codex app-server --help` check accepted `app-server` and documented `--stdio`. This verifies the current host command only; it is not macOS evidence and does not prove the packaged app's PATH.

The code-level route is substantially aligned:

- `find_codex_executable` searches only `PATH`, uses `codex.exe` on Windows and `codex` otherwise, rejects link components, and returns a canonical file (`src-tauri/src/codex.rs:1470-1490`);
- the command layer starts `app-server --stdio`, completes `initialize`/`initialized` before account or thread requests, replaces dead sessions, and clears the failed session (`src-tauri/src/commands.rs:295-329`; `src-tauri/src/codex.rs:227-240`);
- the transport clears the environment, passes only an allowlisted process environment, bounds JSONL line size, uses a reader thread, polls liveness, and terminates/waits for the child on close/drop (`src-tauri/src/codex.rs:1254-1467`);
- protocol fixtures cover initialize ordering, account/rate-limit reads, interruption, bounded JSONL, logout, cancellation, and schema-boundary behavior (`src-tauri/src/codex.rs:1668-1819`, `:1927-2011`).

The discovery route is still **PARTIAL** against the architecture contract: it has no explicit configured executable path and no separate version/capability probe before login. The architecture document calls for an explicit configured path plus narrow PATH lookup and app-server support verification (`docs/16_platform_architecture.md:126-132`).

### Browser and device-code login

The request/response contract is present:

- browser login sends `type: chatgpt`, hosted success-page options, and `appBrand: chatgpt`;
- device login sends `type: chatgptDeviceCode`;
- the bridge waits for matching `account/login/completed` and `account/updated` notifications before rereading account state;
- URLs are restricted to validated HTTPS values and opened through fixed OS-owned browser executables (`src-tauri/src/codex.rs:260-335`, `:1090-1189`; `src-tauri/src/commands.rs:610-626`; `src-tauri/src/process.rs:303-332`).

The corresponding fake-transport tests are present (`src-tauri/src/codex.rs:1731-1785`, `:1927-1996`), and frontend tests cover URL opening, cancellable sign-in, usage-limited state, and logout error presentation (`src/App.test.tsx:157-197`). However, `VALIDATION_REPORT.md:81` records only historical Windows help/initialize/account-read evidence on 2026-07-26, with an unauthenticated account; browser completion and device-code completion were not exercised. No real managed login, cancellation, logout, usage-limit, or no-secret persistence run on Windows or macOS is available.

## Launcher-ready descriptor and thumbnail behavior

The deterministic unit-level contract is **PASS**:

- `descriptor.mod` renders `picture="thumbnail.png"` (`src-tauri/src/descriptors.rs:118-157`);
- the external launcher descriptor writes the selected project path with slash normalization and validates control characters (`src-tauri/src/descriptors.rs:216-232`);
- the external descriptor must be named `<project_id>.mod`, with Windows case-insensitive and macOS case-sensitive matching (`src-tauri/src/descriptors.rs:235-257`);
- a deterministic 1x1 RGBA PNG is generated, decoded, size-bounded, and hash-tracked (`src-tauri/src/descriptors.rs:178-213`, `:260-291`);
- readiness checks the external descriptor against the project name/path and checks the thumbnail's decoded bytes against its locked hash (`src-tauri/src/readiness.rs:1021-1062`, `:1256-1281`);
- Rust tests cover descriptor round-trip/path rendering, PNG decoding, tags, and launcher filename validation (`src-tauri/src/descriptors.rs:340-387`).

The release/launcher evidence is **NOT EVIDENCED**. `scripts/run_desktop_e2e.mjs:27-39` finds a built executable and `:72-105` directly spawns it, observes it briefly, and terminates it. It does not install an MSI/NSIS package or DMG, write an external launcher descriptor, inspect thumbnail rendering, query the actual HOI4 launcher, preserve a user-modified thumbnail, repair, roll back, or uninstall. `VALIDATION_REPORT.md:69-74` explicitly retains Windows/macOS launcher-path and live HOI4 discovery as manual gates.

## Workflow permissions and publication protection

Positive controls:

- CI and release workflows default to `contents: read` (`.github/workflows/ci.yml:9-10`, `.github/workflows/release.yml:9-10`);
- only `draft-release` requests `contents: write`, and it uses the `release` environment (`.github/workflows/release.yml:89-95`);
- release runs on tags or manual dispatch, not pull requests, and third-party checkout/setup/upload/download actions are pinned to full SHAs (`.github/workflows/release.yml:3-7`, `:30-35`, `:83-98`);
- the repository contains a declarative protected-main ruleset and CODEOWNERS, but local files cannot activate GitHub settings (`.github/rulesets/main-protected.json:1-76`, `docs/26_open_source_github_workflow.md:179`).

Blocking findings:

1. The release job runs frontend gates, repository validation, secret scan, and Rust formatting, but not Cargo tests, clippy, fuzz compilation, or the security workflow (`.github/workflows/release.yml:57-67`). The tag workflow does not depend on the CI/security jobs.
2. No release signing or notarization secret is consumed, and the boolean publication variables are not cryptographic evidence.
3. The draft job downloads every file under every matrix artifact tree and passes the complete file list to `gh release create` (`.github/workflows/release.yml:97-115`). There is no curated package-only asset list, final cross-platform hash manifest, SBOM/provenance upload, or post-create draft verification.
4. No repository remote, tag, release environment activation, variable configuration, or administrator ruleset activation was observable in this worktree. Those are external maintainer facts, not local-file evidence.
5. There is no automated withdrawal/no-reuse check. The documented policy must be followed manually until an operational release runbook and protected tag process exist.

## Clean-machine and E2E evidence

| Scenario | Evidence available now | Required evidence missing |
|---|---|---|
| Windows install | Existing local package files; `VALIDATION_REPORT.md:57-59` claims a local unsigned NSIS build/hash/smoke | Fresh Windows x64 install from the selected public installer, first run, permissions/elevation, signature verification, and uninstall cleanup |
| Windows launch | Direct executable smoke script and historical local claim | Launch after installer installation, profile/path behavior, Credential Manager, long paths, and cloud-sync profile |
| macOS arm64/x64 install and launch | CI matrix intent only | Native DMG build, install, quarantine/Gatekeeper, app-bundle launch, Keychain, and both architectures on clean machines |
| Application update | No updater route or metadata | Signed update package/metadata and update/failure/rollback test, or an explicit manual-update-only product decision |
| Project update/repair | Rust/Tauri maintenance commands exist | Do not conflate project repair with desktop-app repair; no clean-machine package repair evidence |
| Uninstall | No observed installer/uninstall run | Windows installer removal, macOS app removal, retained user-data policy, and credential cleanup policy |
| Launcher discovery | Descriptor/thumbnail unit tests and readiness validation | Actual HOI4 launcher discovery on clean Windows and macOS, external path, thumbnail display, modification preservation, repair, and rollback |
| Managed login | Fake protocol tests and historical unauthenticated Windows account check | Real browser/device-code completion, cancellation, logout, usage limits, interruption/restart, and no-secret persistence on both platforms |

`pnpm test:e2e` is only the structural browser smoke (`scripts/run_e2e_smoke.mjs:1-7`); it is not an installer or native platform test. Existing status documents claim useful local Windows and Rust results (`VALIDATION_REPORT.md:32-59`, `docs/development/implementation-status.md:22-24`), but this audit did not rerun build/test commands because they generate workspace artifacts and this audit is read-only.

## Unsupported or blocked routes

- Stable public Windows release: blocked by unresolved local source identity, unsigned artifacts, missing license, incomplete publication verification, and no clean-machine evidence.
- Stable public macOS arm64/x64 release: blocked by absent local packages, absent signing/notarization evidence, and absent native clean-machine evidence.
- Signed/notarized release: unsupported by the current route; no signing command, certificate identity, notarization route, or cryptographic verifier is present.
- Application updater: unsupported until an updater design and signed metadata route exist; project update/repair is not an app updater.
- Exact tagged build provenance: not enforced; no tag or remote exists in this snapshot.
- HOI4 launcher discovery: not claimed; descriptors and PNGs are only unit/readiness evidence.
- Real managed ChatGPT login: not claimed; protocol fixtures and help-level discovery are not live login evidence.
- macOS translation of Windows-only MCP/3D: correctly unsupported; do not add an invented route.

## Recommended parent actions

1. Keep the current worktree and local packages explicitly labeled development/unsigned; do not publish or describe them as signed releases.
2. Choose and implement the maintainer-approved Windows signing and macOS Developer ID/notarization routes, then make publication require cryptographic verification results rather than boolean variables.
3. Harden release identity: require a tag build, compare tag target, `GITHUB_SHA`, and checked-out `HEAD`, reject `unresolved-local`/dirty provenance, and validate package, Tauri, Cargo, and lock versions together.
4. Curate one package set per runner, remove stale cross-target bundle output, assert actual architecture and required extensions, and produce one final cross-platform checksum plus SBOM/provenance record.
5. Add release-job dependencies for the required Rust/security/fuzz gates and a post-create draft verification that checks tag, assets, hashes, notes, platform coverage, signing, and notarization.
6. Decide whether desktop updates are implemented or explicitly manual-only. Do not claim update behavior without updater metadata and clean-machine evidence.
7. Run native clean-machine matrices on Windows x64, macOS arm64, and macOS x64 for install, launch, path/profile behavior, credential stores, update/manual-update policy, repair, uninstall, and HOI4 launcher discovery.
8. Run the real-account managed ChatGPT browser/device-code test on both supported platforms without persisting credentials; retain only redacted pass/fail evidence.
9. Add native Credential Manager/Keychain and path tests, plus a configured Codex path/capability check that matches the architecture contract. Keep non-Windows targets from being silently treated as macOS in support decisions.
10. Select the formal license and notices, configure the protected repository/release environment, create the first tag only from the verified protected commit, and withdraw any bad release without moving or reusing its tag.

## Exact evidence inspected

Named governance, architecture, release, and packaging files: `AGENTS.md`, `docs/16_platform_architecture.md`, `docs/26_open_source_github_workflow.md`, `RELEASING.md`, `.agents/skills/hoi4-mod-setup-open-source-release/SKILL.md`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `scripts/release_build.mjs`, `scripts/release_verify.mjs`, `src-tauri/tauri.conf.json`, `README.md`.

Relevant test/implementation/evidence files: `package.json`, `Cargo.toml`, `rust-toolchain.toml`, `src-tauri/Cargo.toml`, `scripts/run_desktop_e2e.mjs`, `scripts/run_e2e_smoke.mjs`, `src-tauri/src/codex.rs`, `credentials.rs`, `paths.rs`, `process.rs`, `descriptors.rs`, `commands.rs`, `models.rs`, `readiness.rs`, `src/App.test.tsx`, `src/lib/tauri.test.ts`, `source-audit/openai_codex_app_server.json`, `VALIDATION_REPORT.md`, `docs/development/implementation-status.md`, and the existing platform/provider handoffs.

Read-only checks performed in this snapshot: current Git identity/status inspection; current output/manifest enumeration; SHA-256 recomputation of all eight `ARTIFACTS.sha256` entries; Authenticode status for the raw EXE, MSI, and NSIS package; `HOI4_MOD_SETUP_REQUIRE_TAURI=1 node scripts/release_verify.mjs` (expected failure on `unresolved-local`); and `codex app-server --help` on the current Windows host (accepted `--stdio`). No native build, installer mutation, macOS command, signing/notarization action, real login, or clean-machine test was run.
