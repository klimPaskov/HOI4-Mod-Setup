# Platform and release audit

Audit date: 2026-07-28
Repository: `C:\Users\klimp\Documents\Projects\hoi4-mod-setup`
Scope: platform, packaging, signing, release, workflow, launcher, and Codex App Server evidence needed for a public GitHub release with Windows and macOS installers.

## Decision

The project is not ready to claim a public stable release or easy signed Windows/macOS installation.

The repository contains a functioning Tauri packaging path and a substantially implemented protocol/launcher contract, but the release identity and distribution gates are incomplete:

- no Git remote, public GitHub repository, `main` branch, or semver tag is configured;
- the worktree is dirty, so the current implementation is not an immutable release source;
- no real `LICENSE` file exists;
- the configured Windows artifact is an MSI, not a Windows `.exe` installer; the locally observed raw `.exe` is an unsigned application binary;
- the locally observed Windows `.exe` and MSI are `NotSigned`;
- no macOS DMG was observed locally, and no macOS signing or notarization route is configured;
- no updater plugin, endpoint, public key, or updater metadata is present;
- the current release verifier checks metadata and hashes but does not cryptographically verify signing, notarization, architecture, or tag-to-commit identity;
- no clean-machine install, launch, update, repair, uninstall, HOI4 launcher discovery, or real managed-login evidence was available.

This report is the only file written by this audit. No source, workflow, packaging, or existing user change was edited, committed, or reverted.

## Repository and release identity

Observed Git state:

| Check | Evidence | Finding |
|---|---|---|
| Branch | `codex/bootstrap-hoi4-mod-setup` | Not a release branch or `main`. |
| HEAD | `ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70` | A candidate source revision, not a release tag. |
| Remotes | `git remote -v` returned none | No GitHub destination is configured. |
| Tags | `git tag --list` returned none | No exact release tag or tag commit exists. |
| Worktree | Multiple modified and untracked files, including core/authentication files | A release cannot be reproduced from the current checkout as an immutable source. |
| License | Only `LICENSE_SELECTION.md` exists | Public-source release is legally blocked until the maintainer selects and adds the actual license and notices. |

The version is currently `0.1.0` in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and the root package entry in `Cargo.lock`. The release scripts validate `package.json` against Tauri configuration (`scripts/release_build.mjs:10-16`, `scripts/release_verify.mjs:6-15`), but do not validate the Cargo package or lockfile version. `CHANGELOG.md` contains only `Unreleased`.

`release_build.mjs:17-21` and `release_verify.mjs:17-21` compare a `v*` tag name with the configured version when the environment variable is present. They do not resolve the tag, verify that it points at the checked-out commit, or require `GITHUB_SHA` to equal the tag target. `BUILD_METADATA.json` records `GITHUB_SHA` when supplied (`scripts/release_build.mjs:79-89`), but the verifier only requires a 40-character hexadecimal value (`scripts/release_verify.mjs:39-46`). Exact tag-commit provenance is therefore unproven.

## Platform and artifact matrix

| Platform / architecture | CI runner and bundle target | Artifact naming currently evidenced | Local evidence | Release status |
|---|---|---|---|---|
| Windows x64 | `windows-latest`, `msi` in `.github/workflows/release.yml:17-27` | `HOI4 Mod Setup_0.1.0_x64_en-US.msi`; raw `hoi4-mod-setup.exe` | Both exist under `target/release`; SHA-256 values are `36558F0DCB65B59C93F438D5A5B2D48F99E6625D701CCD345042C6A77A0E8DF5` for the MSI and `359DAF7DD14E9566FEA50DB6874F4312172F2EC0ADD17FC675C6EECA4F8399B7` for the raw executable. Both were `NotSigned`. | Candidate local build only. No publishable signed installer. No configured `.exe` installer target. |
| macOS arm64 | `macos-14`, `dmg` in `.github/workflows/release.yml:22-24` | No final stable asset name or architecture assertion | No macOS artifact is present on this Windows checkout. | CI intent exists; signing, notarization, package, and clean-machine evidence are absent. |
| macOS x64 | `macos-13`, `dmg` in `.github/workflows/release.yml:25-27` | No final stable asset name or architecture assertion | No macOS artifact is present on this Windows checkout. | CI intent exists; signing, notarization, package, and clean-machine evidence are absent. |

`src-tauri/tauri.conf.json:28-39` enables Tauri bundling with targets `msi` and `dmg`, and includes Windows/macOS icons. It contains no `nsis`/`.exe` installer target, signing identity, certificate configuration, notarization configuration, updater configuration, or architecture-specific artifact contract. The raw executable is not an installer and is not copied as a separately named public asset by `scripts/release_build.mjs:49-57`.

The workflow labels jobs `windows-x64`, `macos-arm64`, and `macos-x64`, but `release_verify.mjs:44-57` only checks that runner platform and architecture are not `unresolved-local`; it does not verify the actual binary architecture or enforce exact artifact names. The release upload currently passes the complete `dist/release/**` tree from every matrix job (`.github/workflows/release.yml:83-87`) rather than a curated, uniquely named cross-platform asset set.

## Platform-neutral core and adapter ownership

The Rust library exposes platform-neutral domain modules in `src-tauri/src/lib.rs`, while platform behavior is selected directly in several modules with `cfg` branches. The useful boundaries are present as traits such as `CredentialStore` and `JsonlTransport`, but there is no single explicit platform-adapter module that owns all Windows/macOS behavior.

Relevant behavior:

- Windows path roots use `%LOCALAPPDATA%\\HOI4 Mod Setup` for application data/cache and `%APPDATA%\\HOI4 Mod Setup` for settings; macOS uses `~/Library/Application Support/HOI4 Mod Setup` and `~/Library/Caches/HOI4 Mod Setup` (`src-tauri/src/paths.rs:13-46`). Project roots are canonicalized and link/junction components are rejected (`src-tauri/src/paths.rs:72-91`; `src-tauri/src/security.rs:488-517`).
- Windows credentials use the `keyring` Windows-native backend; macOS uses the Apple-native backend (`src-tauri/Cargo.toml:44-49`). `OsCredentialStore` stores an opaque reference under service `com.klimpaskov.hoi4-mod-setup` and labels providers `windows_credential_manager` or `macos_keychain` (`src-tauri/src/credentials.rs:9-16`, `61-126`).
- Tests cover the in-memory credential adapter and opaque-reference/redaction behavior, but no Windows Credential Manager or macOS Keychain integration test was run or found (`src-tauri/src/credentials.rs:271-280` and following tests).
- Windows reparse-point handling and path containment are implemented, but long-path, junction, cloud-sync, and real user-profile behavior have not been proven on a clean Windows machine.
- macOS application-data and Keychain code exists, but case sensitivity, symlink behavior, app-bundle execution, quarantine, executable permissions, and real Keychain prompts have not been proven on a clean macOS machine.

The non-Windows branches in path/platform selection should be treated as product support only after explicit Windows and macOS target tests; they are not evidence of a supported Linux product route.

## Codex App Server and managed ChatGPT login

The implementation follows the required product boundary in code:

- `AppServerProtocol::initialize` sends `initialize` and then `initialized` (`src-tauri/src/codex.rs:229-241`).
- Account state is read with `account/read`; ChatGPT-authenticated sessions also read rate limits (`src-tauri/src/codex.rs:244-259`).
- Browser login requests `type: chatgpt` with the hosted success page; device-code login requests `type: chatgptDeviceCode` (`src-tauri/src/codex.rs:262-274`). Login completion waits for both `account/login/completed` and `account/updated` before rereading the account (`src-tauri/src/codex.rs:276-330`).
- The process transport starts the official child as `app-server --stdio`, clears the environment, passes only an allowlisted environment set, bounds JSONL input, supervises liveness, and terminates the child tree on close/drop (`src-tauri/src/codex.rs:1214-1319`, `1383-1427`).
- Schema-constrained semantic output, cancellation, timeout, interruption, redaction, login notifications, and no-account/token persistence have fixture tests (`src-tauri/src/codex.rs:1620-1680`, `1878-1935`, `1965-2040`).

The release risk is discovery and live evidence:

- `find_codex_executable` searches only `PATH`, accepts `codex.exe` on Windows and `codex` on macOS, and rejects links (`src-tauri/src/codex.rs:1430-1450`). It does not use an explicit configured path, does not consider `codex.cmd`, and does not perform a separate version/capability check before starting the server.
- The command layer initializes the discovered process before account requests and replaces dead sessions (`src-tauri/src/commands.rs:286-320`). `open_in_codex` requires an active ChatGPT session and core readiness, then launches with `--cd`; otherwise it returns a manual-folder fallback (`src-tauri/src/commands.rs:3246-3313`).
- The repository's validation report records historical Windows App Server initialize/account evidence, but it is not a fresh release run and does not establish browser login, device-code completion, cancellation, logout, usage-limit behavior, or macOS behavior. No live account was used during this read-only audit.

Before release, the maintainer needs a real-account manual test on each supported OS for browser login, device-code fallback, cancellation, logout, rate-limit blocking, App Server interruption/restart, and no-secret persistence. Mock protocol tests are not a substitute for that gate.

## Launcher descriptor and thumbnail behavior

The deterministic generation path is good enough for unit-level evidence:

- `descriptor.mod` includes `picture="thumbnail.png"` (`src-tauri/src/descriptors.rs:118-157`).
- The launcher descriptor includes the selected project path with backslashes normalized to forward slashes (`src-tauri/src/descriptors.rs:216-232`).
- The external descriptor must be named `<project_id>.mod`, with Windows case-insensitive matching and macOS case-sensitive matching (`src-tauri/src/descriptors.rs:235-257`).
- A deterministic 1x1 RGBA PNG placeholder is generated and decoded/validated; it is hash-tracked as generated content (`src-tauri/src/descriptors.rs:178-213`, `260-291`).
- Readiness verifies descriptor, launcher, thumbnail validity, and lock-file hashes (`src-tauri/src/readiness.rs:867-907`, `1074-1098`). Unit tests cover rendering, filename validation, and PNG decoding (`src-tauri/src/descriptors.rs:340-387`).

There is no evidence that the installed project is discovered by the actual HOI4 launcher on clean Windows and macOS machines. The existing desktop smoke script launches the app binary directly; it does not exercise HOI4 launcher discovery, an external descriptor write, or thumbnail rendering (`scripts/run_desktop_e2e.mjs:27-39`, `72-95`). This must remain an explicit acceptance test rather than a claim inferred from descriptor unit tests.

## Optional and unsupported workflows

The code reports unsupported states instead of inventing platform routes:

- The verified 3D route is Windows-only. `run_3d_health_check` returns `unsupported_platform` when no runnable route exists and requires the locked, hash-tracked Windows Python bootstrap and OS-vault credential (`src-tauri/src/commands.rs:1031-1110`).
- The MCP/agent-tools route likewise reports `unsupported_platform` when the selected component has no verified platform route (`src-tauri/src/commands.rs:1186-1218`). The source resolver test confirms the Windows-only default MCP component is unsupported on macOS without blocking core readiness (`src-tauri/src/source.rs:892-944`, `1233-1255`).
- A missing or invalid `MESHY_API_KEY` leaves the optional 3D workflow incomplete while core setup remains usable. The LoRA/ComfyUI path is interest/planning only and is represented as `planned_unavailable` or `not_selected` (`src-tauri/src/commands.rs:1790-1808`, `src-tauri/src/readiness.rs:452-484`).

The README and release notes must state these platform limitations. They must not imply that macOS has a translated 3D/MCP route or that LoRA/ComfyUI is installed or ready.

## Signing, notarization, updater, checksums, and provenance

### Signing and notarization

No signing route is configured in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `.github/workflows/release.yml`, or a release script. `HOI4_MOD_SETUP_SIGNING_CONFIGURED` only changes a metadata string in `scripts/release_build.mjs:79-89`; it does not sign an artifact. The workflow does not set that variable and does not run `release:verify` with `HOI4_MOD_SETUP_REQUIRE_SIGNING=1` (`.github/workflows/release.yml:68-82`).

The draft job is conditional on the repository variable `HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED == 'true'` (`.github/workflows/release.yml:89-95`), but that variable is not coupled to a certificate, signing command, Apple Developer ID identity, notarization submission, or verification result. Setting the variable could therefore allow the draft path without proving a signed package.

The observed Windows `.exe` and MSI both returned `Status: NotSigned` from `Get-AuthenticodeSignature`. There is no macOS `codesign --verify`, `spctl`, notarization, stapling, or Gatekeeper evidence. Do not publish either platform as signed or trusted until the maintainer chooses and supplies the authorized signing/notarization route and CI verifies the resulting packages.

### Checksums and provenance

The build script produces `dist/release/ARTIFACTS.sha256` and metadata, and the verifier recomputes each listed file hash and rejects unsafe/duplicate paths (`scripts/release_build.mjs:59-96`; `scripts/release_verify.mjs:23-37`). That is useful local integrity evidence, but it is not yet a complete public provenance record:

- the manifest is per CI job, not a curated cross-platform release manifest;
- it includes frontend and metadata files alongside packages;
- the verifier accepts any non-unresolved 40-hex source revision and does not bind it to the tag target;
- no SBOM, build attestation, signed provenance, or final release checksum file is created outside each job's artifact tree;
- `CHECKSUMS.sha256` is a source/repository checksum inventory, not proof of the generated release packages.

### Updater

No `tauri-plugin-updater`, updater public key, endpoint, signed update bundle, or update metadata was found. The app's project update/repair/rollback features are not an application updater. Consequently, update behavior is currently unsupported and cannot be included in clean-machine release claims.

## Workflow permissions and draft-release findings

Positive controls:

- `.github/workflows/ci.yml` and `.github/workflows/release.yml` default to `contents: read`.
- Only `draft-release` requests `contents: write` and it runs in the protected `release` environment (`.github/workflows/release.yml:89-103`).
- The release workflow does not run on pull requests and does not expose release secrets to fork builds.
- Checkout, setup, upload, and download actions are pinned to full commit SHAs in the release workflow (`.github/workflows/release.yml:30-35`, `83`, `97-98`).
- The GitHub ruleset, Dependabot, issue forms, CODEOWNERS, security policy, and contributor files exist. Their activation and repository settings are external GitHub administration, not established by local files.

Blocking or weakening findings:

1. The release build does not run Cargo tests, clippy, fuzz compilation, or the security workflow as a required dependency. It runs frontend gates, template/secret checks, and `cargo fmt --check` (`.github/workflows/release.yml:57-67`).
2. The signing publication gate is declarative but not substantive, as described above. `HOI4_MOD_SETUP_REQUIRE_SIGNING` is never enabled by the release job.
3. `workflow_dispatch` can run the build on a branch because tag validation is conditional on a tag ref (`.github/workflows/release.yml:52-56`). It cannot create the draft without a tag, but the workflow does not make the manual result visibly non-publishable in its build metadata.
4. The draft job downloads every file under all three artifact trees and passes every file to `gh release create` (`.github/workflows/release.yml:97-115`). There is no post-create check for draft state, exact asset count/names, duplicate metadata/hash basenames, asset hashes, release notes, source archive, or platform coverage.
5. There is no automation or checked procedure that withdraws a bad release and prevents tag reuse. `RELEASING.md` and `docs/26_open_source_github_workflow.md` describe the no-tag-reuse policy, but the repository has no remote, tag, or release proving it has been enacted.

## README, license, and public-repository readiness

`README.md` is honest that the application is in development and that public builds will follow release gates. It documents the no-API-key boundary, ChatGPT/App Server prerequisite, optional workflow limitations, security posture, and generated launcher artifacts.

It is not yet a clean public download page:

- it has no Download/Install section with exact Windows/macOS artifact names and architecture selection;
- it has no signature/notarization verification instructions;
- it has no supported-release matrix or known limitations for the Windows-only external workflows;
- it points to a relative issues URL without a configured GitHub remote;
- its license section defers the choice to `LICENSE_SELECTION.md`, and no formal `LICENSE` is present.

The maintainer can improve the README locally once the artifact contract, license, signing status, and GitHub URL are decided. The legal license selection, dependency/asset notices, public repository creation, and GitHub security settings require maintainer authority or external GitHub administration.

## Clean-machine evidence

No clean-machine evidence was found for any complete release path.

| Scenario | Evidence currently available | Missing release evidence |
|---|---|---|
| Windows install | Local MSI exists; raw binary can be launched by the smoke script | Install from the public asset on a fresh Windows x64 machine, elevation/permissions, first run, uninstall cleanup, and signature verification. |
| Windows launch | `scripts/run_desktop_e2e.mjs` directly spawns a native executable and observes it for two seconds (`:72-95`) | Launch after MSI/selected `.exe` installation, first-run state, Credential Manager, long paths, cloud-sync profile, and HOI4 launcher registration. |
| macOS install/launch | CI matrix declares DMG jobs only | Fresh arm64 and x64 installs, quarantine/Gatekeeper, `codesign`, notarization/stapling, app-bundle launch, Keychain access, and launcher registration. |
| Update | No updater implementation/configuration | Signed update package, metadata, rollback/failure behavior, and update from the preceding public version. |
| Repair | Local project repair/rollback code exists | Product repair of an installed desktop app and recovery after interrupted installation on clean systems. |
| Uninstall | No package uninstall evidence | MSI/selected Windows installer removal, macOS app removal, retained/removed user data policy, and credential cleanup policy. |
| Managed login | Mock protocol tests and historical partial Windows validation report | Real browser login, device-code fallback, cancellation, logout, usage limits, App Server restart, and no-secret persistence on both platforms. |

`pnpm test:e2e` is a browser/UI smoke layer, not an installer test. The desktop smoke script does not install MSI/DMG, invoke an updater, repair, uninstall, or interact with the HOI4 launcher.

## Exact audit checks

Read-only checks run during this audit:

- `node --check scripts/release_build.mjs` — passed.
- `node --check scripts/release_verify.mjs` — passed.
- `node --check scripts/run_desktop_e2e.mjs` — passed.
- `python scripts/check_committed_secrets.py` — passed: no committed secret patterns found.
- `python scripts/validate_repository_templates.py` — passed: 12 integrity groups validated.
- `pnpm test:a11y` — passed its accessibility contract check.
- `pnpm test:e2e` — passed its browser smoke check.
- `pnpm release:verify` — failed because `dist/release/BUILD_METADATA.json` is absent. No build was run by this audit because it writes generated output.
- `git diff --check` — passed.
- `Get-AuthenticodeSignature` on the observed Windows raw executable and MSI — both `NotSigned`.
- No macOS package, `dist/release` directory, Git remote, tag, or formal `LICENSE` was observed.

The existing `VALIDATION_REPORT.md` and `docs/development/implementation-status.md` contain useful historical/local claims, including a Windows native build and smoke result, but they are modified worktree documents and were not treated as fresh cross-platform release evidence.

The following were intentionally not claimed or rerun: native build, Cargo test/clippy/fuzz, real browser/device login, macOS build, signing/notarization, installer tests, updater tests, clean-machine tests, and HOI4 launcher tests. Those actions either require another platform/authority or generate state beyond this read-only audit.

## What can be implemented locally now

Subject to maintainer decisions, the repository can locally implement and test:

1. A release-ready README, supported-platform/artifact table, installation instructions, known limitations, release-notes template, and a formal license/notices layout once the license choice is supplied.
2. A precise artifact contract: choose the Windows installer route (`msi` or a maintainer-approved `.exe` installer), stable filenames, architecture assertions, separate macOS arm64/x64 names, and a curated release asset directory.
3. Stronger release verification: validate package/Cargo versions, require a real tag ref, compare the tag target with `GITHUB_SHA` and repository `HEAD`, reject dirty/unresolved provenance, inspect actual package architectures, and produce one final checksum/provenance manifest.
4. Real signing gates: invoke the maintainer-approved Windows signing and macOS Developer ID/notarization commands, require their outputs, and verify Authenticode, `codesign`, Gatekeeper, notarization, and stapling before the draft job can run.
5. A release-specific test dependency on Rust tests/clippy/fuzz/security gates and package-level install/launch tests, plus a post-create draft verification job that checks exact assets, hashes, notes, tag target, and draft state.
6. An explicit update decision: implement the approved Tauri updater route and signed metadata, or document that public releases update manually and omit update claims.
7. OS-targeted integration fixtures for Credential Manager/Keychain, Windows path/reparse/long-path behavior, macOS app-bundle/quarantine behavior, Codex discovery, managed login, and launcher descriptor/thumbnail registration.
8. A withdrawal runbook and no-tag-reuse check that leaves a bad tag permanently withdrawn rather than rebuilt under the same tag.

## What requires maintainer or external authority

- Select the project license, approve third-party notices/assets, and authorize public-source distribution.
- Create/configure the public GitHub repository, remote, default/protected `main`, CODEOWNERS access, private vulnerability reporting, dependency alerts, ruleset, topics, release environment, and repository variables.
- Supply and authorize Windows code-signing material and the Apple Developer ID/notarization route. Secrets must remain in protected CI or the platform credential systems; they must not enter this repository or release metadata.
- Choose whether the Windows download is an MSI or a true `.exe` installer and approve public artifact names.
- Provide macOS arm64/x64 runners or machines and clean Windows/macOS test machines with HOI4 launcher and the supported Codex installation.
- Perform the live ChatGPT-managed browser/device-code login tests with a real account and validate cancellation, logout, usage-limit, and recovery behavior.
- Create the first release tag from a verified protected commit, inspect the draft assets, publish only after all evidence passes, and withdraw without reusing a tag if any gate fails.

## Recommended parent actions

1. Do not publish the current worktree or describe it as signed, notarized, update-capable, or cross-platform verified.
2. Resolve the license and artifact-contract decisions, then make README/community files match the approved public URL and support matrix.
3. Harden the release scripts/workflow around exact tag commit identity, Cargo/package version consistency, architecture, signing/notarization, curated assets, checksums, provenance, and draft verification.
4. Implement or explicitly defer updater support; add the corresponding metadata and clean-machine test gate if implemented.
5. Run the native release matrix from a clean protected commit on Windows x64, macOS arm64, and macOS x64, with real signing/notarization and protected release environment evidence.
6. Execute and retain evidence for install, launch, update/manual-update policy, repair, uninstall, credentials, Codex browser/device login, and HOI4 launcher discovery on every claimed platform.
7. Only then create one immutable semver tag, build from its exact commit, create a draft release, verify the draft assets and notes, and publish. Never move or reuse that tag.

## Parent follow-up after the audit

The parent implementation selected the maintainer-approved Windows NSIS route
so the planned download is a true `.exe` installer rather than the previously
observed MSI. `src-tauri/tauri.conf.json`, both native CI matrices, the release
build script, and contributor instructions now use `nsis`; no new native
package was built in this follow-up, so signing, architecture, clean-machine,
macOS DMG, GitHub publication, and release provenance findings remain open.
