# Release process

This process covers publishing HOI4 Mod Setup itself. It is separate from the Git features that the app configures inside users' mod projects.

## Versioning

Use semantic versions and tags in the form `vMAJOR.MINOR.PATCH`.

- Major: incompatible project-lock, manifest, transaction, or user-workflow changes
- Minor: backward-compatible features
- Patch: backward-compatible fixes and security hardening

Pre-releases use identifiers such as `v0.3.0-beta.1`.

## Release preparation

1. Confirm all required checks pass on Windows and macOS.
2. Confirm schema migrations and rollback behavior.
3. Confirm the remote workflow manifest is compatible with the application release.
4. Confirm user-facing README, release notes, support status, and known limitations.
5. Run `pnpm release:notices` and `pnpm release:sbom`; review the generated `THIRD_PARTY_NOTICES.md` and `SBOM.cdx.json` together with bundled assets and dependency license text.
6. Confirm no credentials or private paths are present in artifacts, logs, source maps, or debug symbols.
7. Update the changelog and version through the repository-owned release script.
8. Update affected repo-local skills when release, signing, packaging, or validation steps changed.

## Tag and build

Create the release through a reviewed release pull request. After merge, create an annotated tag from the verified `main` commit:

```bash
git switch main
git pull --ff-only
git tag -a vX.Y.Z -m "HOI4 Mod Setup vX.Y.Z"
git push origin vX.Y.Z
```

The tag workflow should build from that exact commit. Release jobs must not modify source and then publish uncommitted output.

## Public release assets

- Windows Authenticode-signed installer
- macOS code-signed disk image for each supported architecture
- release notes with direct platform download links

Checksums, build metadata, signing evidence, the SBOM, and generated
third-party notice inventory remain internal workflow artifacts. The
user-facing GitHub Release uploads only the three installers. GitHub supplies
the tag source archives automatically.

Automatic application updates are explicitly deferred for version 0.1.x. Do
not publish updater metadata or claim update support until the signed update
channel, rollback behavior, and clean-machine tests are implemented.

## Development previews

The manually dispatched `Development preview` workflow builds Windows x64,
macOS arm64, and macOS x64 packages, runs the repository-owned UI and native
smoke gates, verifies each package manifest and architecture, and publishes a
user-facing semantic-version GitHub prerelease such as `0.1.1`. The workflow
refuses to reuse a published version. `scripts/prepare_preview_assets.mjs`
rechecks the exact source commit, package hashes, platform set, and shared
third-party notice inventory before publication. A prerelease is explicitly
not the stable release route. The stable `Release` workflow uses community
signing when protected Windows and Apple credentials are unavailable and
upgrades to official signing/notarization when they are configured. The
published assets use the deterministic names `HOI4-Mod-Setup-windows-x64-setup.exe`,
`HOI4-Mod-Setup-macos-arm64.dmg`, and `HOI4-Mod-Setup-macos-x64.dmg`. Generated
release notes lead with direct installer bullets. Hashes, provenance, the SBOM,
third-party notices, metadata, and per-platform manifests remain in CI evidence
and are verified before publication; the user-facing GitHub Release uploads
only the three installers.

## Publication gate

The tagged workflow publishes only after the source gate, platform builds,
signature checks, native launch smoke tests, artifact curation, and release
environment gate pass. Never publish directly from an unverified local build.

## Rollback and withdrawal

Do not move or reuse a published tag. If an artifact is wrong, withdraw the release, publish a clear notice, fix the source, and create a new version. A security withdrawal follows `SECURITY.md` and the private advisory process.

## GitHub release environment

Use protected GitHub environments for stable release credentials. The release workflow must receive only the minimum platform-specific secrets and permissions required for its job. Fork pull requests never receive these secrets.

The repository-owned `pnpm release:build`, `pnpm release:verify`, `pnpm desktop:e2e`, and `pnpm installer:e2e` scripts are the stable native build and test surface. `release:build` clears generated `dist/release` output before copying the current frontend and only the requested native bundle (`nsis` or `dmg`), and `release:verify` checks the artifact manifest plus the package extension expected for that runner target. The installer test installs, launches, and uninstalls the NSIS package on Windows; on macOS it mounts the DMG, copies and launches the app, then removes it and detaches the image. The platform signing jobs then verify Authenticode or codesign directly, record the result, and regenerate the internal artifact manifest before curation. Native release metadata must match checked-out `HEAD`, `GITHUB_SHA`, and the tag target; a local build without that exact revision binding is intentionally not release evidence. When this behavior changes, update this file and `hoi4-mod-setup-open-source-release` in the same pull request.

### Signing configuration

The release environment always requires
`HOI4_MOD_SETUP_RELEASE_PUBLISH=true`. With
`HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED=false`, Windows creates an ephemeral
code-signing certificate named `ChaosX`, timestamps the installer through
`http://timestamp.digicert.com`, and discards the private key after signing;
macOS applies an ad-hoc code signature to each app before rebuilding its DMG.
Apple certificate and notarization secrets are injected only into the
official-signing step; the community macOS path receives no Apple secrets.
The public release contains no certificate or private key.

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

## Public source gate

Do not publish the first public source release until the Apache 2.0 `LICENSE` is included, third-party notices are reviewed, the user-facing README names the license, and the repository security and contribution files are active on the default branch.

## Codex integration release gate

Before a public build, verify compatible App Server startup, browser login, device-code fallback, logout, rate-limit handling, read-only analysis, output-schema rejection, redaction, no account data in project artifacts, and offline recovery. Do not use a real ChatGPT credential in public CI logs or release artifacts.

## Launcher and authentication artifact gate

Before publishing, test clean ChatGPT browser and device-code login, signed-out recovery, usage-limit handling, App Server interruption, output-schema rejection, and account-data redaction. Build fresh launcher-ready mods on Windows and macOS and verify both descriptors, the external path, thumbnail decoding, modification preservation, repair, and rollback.
