# Release process

This process covers publishing HOI4 Mod Setup itself. It is separate from the Git features that the app configures inside users' mod projects.

## Versioning

Use semantic versions and tags in the form `vMAJOR.MINOR.PATCH`.

- Major: incompatible project-lock, manifest, transaction, or user-workflow changes
- Minor: backward-compatible features
- Patch: backward-compatible fixes and security hardening

Pre-releases use identifiers such as `v0.3.0-beta.1`.

## Release preparation

1. Confirm all required checks pass on Windows, macOS, and Linux x64.
2. Confirm schema migrations and rollback behavior.
3. Confirm the remote workflow manifest is compatible with the application release.
4. Confirm user-facing README, release notes, support status, and known limitations.
5. Run `pnpm release:notices THIRD_PARTY_NOTICES.md` to refresh the checked-in notices, then `pnpm release:sbom` for `dist/release/SBOM.cdx.json`. Native builds also generate both inventories under `dist/release/`. Review them together with bundled assets and dependency license text.
6. Confirm no credentials or private paths are present in artifacts, logs, source maps, or debug symbols.
7. Update the changelog and version through the repository-owned release script.
8. Update affected repo-local skills when release, signing, packaging, or validation steps changed.

Set the intended version once with `pnpm version:set -- MAJOR.MINOR.PATCH` and
confirm package, Cargo, lock, and Tauri metadata agree. Run the local source
gates before creating the release PR:

```powershell
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm test:manifest
pnpm test:release-assets
pnpm test:workflows
pnpm test:a11y
pnpm test:e2e
pnpm test:scratch-live
pnpm validate
pnpm validate:secrets
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
pnpm test:rust
cargo check --manifest-path fuzz/Cargo.toml --bins
pnpm audit --audit-level high
cargo audit
```

`pnpm test:rust` runs every non-opt-in Rust unit, integration, property, and
fault test with all features and four test threads. Set
`HOI4_TEST_RUST_THREADS` from 1 to 32 only for a dedicated runner. Install
Playwright once with `pnpm test:e2e:install` if needed. Activate MSVC as described
in `DEVELOPMENT.md` when the Windows toolchain requires it.

Set `HOI4_MOD_SETUP_SOURCE_REPOSITORY` to an explicit local package checkout and
run `pnpm test:offline`. This verifies every declared committed blob, exercises
fresh all-component installation/import/repair/removal/rollback and the exact
public 0.3.5 source layout upgrade, then scans a synthetic 50,000-file mod.
The application profiles, projects, and caches are temporary. Provider replies
and external installers are fakes; no real account, paid service, MCP tool call,
or shared desktop is used. `pnpm docs:screenshots` refreshes the 17 public
captures through the same headless browser gate. Review the captures before
commit; the Pages workflow consumes them directly.

These checks do not substitute for signed native-package tests on Windows x64,
macOS arm64, macOS x64, and Linux x64, real launcher/game loading, completed browser or
device-code sign-in, OS-vault round trips, or updater replacement/restart/retry.
Record those results on the exact release candidate in an available dedicated
desktop or clean CI runner. Do not run focus-changing native launch/installer
smokes on a shared desktop.

Record counts, exact source revisions, workflow coverage, and remaining
native/publication gates in a versioned document under `docs/development/`.

## Tag and build

Create the release through a reviewed release pull request. After merge, create an annotated tag from the verified `main` commit:

```bash
git switch main
git pull --ff-only
git tag -a vX.Y.Z -m "HOI4 Mod Setup vX.Y.Z"
git push origin vX.Y.Z
```

The tag workflow should build from that exact commit. Release jobs must not modify source and then publish uncommitted output.

For the current prepared candidate, the maintainer's publication sequence is:

1. Push the local review branch and open a PR into `main`; finish source and
   native platform gates and merge through the protected branch.
2. Confirm the live Agentic package publication resolves to the tested exact
   revision. A bundled local candidate or a local MCP tarball is development
   evidence, not proof that users can fetch that package from GitHub/npm.
3. Confirm the protected `release` environment's publication variable,
   platform signing settings, community/official signing credentials, and
   Tauri private updater key match the configuration below.
4. From a clean checkout, run `git switch main`, `git pull --ff-only`,
   `git tag -a vX.Y.Z -m "HOI4 Mod Setup vX.Y.Z"`, and
   `git push origin vX.Y.Z`, using the verified configured version. The workflow requires that tag to target current
   `main` and rejects an already published version.
5. Approve the protected release environment when requested and let the tag
   workflow verify, sign, curate, create its draft, compare all eight uploaded
   public assets, and publish. Do not manually upload or undraft unverified
   assets. Retain the internal signing/provenance/dependency artifacts.
6. Confirm `latest.json` contains Windows/macOS updater targets plus both
   installer-specific Linux routes and the AppImage fallback, and the release
   presents all five installer links. Confirm GitHub Pages uses GitHub Actions;
   merging the page changes into `main` runs `Publish project page`, or dispatch
   that workflow manually if Pages needs its first deployment.
7. Keep the README video placeholder until the editors provide an approved
   video link. Video editing and delivery are separate from the installer
   release. Media stays outside Git and updater metadata.

## Public release assets

- Windows Authenticode-signed installer
- macOS code-signed disk image for each supported architecture
- Linux x64 AppImage and Debian/Ubuntu package, both signed with the Tauri updater key
- release notes with direct platform download links
- signed updater metadata plus a macOS update bundle for each architecture

Checksums, build metadata, signing evidence, the SBOM, and generated
third-party notice inventory remain internal workflow artifacts. The
user-facing GitHub Release keeps the five installers easy to identify and also
publishes `latest.json` plus the two macOS update bundles used by installed
apps. GitHub supplies the tag source archives automatically.

Every stable release signs the final Authenticode-signed Windows installer and
the final code-signed macOS app archives plus both Linux packages with the dedicated Tauri updater key.
The release environment stores `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; only the matching public key is committed.
The app checks the fixed `releases/latest/download/latest.json` endpoint and
requires a valid updater signature before its automatic startup install.
Signature, download, or replacement failure preserves the running version and
leaves a retry path in the title bar.
The curation job verifies each final updater artifact against that committed
public key before it creates `latest.json`; a wrong or rotated private key
therefore blocks publication instead of shipping an unusable update.

## Linux packaging and updates

Linux releases are built on Ubuntu 22.04 x64, the oldest supported build baseline
with WebKitGTK 4.1. Ubuntu 24.04 is also available for native development
verification; a build made there does not prove Ubuntu 22.04 compatibility.
The release includes one `.AppImage` and one `.deb`, with ELF64 architecture,
AppImage type-2 format, extracted application, and Debian product/version/amd64
checks. The Debian package declares `libsecret-tools`; hosted API-provider
credentials require a working desktop Secret Service and `/usr/bin/secret-tool`.

Linux has no invented Authenticode or Apple signing route. The protected
`sign-linux` job receives the Tauri updater key only in its bounded final-package
signing step, checks out no source, and uses no OIDC. It signs both packages,
records `signing=updater_signed` with package-bound evidence, and regenerates
the internal manifest. Curation then cryptographically verifies both signatures
against the committed public key before publishing.

`latest.json` contains `linux-x86_64-appimage` and `linux-x86_64-deb`, plus an
AppImage `linux-x86_64` fallback. The locked updater selects its installer-specific
target first; a Debian installation must receive Debian bytes. Debian updates
use the updater's privileged package-manager route, so denied authorization
must preserve the running installation and expose retry.

CI, preview, and release runners execute native Linux launches under Xvfb and
D-Bus. Their explicit `HOI4_MOD_SETUP_DEDICATED_LINUX=1` flag enables real
`dpkg --install`/`--purge`, with a fail-closed database preflight, refusal of an
existing product registration or binary, bounded process cleanup, and proof
that the test package is absent afterward. Without that flag, local installer
smokes only extract, launch, and remove both packages in a temporary directory.
AppImage extraction avoids requiring a FUSE mount during native smokes.

After the new Linux CI context and every existing required check succeed on
current `main`, review `python scripts/apply_main_ruleset.py --sha <main-commit>`
then apply the same verified policy with `--apply`. The tool binds current main,
requires completed successful GitHub Actions evidence (including
`Desktop E2E (ubuntu-22.04)`), preserves existing protection, and refuses policy
drift. Do not weaken protection to obtain a release.

## Development previews

The manually dispatched `Development preview` workflow builds Windows x64,
macOS arm64, macOS x64, and Linux x64 packages, runs the repository-owned UI and native
smoke gates, verifies each package manifest and architecture, and publishes a
user-facing semantic-version GitHub prerelease such as `0.1.1`. The workflow
refuses to reuse a published version. `scripts/prepare_preview_assets.mjs`
rechecks the exact source commit, package hashes, platform set, and shared
third-party notice inventory before publication. A prerelease is explicitly
not the stable release route. The stable `Release` workflow uses community
signing when protected Windows and Apple credentials are unavailable and
upgrades to official signing/notarization when they are configured. The
published assets use the deterministic names `HOI4-Mod-Setup-windows-x64-setup.exe`,
`HOI4-Mod-Setup-macos-arm64.dmg`, `HOI4-Mod-Setup-macos-x64.dmg`,
`HOI4-Mod-Setup-linux-x64.AppImage`, and `HOI4-Mod-Setup-linux-x64.deb`. Generated
release notes lead with direct installer bullets. Hashes, provenance, the SBOM,
third-party notices, metadata, and per-platform manifests remain in CI evidence
and are verified before publication. Stable releases add the updater metadata
and macOS app archives beside the five clearly named installers.

## Publication gate

The tagged workflow publishes only after the source gate, platform builds,
signature checks, native launch smoke tests, artifact curation, and release
environment gate pass. Never publish directly from an unverified local build.
The stable workflow accepts only unsuffixed `vMAJOR.MINOR.PATCH` tags. Before a
draft is made public, it downloads the exact eight uploaded public assets and
compares every byte to the curated publication bundle; a missing, extra, or
changed remote asset blocks publication.
Each native release runner also exercises the launcher-ready first-install
resume path, user-modification-preserving repair/removal, and apply-to-rollback
hash restoration in fresh temporary project roots before its package is
accepted as release evidence.

## Rollback and withdrawal

Do not move or reuse a published tag. If an artifact is wrong, withdraw the release, publish a clear notice, fix the source, and create a new version. A security withdrawal follows `SECURITY.md` and the private advisory process.

## GitHub release environment

Use protected GitHub environments for stable release credentials. The release workflow must receive only the minimum platform-specific secrets and permissions required for its job. Fork pull requests never receive these secrets.

The repository-owned `pnpm release:build`, `pnpm release:verify`, `pnpm desktop:e2e`, and `pnpm installer:e2e` scripts are the stable native build and test surface. `release:build` clears generated `dist/release` output before copying the current frontend and only the requested native bundle (`nsis`, `dmg`, or the comma-separated `appimage,deb`), and `release:verify` checks the artifact manifest, package extension, architecture, and the Windows native executable's GUI subsystem. The installer test installs, launches, and uninstalls the NSIS package on Windows; on macOS it mounts the DMG, copies and launches the app, then removes it and detaches the image. The current-user Windows route fails closed when its fixed registry keys cannot be inspected, refuses existing HKCU product registrations and matching legacy machine-wide installations, and removes only registry state whose install path exactly matches the current temporary E2E root. The broader legacy machine-wide registry scan has a separate 60-second bound because crowded Windows runners can legitimately need longer than the 10-second bound used for fixed-key operations. The platform signing jobs then verify Authenticode or codesign directly, record the result, and regenerate the internal artifact manifest before curation. Native release metadata must match checked-out `HEAD`, `GITHUB_SHA`, and the tag target; a local build without that exact revision binding is intentionally not release evidence. When this behavior changes, update this file and `hoi4-mod-setup-open-source-release` in the same pull request.

### Signing configuration

The release environment always requires
`HOI4_MOD_SETUP_RELEASE_PUBLISH=true`. With
`HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED=false`, Windows creates an ephemeral
copy of the protected `WINDOWS_COMMUNITY_PFX_BASE64` certificate named `ChaosX`,
uses `WINDOWS_COMMUNITY_PFX_PASSWORD_BASE64` to sign without network access, and
removes the temporary PFX after signing;
macOS applies an ad-hoc code signature to each app before rebuilding its DMG.
Apple certificate and notarization secrets are injected only into the
official-signing step; the community macOS path receives no Apple secrets.
The public release contains no certificate or private key.

The two `WINDOWS_COMMUNITY_*` values are encrypted repository secrets. Rotate
them together; never store the decoded PFX or password in source, artifacts, or
logs. The official Azure route continues to use the reviewed DigiCert RFC 3161
timestamp endpoint.

Set `HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED=true` only after the protected
official credentials below are available:

- Windows OIDC secrets: `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, and
  `AZURE_SUBSCRIPTION_ID`.
- Windows variables: `HOI4_MOD_SETUP_ARTIFACT_SIGNING_ENDPOINT`,
  `HOI4_MOD_SETUP_ARTIFACT_SIGNING_ACCOUNT`,
  `HOI4_MOD_SETUP_ARTIFACT_SIGNING_PROFILE`,
  `HOI4_MOD_SETUP_WINDOWS_SIGNER`, and
  `HOI4_MOD_SETUP_WINDOWS_TIMESTAMP_URL`.
- macOS secrets: `HOI4_MOD_SETUP_APPLE_CERTIFICATE` (base64 `.p12`), `HOI4_MOD_SETUP_APPLE_CERTIFICATE_PASSWORD`, `HOI4_MOD_SETUP_APPLE_ID`, and `HOI4_MOD_SETUP_APPLE_PASSWORD` (an Apple app-specific password).
- macOS variables: `HOI4_MOD_SETUP_MACOS_SIGNING_IDENTITY` and `HOI4_MOD_SETUP_MACOS_TEAM_ID`.

The official Windows route uses the pinned Azure Artifact Signing actions to
sign only the generated Windows package. It never stores a PFX, private key, or
signing secret in the repository.

The workflow never commits these values, copies them into `dist/release`, or
prints them. The official macOS runner uses a random disposable keychain
password; no keychain password secret is required.

Both community and official signing change the package after the unsigned build
manifest is written. Each signing job recalculates the complete internal
artifact manifest only after verifying the signed package.

Stable and preview curation preserve the deterministic union of the four
platform SBOMs and notice inventories under the downloaded `release-artifacts/`
or `preview-artifacts/` root. Each workflow uploads that combined evidence as a
separate internal artifact; it never enters the public installer/update set.

## Public source gate

Do not publish the first public source release until the Apache 2.0 `LICENSE` is included, third-party notices are reviewed, the user-facing README names the license, and the repository security and contribution files are active on the default branch.

## Codex integration release gate

Before a public build, verify compatible App Server startup, browser login, device-code fallback, logout, rate-limit handling, read-only analysis, output-schema rejection, redaction, no account data in project artifacts, and offline recovery. Do not use a real ChatGPT credential in public CI logs or release artifacts.

## Launcher and authentication artifact gate

Before publishing, test clean ChatGPT browser and device-code login, signed-out recovery, usage-limit handling, App Server interruption, output-schema rejection, and account-data redaction. Build fresh launcher-ready mods on Windows, macOS, and Linux and verify both descriptors, the external path, thumbnail decoding, modification preservation, repair, and rollback.
