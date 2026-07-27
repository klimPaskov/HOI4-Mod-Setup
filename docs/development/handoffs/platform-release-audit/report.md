# Platform and release audit

> Historical snapshot from 2026-07-26. The release scripts and workflow were
> implemented after this audit; its negative findings describe the pre-build
> state and are not current package evidence. See `VALIDATION_REPORT.md` and
> the current release scripts for the latest verified boundary.

Date: 2026-07-26
Release type: public stable, semantic-version tag based
Platforms in scope: Windows x64; macOS Apple Silicon and Intel while supported
Audit scope: `src-tauri/tauri.conf.json`, all `Cargo.toml` files, `scripts/release_build.mjs`, `scripts/release_verify.mjs`, `.github/workflows/`, `package.json`, `pnpm-lock.yaml`, `.gitignore`, and platform/release documentation.

The requested `.codex/agents/hoi4setup_platform_release_auditor.toml` instructions were read. No callable subagent-dispatch tool was available in this session, so no forked subagent handoff is claimed; the parent performed the same read-only scope with the requested `fork_context=false` constraint. Only this report was written.

## Decision

**Release blocked.** The current tag workflow does not build a Tauri application package. It builds and uploads a frontend-only directory, then creates a draft release without signatures, notarization, checksums, updater metadata, architecture identity, or clean-machine evidence.

The README correctly labels the product as in development and calls the packages planned (`README.md:7`, `README.md:13-18`).

## Platform and artifact matrix

| Surface | Current claim or contract | Workflow/config evidence | Observed release status |
| --- | --- | --- | --- |
| Windows x64 | Planned Windows x64 installer (`README.md:13-18`) | Matrix label `windows-x64` on `windows-latest` (`.github/workflows/release.yml:17-23`); Tauri target `msi` (`src-tauri/tauri.conf.json:26-29`) | **Blocked/unverified.** No Tauri build, `.msi`, signing, or install evidence. Uploaded artifact name is only `hoi4-mod-setup-windows-x64`. |
| macOS Apple Silicon | Planned macOS application (`README.md:13-18`) | One generic `macos` label on `macos-latest` (`.github/workflows/release.yml:17-23`); Tauri target `dmg` (`src-tauri/tauri.conf.json:26-29`) | **Blocked/unverified.** No `arm64` identity, package, Developer ID signature, notarization, or clean-machine evidence. |
| macOS Intel | Planned while supported (`README.md:15-18`) | No Intel-specific runner, target, or artifact label | **Not verified.** Do not claim Intel support from the generic macOS job. |
| Linux | Not a product release claim | Ubuntu appears only in the Rust CI matrix (`.github/workflows/ci.yml:66-75`) | **CI-only.** No Linux package or support route is defined. |
| Platform-neutral core and native credentials | Core/adapters are separated by architecture docs; `keyring` is target-gated to `windows-native` and `apple-native` (`src-tauri/Cargo.toml:39-43`) | Windows Credential Manager/DPAPI, macOS Keychain, and platform path rules are documented (`docs/16_platform_architecture.md:43-61`, `docs/16_platform_architecture.md:83-95`) | **Architecture documented; runtime behavior unverified.** The scoped release files contain no platform credential/path E2E evidence. |

The workflow artifact containers are `hoi4-mod-setup-windows-x64` and `hoi4-mod-setup-macos` (`.github/workflows/release.yml:51-55`). They both upload `dist/release/**`, not named installer/application packages.

## Findings

### P0 — Release build and verification are frontend-only

`release_build.mjs` runs only `pnpm build`, copies `dist/index.html` and optional frontend assets into `dist/release/frontend`, and writes `BUILD_METADATA.json` with `frontendOnly: true` (`scripts/release_build.mjs:7-17`). It never invokes the Tauri CLI or a Rust/package build. `release_verify.mjs` checks only `dist/index.html` and that metadata’s product/version (`scripts/release_verify.mjs:5-12`).

The workflow calls this step **“Build signed release artifacts”** and then uploads it (`.github/workflows/release.yml:43-55`). The verifier’s message admits that platform packaging remains a later CI step, but no such step exists before `draft-release` publishes the draft (`scripts/release_verify.mjs:12`, `.github/workflows/release.yml:57-76`). This is the primary honesty failure.

### P0 — Tauri packaging route is not implemented or reproducibly provisioned

`tauri.conf.json` enables bundling and names `msi`/`dmg`, but the release script does not call Tauri (`src-tauri/tauri.conf.json:26-29`). `package.json` contains `@tauri-apps/api` but no `@tauri-apps/cli`; the lockfile importer likewise contains the API only (`package.json:21-35`, `pnpm-lock.yaml:7-47`). A local `node_modules/.bin/tauri`, `src-tauri/build.rs`, `tauri-build` build dependency, platform overlay config, icons, capabilities, and entitlements were not present in the inspected tree. This makes the documented `pnpm tauri dev`/production packaging route unproven and prevents the current release script from producing the configured targets.

### P0 — Signing, notarization, and updater gates are absent

The architecture and release contract require Windows code signing, macOS Developer ID signing/notarization, deterministic metadata, and signed update packages (`docs/16_platform_architecture.md:79-81`, `RELEASING.md:39-47`). The scoped workflow has no signing or notarization commands, certificate/key inputs, protected `environment`, updater configuration, update endpoint/metadata, or signature verification. `HOI4_MOD_SETUP_RELEASE=1` is set but unused (`.github/workflows/release.yml:43-50`, `scripts/release_build.mjs:1-17`).

Current status is **not configured and not executed**, not “signed” or “notarized.” `.gitignore` excludes several common certificate/key extensions (`.gitignore:30-38`), but that is not a signing boundary or evidence that secrets stay out of the runner (`.gitignore:15-22`).

### P0 — Artifact integrity and provenance checks are missing

No scoped script or workflow generates or verifies SHA-256 checksums, SBOMs, signatures, attestations, source-revision metadata, or updater metadata. The release verifier does not inspect artifact type, platform, architecture, hashes, signature state, notarization ticket, or commit identity (`scripts/release_verify.mjs:5-12`). The metadata is hardcoded to product/version and has no source commit or target identity (`scripts/release_build.mjs:17`).

The draft step passes `release-artifacts/**/*` directly to a Bash shell rather than using a verified file manifest (`.github/workflows/release.yml:63-76`). With default Bash settings, `**` is not guaranteed to recurse through the nested `frontend` tree; this can miss files or pass directories. There is no post-download attachment check.

The build script also does not clean an existing `dist/release` tree before copying. A repeated local run can leave stale files that the shallow verifier does not detect (`scripts/release_build.mjs:11-17`).

### P1 — Version strings currently agree but have no source-of-truth enforcement

The application, Tauri config, Rust package, and frontend release metadata all currently say `0.1.0` (`package.json:2-3`, `src-tauri/tauri.conf.json:3-5`, `src-tauri/Cargo.toml:1-8`, `scripts/release_build.mjs:17`). The root package in `Cargo.lock` is also `0.1.0` (`Cargo.lock:1257-1271`). This is a current consistency pass, not a release mechanism.

The release script hardcodes `0.1.0`; it does not derive a version from the tag, update the changelog, or update the other version sources, despite `RELEASING.md` requiring a repository-owned version update (`RELEASING.md:15-24`, `RELEASING.md:57-61`). The workflow accepts any `v*` tag and also allows `workflow_dispatch` without a strict semver/tag/ref assertion (`.github/workflows/release.yml:3-7`).

`pnpm-lock.yaml` has lockfile version `9.0` and integrity-backed resolved packages (`pnpm-lock.yaml:1-47`), and `Cargo.lock` is committed. However, there is no `packageManager` field, `rust-toolchain` file, or equivalent exact toolchain pin. CI and release use Node 22 and moving `stable` Rust (`.github/workflows/ci.yml:42-49`, `.github/workflows/ci.yml:87-93`, `.github/workflows/release.yml:26-34`), so the build identity can drift.

This workspace has no `.git` metadata. Read-only `git status`, `git rev-parse`, `git log`, and `git ls-files` commands therefore failed with “not a git repository.” No local tag/commit or clean-worktree evidence can be established. The tag-triggered checkout is not followed by an explicit tag-object/ref/commit assertion or recorded in build metadata.

### P1 — Release CI does not run release gates

The normal CI workflow has read-only default contents permission and does run frontend lint/typecheck/tests/accessibility plus Rust format/clippy/tests on Ubuntu, Windows, and macOS (`.github/workflows/ci.yml:9-10`, `.github/workflows/ci.yml:29-102`). Those are useful baseline checks, but the release workflow does not depend on them and runs no frontend tests, Rust tests, accessibility checks, E2E tests, or security audits before artifact upload (`.github/workflows/release.yml:35-55`).

No workflow runs `pnpm test:e2e`. The only dedicated frontend test file found is `src/App.test.tsx`, covering initial wizard entry and optional-question/portrait-interest behavior; it does not cover packaging or platform installation. Rust tests are inline in source modules; no `tests/` or `src-tauri/tests/` directory exists. This is not evidence for Windows/macOS install, launch, update, repair, uninstall, credential, or path behavior.

### P1 — macOS architecture and clean-machine support claims are not evidenced

The release matrix has one generic macOS row and one generic `macos-latest` runner. It has no `aarch64`/Apple Silicon or `x86_64`/Intel row, artifact name, build target, or verification assertion (`.github/workflows/release.yml:17-23`, `.github/workflows/release.yml:51-55`). The architecture docs require actual platform evidence and the testing strategy requires both Apple Silicon and Intel while supported (`docs/16_platform_architecture.md:53-61`, `docs/20_testing_strategy.md:90-103`).

The Windows/macOS credential and path contracts are documented, and the Cargo dependency feature split is directionally correct. There is no scoped evidence for Windows Credential Manager, long paths/junctions/reparse points, macOS Keychain, case-sensitive volumes, symlink defense, app-bundle resolution, quarantine, or cloud-synced paths on clean machines.

### P1 — Workflow permissions and action pinning need release hardening

The top-level workflows default to `contents: read`, and only `draft-release` elevates to `contents: write` (`.github/workflows/release.yml:9-10`, `.github/workflows/release.yml:57-61`). No signing secrets are currently exposed to build jobs, and release is not triggered by pull requests. These are positive findings.

The write-enabled job has no protected GitHub `environment`, and production actions use mutable major tags (`actions/checkout@v4`, `setup-node@v4`, `upload-artifact@v4`, `download-artifact@v4`, and similar) rather than reviewed immutable revisions (`.github/workflows/release.yml:25-29`, `.github/workflows/release.yml:51-66`). This contradicts the release/security policy (`docs/26_open_source_github_workflow.md:223-238`). The release job is also not conditioned to tag pushes, so manual dispatch can enter the same publication path without a tag/ref preflight.

### P2 — Dependency update coverage is present but reproducibility/review gates are incomplete

Dependabot is configured weekly for npm, Cargo, and GitHub Actions and groups compatible minor/patch updates (`.github/dependabot.yml:1-38`). The security workflow runs `pnpm audit` and `cargo audit` when lockfiles are present (`.github/workflows/security.yml:25-66`). No auto-merge is configured, which is appropriate for Tauri, credential, filesystem, and signing changes.

The manifests use broad semver ranges, the package manager is not pinned, Rust is moving `stable`, and action refs are mutable. The release workflow does not require a successful dependency/security workflow, so these checks are not publication gates.

### P2 — License and withdrawal policy are documented, not release-enforced

`RELEASING.md` and the release skill correctly require a real license, draft-first verification, and no tag reuse (`RELEASING.md:49-65`; `docs/20_testing_strategy.md:105-119`). No `LICENSE` file exists in the inspected root, and the changelog is still `Unreleased`. The public-source/binary release gate is therefore blocked independently of packaging.

Withdrawal and no-tag-reuse are prose-only. No workflow checks that a published tag is immutable or provides a withdrawal/replacement procedure after draft creation.

## Signing and notarization status

| Gate | Status | Evidence |
| --- | --- | --- |
| Windows Authenticode/Tauri signing | **Missing** | No route, secret, environment, command, or verification in scoped workflow/scripts. |
| macOS Developer ID signing | **Missing** | No route, certificate input, entitlements, or signature verification. |
| macOS notarization/stapling | **Missing** | No Apple credential input, notarization command, ticket/staple check, or architecture-specific job. |
| Tauri updater signing/metadata | **Missing** | No updater plugin/config, endpoint, signed update manifest, or verifier. |
| SHA-256 checksums | **Missing** | No generation or verification step; only the documentation/README requirement exists. |
| Provenance/SBOM | **Missing** | No source commit, attestation, SBOM, or release manifest is emitted. |

Do not add a signing or notarization command by inference. The parent must choose and document the supported route and required protected secrets before enabling release publication.

## Unsupported and blocked routes

- The current `hoi4-agent-tools.cmd` and 3D `.cmd` wrappers are documented as Windows-specific. macOS must show them as unsupported and must not translate or invent a substitute (`docs/16_platform_architecture.md:75-77`). The macOS E2E contract says this remains non-blocking for the platform-neutral core (`docs/20_testing_strategy.md:58-60`). No release evidence verifies that state.
- A missing `MESHY_API_KEY` should leave only 3D incomplete and core usable; the first LoRA/ComfyUI release records interest only and performs no installation or configuration (`README.md:61-71`). These are product-state requirements, not release artifacts, and are not exercised by `release.yml`.
- macOS Intel is a planned conditional package, not a currently verified route. Linux is a CI test runner, not a supported desktop artifact.

## Clean-machine evidence

**Observed:** CI syntax and baseline checks are configured; the local release directories `dist/release`, `artifacts`, and `release-artifacts` do not exist. No platform package was available for inspection.

**Not evidenced:** clean Windows/macOS install, first launch, update, repair, uninstall, signature trust, notarization, updater metadata, Credential Manager/Keychain behavior, path edge cases, rollback after update, or artifact hash verification. `release_verify.mjs` is a local file-presence/metadata check, not clean-machine verification.

## Exact checks performed

- `node --check scripts/release_build.mjs` and `node --check scripts/release_verify.mjs`: passed.
- JSON parsing of `src-tauri/tauri.conf.json` and `package.json`: passed.
- PyYAML parsing of `.github/workflows/ci.yml`, `release.yml`, `security.yml`, and `.github/dependabot.yml`: passed.
- Read-only file and lock inspection: version strings, target-specific keyring features, workflow permissions, matrix labels, package targets, lockfile integrity sections, and release-script outputs were inspected.
- The actual release build was not run because this audit is read-only and the script writes `dist/`; no claim is made that a package build succeeds.
- Git provenance commands were attempted and failed because the supplied workspace is not a Git repository; exact tag commit and clean-worktree status are therefore unavailable here.

## Recommended parent actions

1. Keep public/stable release publication disabled until a real Tauri build produces platform packages and the release verifier checks those packages.
2. Establish one version source and pass an explicit version/tag/commit into build metadata; enforce strict semantic tags and reject non-tag manual publication.
3. Add the approved local Tauri CLI/build prerequisites and implement the actual Windows x64 and macOS architecture-specific packaging jobs. Name artifacts with platform and architecture.
4. Configure protected release environments and minimum platform-specific signing/notarization secrets; pin production actions to reviewed immutable revisions.
5. Add checksum generation/verification, source commit and target provenance, SBOM/attestation policy, updater metadata/signatures, and a deterministic release asset manifest. Replace the unverified recursive shell glob.
6. Make the release job depend on the release-specific tests and security checks. Add clean-machine Windows/macOS install, launch, update, repair, uninstall, credential-store, path, signature, notarization, unsupported-route, and rollback evidence.
7. Add the macOS Apple Silicon and Intel rows only for architectures actually tested; keep Windows-only external workflows explicitly unsupported on macOS and non-blocking.
8. Add the selected `LICENSE`, third-party notices, release notes/compatibility evidence, and a reviewed withdrawal runbook that never reuses a published tag.
