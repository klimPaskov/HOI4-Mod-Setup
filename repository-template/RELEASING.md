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

## Required artifacts

- Windows signed installer or signed application package
- macOS signed and notarized application package for each supported architecture
- SHA-256 checksum file
- generated third-party dependency notice inventory
- software bill of materials when the release pipeline supports it
- release notes
- source archive produced by GitHub from the tag
- signatures or provenance attestations when enabled

Automatic application updates are explicitly deferred for version 0.1.0. Do
not publish updater metadata or claim update support until the signed update
channel, rollback behavior, and clean-machine tests are implemented.

## Development previews

The manually dispatched `Development preview` workflow builds Windows x64,
macOS arm64, and macOS x64 packages, runs the repository-owned UI and native
smoke gates, verifies each package manifest and architecture, and publishes a
uniquely tagged GitHub prerelease. `scripts/prepare_preview_assets.mjs`
rechecks the exact source commit, package hashes, platform set, and shared
third-party notice inventory before publication. A preview is explicitly not
a stable signed release; the stable `Release` workflow remains fail-closed
until the protected Windows and Apple signing configuration is available. The
published assets use the deterministic names `HOI4-Mod-Setup-windows-x64-setup.exe`,
`HOI4-Mod-Setup-macos-arm64.dmg`, and `HOI4-Mod-Setup-macos-x64.dmg`; generated
release notes list those installers, their SHA-256 values, and their manifests.

## Publication gate

Publish a draft release first. Verify installation, launch, update metadata, credential behavior, and artifact hashes on clean test machines. Promote the draft only after both platform owners approve.

## Rollback and withdrawal

Do not move or reuse a published tag. If an artifact is wrong, withdraw the release, publish a clear notice, fix the source, and create a new version. A security withdrawal follows `SECURITY.md` and the private advisory process.

## GitHub release environment

Use protected GitHub environments for stable release credentials. The release workflow must receive only the minimum platform-specific secrets and permissions required for its job. Fork pull requests never receive these secrets.

The repository-owned `pnpm release:build` and `pnpm release:verify` scripts are the stable automation surface. `release:build` clears generated `dist/release` output before copying the current frontend and only the requested native bundle (`nsis` or `dmg`), and `release:verify` checks the artifact manifest plus the package extension expected for that runner target. CI sets `HOI4_MOD_SETUP_REQUIRE_TAURI=1` and `HOI4_MOD_SETUP_REQUIRE_SIGNING=1`; the latter performs Authenticode verification on Windows or codesign plus `xcrun stapler validate` on macOS and requires the protected signer identity variables. Native release metadata must match checked-out `HEAD`, `GITHUB_SHA`, and the tag target; a local build without that exact revision binding is intentionally not release evidence. When their behavior changes, update this file and `hoi4-mod-setup-open-source-release` in the same pull request.

### Signing configuration

The release workflow fails closed until its protected environment is configured. It imports runner-only signing material, passes a temporary Tauri configuration for the Windows certificate thumbprint and timestamp URL, and removes the temporary certificate/keychain after the verification steps. Configure these values only as GitHub environment secrets or variables:

- Windows secrets: `HOI4_MOD_SETUP_WINDOWS_CERTIFICATE` (base64 `.pfx`) and `HOI4_MOD_SETUP_WINDOWS_CERTIFICATE_PASSWORD`.
- Windows variables: `HOI4_MOD_SETUP_WINDOWS_SIGNER` (the expected Authenticode subject) and `HOI4_MOD_SETUP_WINDOWS_TIMESTAMP_URL`.
- macOS secrets: `HOI4_MOD_SETUP_APPLE_CERTIFICATE` (base64 `.p12`), `HOI4_MOD_SETUP_APPLE_CERTIFICATE_PASSWORD`, `HOI4_MOD_SETUP_APPLE_ID`, and `HOI4_MOD_SETUP_APPLE_PASSWORD` (an Apple app-specific password).
- macOS variables: `HOI4_MOD_SETUP_MACOS_SIGNING_IDENTITY` and `HOI4_MOD_SETUP_MACOS_TEAM_ID`.

Windows can use Azure Artifact Signing instead of a PFX. Set the protected
environment variable `HOI4_MOD_SETUP_WINDOWS_SIGNING_MODE` to
`artifact-signing`, add the OIDC secrets `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`,
and `AZURE_SUBSCRIPTION_ID`, and add the protected variables
`HOI4_MOD_SETUP_ARTIFACT_SIGNING_ENDPOINT`,
`HOI4_MOD_SETUP_ARTIFACT_SIGNING_ACCOUNT`,
`HOI4_MOD_SETUP_ARTIFACT_SIGNING_PROFILE`,
`HOI4_MOD_SETUP_WINDOWS_SIGNER`, and
`HOI4_MOD_SETUP_WINDOWS_TIMESTAMP_URL`. The workflow then uses the pinned
official Azure actions to sign only the generated Windows package. It never
stores a PFX, private key, or signing secret in the repository.

The workflow never commits these values, copies them into `dist/release`, or prints them. The macOS runner unwraps the certificate passphrase through an environment-only OpenSSL input and uses a random disposable keychain password; no keychain password secret is required. The first public release still requires dependency and third-party notice review, upstream source-manifest publication, clean-machine evidence, and maintainer approval.

When Azure Artifact Signing is enabled, the signer changes the Windows package
after the initial build manifest is written. The workflow runs
`pnpm release:rehash` immediately after signing and before
`pnpm release:verify`. The rehash script refuses added or removed files and
refuses changes outside existing `packages/*.exe` entries.

## Public source gate

Do not publish the first public source release until the Apache 2.0 `LICENSE` is included, third-party notices are reviewed, the user-facing README names the license, and the repository security and contribution files are active on the default branch.

## Codex integration release gate

Before a public build, verify compatible App Server startup, browser login, device-code fallback, logout, rate-limit handling, read-only analysis, output-schema rejection, redaction, no account data in project artifacts, and offline recovery. Do not use a real ChatGPT credential in public CI logs or release artifacts.

## Launcher and authentication artifact gate

Before publishing, test clean ChatGPT browser and device-code login, signed-out recovery, usage-limit handling, App Server interruption, output-schema rejection, and account-data redaction. Build fresh launcher-ready mods on Windows and macOS and verify both descriptors, the external path, thumbnail decoding, modification preservation, repair, and rollback.
