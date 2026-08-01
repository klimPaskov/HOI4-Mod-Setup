---
name: hoi4-mod-setup-open-source-release
description: Use for Git workflow, GitHub community files, CI, dependency updates, versioning, packaging, signing, notarization, release artifacts, updater metadata, or public release maintenance.
---

# Open-source repository and release workflow

## Required sources

Read:

- `AGENTS.md`
- `CONTRIBUTING.md`
- `DEVELOPMENT.md`
- `RELEASING.md`
- `SECURITY.md`
- `docs/26_open_source_github_workflow.md`
- `docs/29_repository_inventory.md`
- `.github/`

## Repository rules

- Keep `README.md` user-facing.
- Document the provider-neutral setup in README, including Codex default, explicit model/provider selection, honest local/hosted routes, and the Codex-only ChatGPT project-source flatten option.
- Use pull requests for `main`.
- Require stable status checks through a ruleset.
- Keep CODEOWNERS current for sensitive paths.
- Use issue forms for actionable reports.
- Route vulnerabilities through private reporting.
- Use weekly Dependabot version updates for npm, Cargo, and Actions.
- Review sensitive dependency changes manually.

## Git workflow

Use focused branches and Conventional Commit messages. Rebase personal branches on current `main` before review. Use `--force-with-lease` only on a personal review branch. Never force-push or delete protected release history.

## CI rules

- Default workflow permissions are read-only.
- Source builds use repository-owned scripts.
- Tagged builds derive and validate one semantic version across the package manifest, Tauri configuration, metadata, and exact tag.
- Pull requests from forks do not receive release secrets.
- Check names remain stable once rulesets depend on them.
- Platform build jobs run on real Windows and macOS runners.
- Current GitHub-hosted macOS packaging labels are `macos-15` for arm64 and `macos-15-intel` for x64; do not restore retired `macos-13` or `macos-14` labels.
- Native desktop jobs run the repository-owned UI gates, Tauri package verification, and bounded launch smoke after the package is built.
- Windows architecture verification requires the native Tauri executable to match the target machine type; the packaged NSIS bootstrap may be 32-bit, but it must still be a valid PE image.
- On macOS, Tauri's DMG bundling removes the temporary `.app` bundle after packaging; `scripts/run_desktop_e2e.mjs` therefore accepts the still-present `target/release/hoi4-mod-setup` binary as a packaging smoke fallback.
- Pull-request package verification accepts GitHub merge refs; semantic tag and tag-target checks are enforced only when the release workflow enables `HOI4_MOD_SETUP_REQUIRE_RELEASE_IDENTITY=1`.
- Fuzz targets compile in CI from the pinned Rust toolchain.
- Generated artifacts are uploaded only after tests pass.
- Linux CI runners install the Tauri/WebKit/GTK build dependencies before all-feature Rust checks; native Windows and macOS release jobs remain the packaging evidence path.
- Release runners install the pinned Python validation dependencies before invoking repository-integrity checks, and publication asset arguments are built from an explicit file list rather than an unexpanded recursive glob.
- Tauri native bundles are copied from the workspace-root `target/release/bundle` or the alternate `src-tauri/target/release/bundle` layout; when `HOI4_MOD_SETUP_BUNDLE` is set, only that requested `nsis` or `dmg` subdirectory is copied so stale cross-target packages cannot enter a release. `src-tauri/icons/app-icon-source.png` is the reviewed source for generated Windows and macOS icon assets.
- Keep only Windows and macOS icon outputs in source control. Android/iOS icon
  trees and the unreferenced duplicate Tauri `windows-schema.json` are ignored
  because mobile platforms are not supported release targets.
- Release build jobs are tag-only and attach the protected `release` environment to the signing and publication jobs. Windows and macOS signing inputs are kept in separate platform steps; Apple secrets are injected only into the official-signing step, while the community path receives no Apple credentials. Cleanup removes temporary private material and disposable macOS keychains.
- The publish job consumes only the curated artifact from `scripts/prepare_release_assets.mjs`, which rechecks every downloaded platform manifest, source/tag/architecture binding, package hash, signature evidence, and exact platform set. It refuses to reuse an existing tag release and creates a normal release only after every gate passes.
- The manually dispatched `development-preview.yml` uses `scripts/prepare_preview_assets.mjs` to recheck exact source, platform, architecture, package hashes, and notices before publishing one user-facing semantic-version prerelease; the workflow refuses to reuse a published tag, receives no stable signing credentials, and keeps the GitHub prerelease label.
- Preview and stable publication scripts use deterministic installer names (`HOI4-Mod-Setup-windows-x64-setup.exe`, `HOI4-Mod-Setup-macos-arm64.dmg`, and `HOI4-Mod-Setup-macos-x64.dmg`). Preview release notes lead with three direct installer bullets. Hashes, metadata, provenance, notices, and SBOM remain in CI evidence and are verified before publication; the user-facing GitHub Release uploads only the three installers.
- Platform-native lockfiles can contain legitimate optional packages for only one runner (for example, a Windows or macOS bundler). Preview and stable curation therefore require identical source, package, and platform evidence but merge the verified platform SBOM and third-party notice inventories into one deterministic union; they must not reject a release merely because those inventories differ by platform.
- `scripts/generate_sbom.mjs` derives a CycloneDX dependency inventory from the locked pnpm and Cargo metadata and places `SBOM.cdx.json` in native release output. `scripts/generate_third_party_notices.mjs` separately derives the human-readable license inventory. Neither is a substitute for reviewing bundled assets or complete dependency license text.
- Every signing route changes package bytes after the unsigned build manifest. The platform signing job verifies the resulting signature, records package-bound evidence, and regenerates the complete internal artifact manifest before curation.
- Ephemeral Windows community certificates, PFX/CER files, and imported trust
  entries are removed by an `always()` cleanup step even when signing or
  verification fails.
- Stable publication first creates a draft from the exact curated three-file
  installer set, verifies the draft state and assets, and only then changes it
  to a normal public release.

Use the package scripts as the stable local release surface: run
`pnpm release:build`, `pnpm release:verify`, `pnpm desktop:e2e`, and
`pnpm installer:e2e` for a native package; run `pnpm release:prepare` only when curating final release
assets. Read the configured version from `package.json`, Tauri
configuration, Cargo metadata/lock, and the tag instead of copying a temporary
version into a skill.

## Release rules

Use semantic version tags. Prepare each release through a pull request. Build
from the exact annotated tag commit. Publish only through the gated tag
workflow.

Stable release evidence includes:

- user-facing Windows `.exe` installer and macOS `.dmg` installer for each
  supported architecture, with extension and platform checks
- a ChaosX Authenticode signature on community Windows builds and an ad-hoc
  codesign signature on community macOS builds; official credentials upgrade
  these to Azure Artifact Signing and Apple Developer ID/notarization
- release notes
- migrations and compatibility notes
- internal SHA-256, third-party-notice, CycloneDX SBOM, source-binding, and
  signing evidence that is not uploaded as public release clutter
- clean-machine install, launch, uninstall or app-removal results

Never move a published tag. Withdraw a bad release and publish a new version.

The current release workflow builds Tauri packages on explicit Windows x64,
macOS arm64, and macOS x64 runners, verifies exact PE or Mach-O architecture
and package extension, then signs and rehashes each package before curation.
When `HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED=false`, Windows generates an
ephemeral `ChaosX` code-signing certificate, applies an RFC 3161 timestamp, and
deletes the private material; macOS ad-hoc signs the app before rebuilding each
DMG. When the variable is `true`, Windows uses Azure Artifact Signing through
OIDC and macOS uses the protected Developer ID P12 plus notarization. Signature
verification is performed by the platform job before publication; metadata
alone cannot pass. Active main and `v*` tag rulesets protect reviewed
publication. Third-party Actions are pinned to reviewed full commit SHAs.

The Azure route requires the OIDC secrets `AZURE_CLIENT_ID`,
`AZURE_TENANT_ID`, and `AZURE_SUBSCRIPTION_ID`, plus the protected endpoint,
account, profile, signer, and RFC 3161 timestamp variables documented in
`RELEASING.md`; it signs only the generated Windows package and never stores a
PFX or private key in the repository.

The required platform-verification mode rejects local uncommitted builds whose
source revision is `unresolved-local`; release builds bind metadata to the
checked-out `HEAD`, current protected `main`, and the annotated tag target before
a package is treated as release evidence. A local build may still produce an unsigned
inspection artifact, but it is not release evidence.

`actions/checkout` may materialize a pushed annotated tag as a lightweight local
ref. The release source gate fetches that one exact remote tag ref before checking
its object type and peeling its commit; do not weaken this to trust the checkout ref.

The Ubuntu release-validation job installs the same Tauri system libraries as the
Linux CI compile job before running all-feature Clippy and tests. These packages
support validation only and do not declare Linux as a distributed app platform.

Community Windows signing uses the generated ChaosX certificate directly through
`Set-AuthenticodeSignature`; it does not export a PFX, scan the Windows SDK, or wait
on a timestamp service. The certificate is trusted only in current-user stores for
verification, the untimestamped method is recorded in internal signing evidence,
and the certificate is always removed afterward. Official Azure signing retains
the configured RFC 3161 timestamp route. The signing job has a hard twenty-minute
limit.

## License and updater gates

The repository is licensed under Apache 2.0 in `LICENSE`, and the decision is
recorded in `docs/LICENSE_SELECTION.md`. Run `pnpm release:notices`, keep the
license and generated notices in source and binary distributions, and review
third-party notices before publication. Automatic
application updates are explicitly deferred for the 0.1.x releases; do not
claim updater support or publish updater metadata until the update channel,
signature policy, rollback behavior, and clean-machine tests are implemented.

## Skill and docs alignment

Update this skill and release docs when branch policy, workflow commands, check names, packaging, signing, notarization, artifact paths, release environments, dependency automation, or publication steps change.

Planning-only prompts, schemas, examples, diagrams, source audits, and UI
references live under `docs/`; application code and user-facing community
files remain at the repository root. Keep repository validators bound to the
moved paths; release checksums stay inside build evidence rather than a public
root ledger.

The public repository is the only authoritative tree. Do not recreate a
`repository-template/` mirror of community files, skills, workflows, scripts,
or screenshots. Validate the canonical tree through `pnpm validate`
(`scripts/validate_repository.py`) instead.

## Update this skill when

Update this skill when any Git, GitHub, CI, dependency, version, packaging, signing, notarization, updater, or release workflow changes.
