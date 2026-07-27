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
- Native desktop jobs run the repository-owned UI gates, Tauri package verification, and bounded launch smoke after the package is built.
- Fuzz targets compile in CI from the pinned Rust toolchain.
- Generated artifacts are uploaded only after tests pass.
- Linux CI runners install the Tauri/WebKit/GTK build dependencies before all-feature Rust checks; native Windows and macOS release jobs remain the packaging evidence path.
- Release runners install the pinned Python validation dependencies before invoking repository-integrity checks, and draft-release asset arguments are built from an explicit `find` file list rather than an unexpanded recursive glob.
- Tauri native bundles are copied from the workspace-root `target/release/bundle` or the alternate `src-tauri/target/release/bundle` layout; `src-tauri/icons/icon.svg` is the text source for generated Windows and macOS icon assets.

## Release rules

Use semantic version tags. Prepare each release through a pull request. Build from the exact tag commit. Create a draft release first.

Stable release evidence includes:

- Windows artifact
- macOS artifact for each supported architecture
- signatures and notarization where required
- SHA-256 checksums
- source archive
- release notes
- migrations and compatibility notes
- third-party notices
- updater metadata
- clean-machine install and launch results

Never move a published tag. Withdraw a bad release and publish a new version.

The current release workflow builds Tauri packages on explicit Windows x64, macOS arm64, and macOS x64 runners, verifies an `ARTIFACTS.sha256` manifest, and keeps publication behind the `release` environment plus repository variables for signing and publication. CI additionally runs the same package verification and `pnpm desktop:e2e` launch smoke on those runners. `.github/rulesets/main-protected.json` is the declarative protection baseline and still requires administrator activation. Unsigned package builds may be evidence artifacts; they must not be described or published as signed releases. Third-party Actions are pinned to reviewed full commit SHAs.

The required platform-verification mode rejects local uncommitted builds whose
source revision is `unresolved-local`; tagged CI builds must provide the exact
`GITHUB_SHA` before a package is treated as release evidence.

## License gate

A public source release needs a real `LICENSE`. Do not call the repository open source only because it is public. Update README and distributions after the license decision.

## Skill and docs alignment

Update this skill and release docs when branch policy, workflow commands, check names, packaging, signing, notarization, artifact paths, release environments, dependency automation, or publication steps change.

## Update this skill when

Update this skill when any Git, GitHub, CI, dependency, version, packaging, signing, notarization, updater, or release workflow changes.
