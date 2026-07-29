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
- `docs/29_repository_template_inventory.md`
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
- Release runners install the pinned Python validation dependencies before invoking repository-integrity checks, and draft-release asset arguments are built from an explicit `find` file list rather than an unexpanded recursive glob.
- Tauri native bundles are copied from the workspace-root `target/release/bundle` or the alternate `src-tauri/target/release/bundle` layout; when `HOI4_MOD_SETUP_BUNDLE` is set, only that requested `nsis` or `dmg` subdirectory is copied so stale cross-target packages cannot enter a release. `src-tauri/icons/icon.svg` is the text source for generated Windows and macOS icon assets.
- Release build jobs are tag-only and attach the protected `release` environment to the platform build job that consumes signing material. Windows and macOS signing inputs are kept in separate platform build steps, and cleanup removes temporary roots plus imported Windows certificates or macOS keychains even after an import/build failure.
- The draft job runs `scripts/prepare_release_assets.mjs`, which rechecks every downloaded platform manifest, source/tag/architecture binding, package hash, and exact platform set before creating uniquely named GitHub assets. It refuses to reuse an existing tag release and verifies that the created release remains a draft.
- The manually dispatched `development-preview.yml` uses `scripts/prepare_preview_assets.mjs` to recheck exact source, platform, architecture, package hashes, and notices before publishing one user-facing semantic-version prerelease (for example `0.1.0`); the workflow refuses to reuse a published tag, receives no stable signing credentials, and keeps the GitHub prerelease label.
- Preview and stable publication scripts use deterministic installer names (`HOI4-Mod-Setup-windows-x64-setup.exe`, `HOI4-Mod-Setup-macos-arm64.dmg`, and `HOI4-Mod-Setup-macos-x64.dmg`) and generate release notes with direct download links, package SHA-256 values, and platform manifests.
- Platform-native lockfiles can contain legitimate optional packages for only one runner (for example, a Windows or macOS bundler). Preview and stable curation therefore require identical source, package, and platform evidence but merge the verified platform SBOM and third-party notice inventories into one deterministic union; they must not reject a release merely because those inventories differ by platform.
- `scripts/generate_sbom.mjs` derives a CycloneDX dependency inventory from the locked pnpm and Cargo metadata and places `SBOM.cdx.json` in native release output. `scripts/generate_third_party_notices.mjs` separately derives the human-readable license inventory. Neither is a substitute for reviewing bundled assets or complete dependency license text.
- Azure Artifact Signing changes Windows package bytes after the initial build manifest. The workflow runs `scripts/refresh_release_manifest.mjs` immediately after signing; that script permits only existing `packages/*.exe` hash changes and refuses file-set or unrelated changes before release verification.

## Release rules

Use semantic version tags. Prepare each release through a pull request. Build from the exact tag commit. Create a draft release first.

Stable release evidence includes:

- Windows artifact
- macOS artifact for each supported architecture
- User-facing Windows `.exe` installer and macOS `.dmg` installer, with extension and platform checks in the release verification script
- signatures and notarization where required
- SHA-256 checksums
- source archive
- release notes
- migrations and compatibility notes
- third-party notices
- CycloneDX SBOM
- updater metadata
- clean-machine install and launch results

Never move a published tag. Withdraw a bad release and publish a new version.

The current release workflow builds Tauri packages on explicit Windows x64, macOS arm64, and macOS x64 runners, verifies a cleanly regenerated `ARTIFACTS.sha256` manifest, exact PE or Mach-O architecture, and platform-appropriate package extension, and keeps publication behind the protected `release` environment plus repository variables for signing and publication. CI additionally runs the same package verification and `pnpm desktop:e2e` launch smoke on those runners. Before a tagged build, the Windows job imports a protected base64 `.pfx` into the runner certificate store and passes a temporary Tauri config with the imported thumbprint, while the macOS jobs unwrap the protected P12 passphrase through an environment-only OpenSSL input, import it into a random disposable keychain, and pass the Apple notarization environment. Temporary signing roots, certificate entries, and keychains are removed after verification. With `HOI4_MOD_SETUP_REQUIRE_SIGNING=1`, the verifier performs Authenticode verification on Windows or codesign plus `xcrun stapler validate` on macOS and requires protected signer identity variables; metadata alone cannot pass the gate. `.github/rulesets/main-protected.json` is the declarative protection baseline and still requires administrator activation. Unsigned package builds may be evidence artifacts; they must not be described or published as signed releases. Third-party Actions are pinned to reviewed full commit SHAs.

Windows can alternatively use the pinned official Azure Artifact Signing action when the protected environment variable `HOI4_MOD_SETUP_WINDOWS_SIGNING_MODE` is `artifact-signing`. That route requires the OIDC secrets `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, and `AZURE_SUBSCRIPTION_ID`, plus the protected endpoint, account, profile, signer, and RFC 3161 timestamp variables documented in `RELEASING.md`; it signs only the generated Windows package and never stores a PFX or private key in the repository.

The required platform-verification mode rejects local uncommitted builds whose
source revision is `unresolved-local`; release builds bind metadata to the
checked-out `HEAD`, `GITHUB_SHA`, and the annotated tag target before a package
is treated as release evidence. A local build may still produce an unsigned
inspection artifact, but it is not release evidence.

## License and updater gates

The repository is licensed under Apache 2.0 in `LICENSE`, and the decision is
recorded in `docs/LICENSE_SELECTION.md`. Run `pnpm release:notices`, keep the
license and generated notices in source and binary distributions, and review
third-party notices before publication. Automatic
application updates are explicitly deferred for the 0.1.0 release; do not
claim updater support or publish updater metadata until the update channel,
signature policy, rollback behavior, and clean-machine tests are implemented.

## Skill and docs alignment

Update this skill and release docs when branch policy, workflow commands, check names, packaging, signing, notarization, artifact paths, release environments, dependency automation, or publication steps change.

Planning-only prompts, schemas, examples, diagrams, source audits, and UI
references live under `docs/`; application code and user-facing community
files remain at the repository root. Keep checksums and repository validators
bound to the moved paths.

## Update this skill when

Update this skill when any Git, GitHub, CI, dependency, version, packaging, signing, notarization, updater, or release workflow changes.
