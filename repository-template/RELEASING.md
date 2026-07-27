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
5. Confirm every bundled dependency and notice required by the selected license policy.
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
- software bill of materials when the release pipeline supports it
- release notes
- source archive produced by GitHub from the tag
- signatures or provenance attestations when enabled

## Publication gate

Publish a draft release first. Verify installation, launch, update metadata, credential behavior, and artifact hashes on clean test machines. Promote the draft only after both platform owners approve.

## Rollback and withdrawal

Do not move or reuse a published tag. If an artifact is wrong, withdraw the release, publish a clear notice, fix the source, and create a new version. A security withdrawal follows `SECURITY.md` and the private advisory process.

## GitHub release environment

Use protected GitHub environments for stable release credentials. The release workflow must receive only the minimum platform-specific secrets and permissions required for its job. Fork pull requests never receive these secrets.

The repository-owned `pnpm release:build` and `pnpm release:verify` scripts are the stable automation surface. `release:build` clears generated `dist/release` output before copying the current frontend and native bundle, and `release:verify` checks the artifact manifest plus the package extension expected for the runner platform. Implement and document them before enabling tag publication. When their behavior changes, update this file and `hoi4-mod-setup-open-source-release` in the same pull request.

## Public source gate

Do not publish the first public source release until `LICENSE` exists, third-party notices are reviewed, the user-facing README names the license, and the repository security and contribution files are active on the default branch.

## Codex integration release gate

Before a public build, verify compatible App Server startup, browser login, device-code fallback, logout, rate-limit handling, read-only analysis, output-schema rejection, redaction, no account data in project artifacts, and offline recovery. Do not use a real ChatGPT credential in public CI logs or release artifacts.

## Launcher and authentication artifact gate

Before publishing, test clean ChatGPT browser and device-code login, signed-out recovery, usage-limit handling, App Server interruption, output-schema rejection, and account-data redaction. Build fresh launcher-ready mods on Windows and macOS and verify both descriptors, the external path, thumbnail decoding, modification preservation, repair, and rollback.
