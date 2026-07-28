# Provider release wiring review

Audit date: 2026-07-28
Repository: `C:\Users\klimp\Documents\Projects\hoi4-mod-setup`
Scope: the named root/template release workflows, release scripts, release docs, open-source release skill, Tauri/package configuration, and directly referenced release-test scripts.

## Decision

**Blocked for release evidence.** The native routing and signature-verification intent is present, but the protected-secret wiring, draft asset handoff, exact source gate, cleanup, provenance, updater, and clean-machine evidence are not sufficient to claim a signed, notarized, publishable release.

No files other than this report were written. No network publication, live signing, macOS execution, real ChatGPT login, or clean-machine test was performed.

The root and repository-template copies of `release.yml`, `RELEASING.md`, and `docs/26_open_source_github_workflow.md` are byte-identical; workflow findings apply to both copies.

## Findings

| ID | Level | Finding | Evidence | Locally testable? |
|---|---|---|---|---|
| B-01 | Blocker | The protected `release` environment is declared only on `draft-release`, while all signing secrets/variables are consumed by `build`. Environment-scoped values therefore are not available to the build job; moving them to repository scope would contradict the documented protected-environment boundary. | `.github/workflows/release.yml:12-28,52-106,126-138,166-172`; `RELEASING.md:57-70`; `docs/26_open_source_github_workflow.md:252-263` | Yes for YAML/configuration; GitHub environment resolution needs CI. |
| B-02 | Blocker | The draft job passes every file from all three artifact trees directly to `gh release create`. Each build generates repeated basenames such as `ARTIFACTS.sha256`, `BUILD_METADATA.json`, and `frontend/index.html`; there is no curated, uniquely named cross-platform asset set. Draft creation is expected to collide or produce an ambiguous release. | `.github/workflows/release.yml:160-164,174-192`; `scripts/release_build.mjs:123-150` | Yes with a local artifact-list/mock-upload fixture; no publication run. |
| H-01 | High | Secrets are broader than the platform job requires. The generic build step exposes Apple certificate, Apple ID, password, team, and identity variables to the Windows runner. `workflow_dispatch` has no tag-only job guard, and tag validation is conditional; a manually selected branch can reach secret-consuming steps before `release_build` rejects its non-tag ref. | `.github/workflows/release.yml:7,52-111,126-138`; `scripts/release_build.mjs:17-22,72-81` | Yes by static workflow review and a non-tag fixture. |
| H-02 | High | Exact source identity is checked after `pnpm tauri build`, not before it. In addition, `release_build` falls back to local `HEAD`, and `release_verify` checks `GITHUB_SHA` and the tag only when those variables are present; it does not require a tag, clean worktree, and `GITHUB_SHA` for release mode. A dirty/local checkout can therefore be represented as its current `HEAD` rather than being rejected before build execution. | `scripts/release_build.mjs:52-82`; `scripts/release_verify.mjs:45-76`; current checkout `HEAD=ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70`, branch `codex/bootstrap-hoi4-mod-setup`, no tag, dirty worktree | Yes with a release-mode identity fixture; no generated build was run. |
| H-03 | High | Failure cleanup is incomplete. The temporary-root variables are written to `GITHUB_ENV` only after import succeeds, so an import/decode/keychain failure can leave the PFX/P12 and keychain without a cleanup path. On Windows, successful cleanup deletes the temp directory but does not remove the imported private-key certificate from `Cert:\CurrentUser\My`. On macOS, signing passwords are passed as `security` command arguments, and keychain deletion errors are ignored. | `.github/workflows/release.yml:64-77,93-106,148-159` | Partly: static review is local; certificate-store/keychain and process-argument behavior require native runner tests. |
| H-04 | High | The macOS notarization route is only intended wiring, not evidence. The import preflight checks the P12, passwords, keychain password, and signing identity, but not `APPLE_ID`, `APPLE_PASSWORD`, or `APPLE_TEAM_ID` before build. Tauri environment consumption, notarization submission, stapling, and the `base64 --decode` command are not exercised on macOS. | `.github/workflows/release.yml:78-106,126-148`; `scripts/release_verify.mjs:126-154`; `src-tauri/tauri.conf.json:28-39` | No on this Windows host; requires a native macOS runner and protected test credentials. |
| H-05 | High | The workflow verifies each job's local hash manifest and, when enabled, the package signature/notarization postcondition, but the draft job does not reverify downloaded artifacts, source/tag metadata, platform/architecture, signatures, notes, or draft state before the write-capable GitHub operation. There is also no final cross-platform checksum/provenance manifest or SBOM/attestation. | `scripts/release_verify.mjs:29-44,100-156`; `.github/workflows/release.yml:166-192`; `docs/26_open_source_github_workflow.md:245-250,289` | Hash/metadata fixture tests are local; GitHub draft-state and native signature checks are not. |
| H-06 | High | Updater metadata is absent. The Tauri config has no updater configuration, and the build script emits only `ARTIFACTS.sha256` and `BUILD_METADATA.json`. The release contract requires updater metadata or an explicit decision to defer application updates. | `src-tauri/tauri.conf.json:28-39`; `scripts/release_build.mjs:131-150`; `.agents/skills/hoi4-mod-setup-open-source-release/SKILL.md:56-68`; `docs/26_open_source_github_workflow.md:289` | Yes by local configuration/script inspection. |
| H-07 | High | Required clean-machine evidence is not produced by the release workflow. `pnpm desktop:e2e` directly spawns a raw executable, observes it briefly, and terminates it; it does not install NSIS/DMG, launch after installation, update, repair, uninstall, exercise Credential Manager/Keychain, register a HOI4 launcher descriptor, or decode the final thumbnail. The release job also does not exercise compatible `codex app-server` discovery, managed browser/device-code login, or the stdio lifecycle against a real installation. | `scripts/run_desktop_e2e.mjs:27-39,72-115`; `RELEASING.md:80-84`; `docs/16_platform_architecture.md:21-27,66-104,126-132`; `docs/20_testing_strategy.md:3-7,45-63,69-81,156-158`; `docs/30_codex_chatgpt_authentication.md:25-63` | No: Windows/macOS machines, HOI4 launcher, Codex installation, and controlled account/manual authority are required. |
| M-01 | Medium | Current version values agree (`0.1.0` in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and the root package entry in `Cargo.lock`), and package/Tauri/tag checks exist. The release scripts do not validate the Cargo package or lockfile version, so version-source drift is still possible. | `package.json:2-4`; `src-tauri/tauri.conf.json:3-5`; `src-tauri/Cargo.toml:1-4`; `Cargo.lock:1256-1259`; `scripts/release_build.mjs:8-22`; `scripts/release_verify.mjs:8-24` | Yes. |
| M-02 | Medium | Matrix labels are clear (`windows-x64`, `macos-arm64`, `macos-x64`) and bundle selection is correct, but metadata records only `RUNNER_OS`/`RUNNER_ARCH`; verification rejects `unresolved-local` without proving the actual package architecture or exact public filename. | `.github/workflows/release.yml:18-27`; `scripts/release_build.mjs:95-110,132-143`; `scripts/release_verify.mjs:77-99` | Partly: static checks are local; binary architecture needs native artifacts. |
| M-03 | Medium | Withdrawal and no-tag-reuse are documented but not enforced by the workflow. `--verify-tag` only checks that the named tag exists; there is no local release-withdrawal/no-reuse state or post-withdrawal guard. | `RELEASING.md:53-55`; `docs/26_open_source_github_workflow.md:289-291`; `.github/workflows/release.yml:187-192` | No: requires GitHub release/tag administration. |
| L-01 | Low | The verifier recomputes hashes for manifest-listed files but does not assert that the manifest is the complete file set or validate a standalone SHA-256 value format. The generator normally creates the set deterministically, so this is secondary to the draft and provenance blockers. | `scripts/release_build.mjs:123-150`; `scripts/release_verify.mjs:29-44` | Yes with a local fixture. |

## Platform and artifact matrix

| Label | Native runner | Tauri route | Copied package path | Current verification contract |
|---|---|---|---|---|
| Windows x64 | `windows-latest` | NSIS via `--bundles nsis` | `dist/release/packages/nsis` | Requires at least one `.exe`; Authenticode is checked only with `HOI4_MOD_SETUP_REQUIRE_SIGNING=1`. Exact filename and binary architecture are not asserted. |
| macOS arm64 | `macos-14` | DMG via `--bundles dmg` | `dist/release/packages/dmg` | Requires at least one `.dmg`; mounted app is checked with `codesign`, identity text, and `xcrun stapler validate` only with signing required. Exact filename and architecture are not asserted. |
| macOS x64 | `macos-13` | DMG via `--bundles dmg` | `dist/release/packages/dmg` | Same as arm64; the matrix label is not bound to the package's actual architecture. |

`src-tauri/tauri.conf.json:28-39` targets `nsis` and `dmg`, and `release_build.mjs:25-30,95-110` selects only the requested subdirectory. This is internally consistent native routing. No generated `dist/release` output exists in the audited checkout, so these are expected routes, not observed artifacts.

## Source and version identity

The current checkout is not an immutable release source: `HEAD` is `ac6538d7cf8a7c1180f7fb3aaf2c6e9da6926c70`, the branch is `codex/bootstrap-hoi4-mod-setup`, no tag is present, and the worktree is dirty. `release_build.mjs` does bind a GitHub build's metadata to `HEAD`, `GITHUB_SHA`, and `refs/tags/<tag>^{commit}` when those values are present, and `release_verify.mjs` repeats those comparisons under `HOI4_MOD_SETUP_REQUIRE_TAURI=1`; the checks are late and not mandatory for local release mode (H-02).

The current configured versions agree at `0.1.0`, but only package/Tauri are enforced by the release scripts (M-01). No release metadata was generated; `pnpm release:verify` stopped at the missing `dist/release/BUILD_METADATA.json` prerequisite.

## Signing and notarization status

- Windows wiring is present: a base64 PFX is written under `RUNNER_TEMP`, imported into `Cert:\CurrentUser\My`, matched by exact subject/private-key presence, and its thumbprint plus SHA-256/timestamp settings are passed through a temporary Tauri config. Verification invokes `Get-AuthenticodeSignature` and checks a valid status plus signer subject (`.github/workflows/release.yml:52-77`; `scripts/release_verify.mjs:111-124`). This was not live-signed or run on a release runner.
- macOS wiring is present as an intended route: a base64 P12 is imported into a temporary keychain, the keychain is made default/unlocked, Apple signing/notarization variables are passed to the Tauri build, and verification invokes `codesign` and `xcrun stapler validate` (`.github/workflows/release.yml:78-106,126-148`; `scripts/release_verify.mjs:126-154`). This was not executed on macOS, so no notarization or Developer ID claim is made.
- Missing Windows certificate/password/identity/timestamp values fail in the import step. Missing core macOS import values fail there; Apple account/team values are only indirectly required by the subsequent Tauri/notarization path (H-04).

## Platform-neutral core, adapters, and unsupported states

The accepted architecture keeps the Rust core responsible for process, credential, path, descriptor, readiness, and release-relevant validation while Windows/macOS behavior sits behind platform adapters (`docs/16_platform_architecture.md:7-27,66-104`). Windows Credential Manager and `%APPDATA%`/`%LOCALAPPDATA%` paths, and macOS Keychain and `~/Library/...` paths, are documented (`docs/16_platform_architecture.md:108-118`); the release workflow's certificate store/keychain are runner-only signing stores and are not evidence for app-runtime credential behavior.

The documented unsupported states remain honest: Windows-only `hoi4-agent-tools.cmd`/3D `.cmd` routes must remain unsupported on macOS (`docs/16_platform_architecture.md:98-100`); a missing `MESHY_API_KEY` leaves optional 3D incomplete and the LoRA/ComfyUI route is interest-only (`AGENTS.md:251-259`). No release file invents a macOS translation or a provider command. The release workflow simply does not test these states.

The App Server contract is documented as official local `codex app-server` over supervised stdio JSONL, with browser login primary and `chatgptDeviceCode` fallback (`docs/16_platform_architecture.md:126-132`; `docs/30_codex_chatgpt_authentication.md:25-63`). The release workflow's Rust tests and direct launch smoke are not evidence of discovery, initialize ordering, managed login completion, logout, usage-limit handling, interruption/restart, or no-secret persistence on both platforms.

## Exact local checks

Passed:

- `node --check scripts/release_build.mjs`
- `node --check scripts/release_verify.mjs`
- PyYAML parse of both release workflows
- SHA-256 mirror check for both workflows, both release docs, and their template copies
- scoped `git diff --check`
- `pnpm test:a11y`
- `pnpm test:e2e`

Expected and correctly fail-closed:

- `pnpm release:verify` — no generated release output; missing `dist/release/BUILD_METADATA.json`.

Not run because they write output or require unavailable external/native authority: `pnpm release:build`, Cargo/native package builds, live Windows signing, macOS build/signing/notarization, GitHub draft publication, clean-machine install/update/repair/uninstall, real Codex browser/device login, and HOI4 launcher discovery.

## Recommended parent actions

1. Attach a protected environment to the platform build jobs, expose only platform-specific secrets to each job, and make manual dispatch non-secret/non-release or tag-only.
2. Validate tag ref, annotated tag target, `GITHUB_SHA`, `HEAD`, clean worktree, and version identity before invoking Tauri; require them in release-verification mode.
3. Stage a curated, uniquely named asset directory; create one final cross-platform checksum/provenance set; verify downloaded assets, signatures, architecture, metadata, notes, and draft state before any write-capable release call.
4. Make cleanup unconditional from the first temp-path assignment, remove the Windows certificate-store entry, reliably delete/restore macOS keychain state, and avoid passing signing passwords in process arguments.
5. Explicitly gate all Apple notarization inputs, then obtain native macOS evidence for the selected Tauri environment route. Do not document it as working until that run passes.
6. Add the Cargo-version and actual-architecture checks, and either implement signed updater metadata/SBOM/provenance or explicitly defer those release claims.
7. Retain clean-machine evidence for installer lifecycle, Credential Manager/Keychain, Codex discovery/login/stdio, launcher descriptors, external paths, thumbnail decoding, and the documented unsupported external workflows. Keep withdrawal/no-tag-reuse as a maintainer-controlled immutable-tag procedure.
