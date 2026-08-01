# Open-source Git and GitHub workflow

This document covers the source repository for HOI4 Mod Setup. It is separate from the Git setup that the application can perform inside a user's mod project.

## Repository identity

Recommended repository name:

```text
HOI4-Mod-Setup
```

Recommended description:

```text
A Windows and macOS desktop wizard that prepares Hearts of Iron IV mods for agentic development with a selected AI provider.
```

Recommended topics:

```text
hearts-of-iron-iv
hoi4
modding
codex
ai-agents
tauri
rust
react
desktop-app
```

The public source repository is `klimPaskov/HOI4-Mod-Setup`. Keep public binary
releases gated until the real `LICENSE` file, source security review, signing,
and native-platform evidence are complete.

## Local repository bootstrap

From the source root:

```bash
git init
git switch -c main
git add .
git status --short
git diff --cached --check
git commit -m "chore: bootstrap HOI4 Mod Setup"
```

Create the GitHub repository through the GitHub website, or use GitHub CLI only after reviewing the local commit:

```bash
gh repo create klimPaskov/HOI4-Mod-Setup \
  --public \
  --source=. \
  --remote=origin
```

Push only after checking the remote and branch:

```bash
git remote -v
git push -u origin main
```

Do not include signing credentials, private mod projects, build certificates, application secrets, or local transaction evidence in the initial commit.

## Daily contribution flow

Start from the current protected branch:

```bash
git switch main
git pull --ff-only
git switch -c feat/short-description
```

Inspect before committing:

```bash
git status --short
git diff
git add -p
git diff --cached
git diff --cached --check
```

Use focused Conventional Commit messages:

```text
feat(scanner): detect nested launcher descriptors
fix(transaction): preserve recovery journal after interruption
test(security): cover junction swap during apply
docs(skills): record manifest validation workflow
```

Before opening a pull request:

```bash
git fetch origin
git rebase origin/main
pnpm lint
pnpm typecheck
pnpm test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git push -u origin HEAD
```

Use `--force-with-lease` only on a personal review branch after a rebase. Never force-push `main` or a shared branch.

## Branch naming

Use one focused branch per change:

- `feat/` for user-visible capabilities
- `fix/` for defects
- `security/` for hardening
- `refactor/` for behavior-preserving restructuring
- `test/` for test-only work
- `docs/` for documentation and living skills
- `chore/` for tooling and maintenance
- `release/` for release preparation

Avoid long-lived development branches. Merge small, complete slices through pull requests.

## Pull request rules

A pull request should state:

- user or maintainer problem
- design and tradeoffs
- files and systems changed
- schema and migration impact
- transaction and rollback impact
- credential and security impact
- platform impact
- tests and fault scenarios run
- screenshots for visible changes
- documentation and living skills updated
- known limitations

Use draft pull requests for early review. Mark a pull request ready only after required checks pass locally and the description is complete.

Preferred merge method:

- squash merge for ordinary contributions
- rebase or merge commit only when the commit sequence is intentionally useful

Delete the source branch after merge unless it is an active release branch.

## Main branch ruleset

Configure a GitHub repository ruleset for `main` with:

- pull request required before merge
- at least one approval
- stale approvals dismissed after new commits
- conversation resolution required
- required status checks
- branch must be current before merge when the queue is not used
- force pushes blocked
- branch deletion blocked
- code owner review required for sensitive paths when maintainers are available
- administrator bypass limited to emergency recovery

Recommended required checks after the source scaffold exists:

- planning and schema integrity
- frontend lint
- frontend typecheck
- frontend tests
- Rust formatting
- Rust clippy
- Rust tests on Windows
- Rust tests on macOS
- transaction fault suite for relevant pull requests
- accessibility and visual checks for UI pull requests

The repository includes a declarative baseline at `.github/rulesets/main-protected.json`. Applying it in GitHub repository settings is still an external maintainer action; the file cannot activate protection without a published repository and an authorized administrator.

Do not make a skipped job a required check. Keep check names stable once rulesets depend on them.

## CODEOWNERS

Use `.github/CODEOWNERS` to request reviews for sensitive surfaces. The initial template assigns the repository owner. Expand it to teams when contributor roles exist.

Sensitive ownership should include:

- `.github/`
- `AGENTS.md`
- `.agents/`
- `.codex/`
- transaction and recovery code
- security and credential code
- schemas and migrations
- release scripts

Protect the CODEOWNERS file itself through the same ownership rule.

## Issues and discussions

Use issue forms for:

- reproducible bug reports
- feature requests
- source manifest or workflow repository problems

Disable blank public issues at first so reports contain enough evidence. Maintainers can still open blank issues.

Use GitHub Discussions only when the project has enough maintenance capacity to answer general questions. Issues should remain actionable engineering work.

Security reports never belong in public issues. Use `SECURITY.md` and GitHub private vulnerability reporting.

## Dependency updates

Commit `.github/dependabot.yml` on the default branch. Monitor:

- npm or pnpm manifests
- Cargo manifests
- GitHub Actions

Use weekly updates by default. Group all version updates into at most one pull
request per ecosystem so dependency maintenance does not flood the repository.
Never auto-merge a dependency update that affects filesystem access,
cryptography, archive handling, Git, credentials, Tauri, updater code, or
release signing without review and tests.

Enable Dependabot alerts and security updates in repository settings. A configuration file controls version update pull requests, while alert settings remain repository settings.

## GitHub Actions security

Set default workflow permissions to read-only. Grant only the permissions required by each job.

Rules:

- use official actions where practical
- pin production release actions to reviewed immutable revisions
- never print secrets
- use protected environments for signing and release credentials
- do not expose secrets to pull requests from forks
- do not run untrusted repository scripts with write tokens
- validate artifact hashes before publication
- keep release creation in a separate job after platform builds succeed

The repository-owned workflows call `pnpm release:build`, `pnpm release:verify`, and `pnpm desktop:e2e`; the native desktop job builds and launches the application on Windows x64, `macos-15` arm64, and `macos-15-intel` x64. These checks must remain required only after the corresponding runners are active and their check names are stable.

## Releases

The public release path publishes clean, user-facing artifacts built on native
runners: one Windows `.exe` installer and one macOS `.dmg` for each supported
architecture. Each artifact needs internal SHA-256, source/tag identity, SBOM,
signature, and platform evidence. Community releases use a ChaosX
Authenticode signature on Windows and an ad-hoc code signature on the macOS
application; protected credentials upgrade those routes to Azure Artifact
Signing and Apple Developer ID signing plus notarization. The public GitHub
Release keeps the three installers clearly named and includes only the update
metadata and macOS update bundles needed by installed copies.

### Development previews

The manually dispatched `.github/workflows/development-preview.yml` builds the
same three native targets, runs the UI and native launch gates, and publishes
a user-facing semantic-version GitHub prerelease such as `0.1.1`. The
workflow refuses to reuse a published version. `scripts/prepare_preview_assets.mjs`
rechecks the exact source commit, platform/architecture metadata, package
hashes, and shared third-party notice inventory before the write-capable
release operation. The preview path is separate from the stable release path:
it does not receive stable signing credentials and its assets remain labelled
as a GitHub prerelease. The stable `Release` workflow uses verified community
signing when protected official credentials are unavailable and upgrades the
same gates when official signing is configured. Publication renames the installers to
`HOI4-Mod-Setup-windows-x64-setup.exe`,
`HOI4-Mod-Setup-macos-arm64.dmg`, and
`HOI4-Mod-Setup-macos-x64.dmg`, then generates release notes with three direct
download bullets. Package hashes, source provenance, SBOM, third-party notices,
metadata, and platform manifests remain verified CI evidence; the public
GitHub Release uploads the three installers, `latest.json`, and the two macOS
update archives. Signatures are embedded in the metadata and do not appear as
separate public files.

The protected environment sets
`HOI4_MOD_SETUP_RELEASE_PUBLISH=true`. When
`HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED=false`, Windows creates an
ephemeral ChaosX code-signing certificate, signs with SHA-256 and the reviewed
DigiCert RFC 3161 timestamp endpoint, verifies Authenticode, and deletes the
private material. macOS ad-hoc signs the application before rebuilding the
DMG and verifies the resulting signature.

When `HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED=true`, Windows uses Azure
Artifact Signing through the OIDC secrets `AZURE_CLIENT_ID`,
`AZURE_TENANT_ID`, and `AZURE_SUBSCRIPTION_ID`, plus the protected variables
`HOI4_MOD_SETUP_ARTIFACT_SIGNING_ENDPOINT`,
`HOI4_MOD_SETUP_ARTIFACT_SIGNING_ACCOUNT`,
`HOI4_MOD_SETUP_ARTIFACT_SIGNING_PROFILE`,
`HOI4_MOD_SETUP_WINDOWS_SIGNER`, and
`HOI4_MOD_SETUP_WINDOWS_TIMESTAMP_URL`. macOS uses the secrets
`HOI4_MOD_SETUP_APPLE_CERTIFICATE`,
`HOI4_MOD_SETUP_APPLE_CERTIFICATE_PASSWORD`, `HOI4_MOD_SETUP_APPLE_ID`, and
`HOI4_MOD_SETUP_APPLE_PASSWORD`, plus the variables
`HOI4_MOD_SETUP_MACOS_SIGNING_IDENTITY` and
`HOI4_MOD_SETUP_MACOS_TEAM_ID`. Temporary certificates and keychains are
removed after verification and never enter the repository or release assets.
The macOS job creates a random disposable keychain password. The Windows
official route uses the pinned Azure login and Artifact Signing actions, signs
only the generated package, and never stores a PFX or private key in the
repository.

`pnpm release:notices` derives the human-readable license inventory from the
locked pnpm and Cargo metadata. `pnpm release:sbom` (also run by
`pnpm release:build`) derives `SBOM.cdx.json` as a CycloneDX dependency
inventory from the same locked metadata. Both are included in native release
outputs, but neither waives maintainer review of bundled assets or complete
license text before publication.

Both community and official signing change package bytes after the unsigned
build manifest is written. Each platform job verifies the signature and then
regenerates the complete internal artifact manifest before curation.

After platform jobs finish, the curation job runs
`scripts/prepare_release_assets.mjs` to revalidate downloaded manifests,
source/tag/architecture identity, signed-evidence markers, package hashes, and
the exact Windows/macOS asset set before the write-capable GitHub release
operation. It uses deterministic platform filenames, refuses to reuse an
existing release tag, and publishes a normal release only after every gate
passes.

Use semantic version tags:

```text
vMAJOR.MINOR.PATCH
```

Prepare releases through a pull request that updates:

- version
- changelog
- README support status
- migrations and compatibility notes
- release notes
- affected living skills

After the release pull request merges:

```bash
git switch main
git pull --ff-only
git tag -a vX.Y.Z -m "HOI4 Mod Setup vX.Y.Z"
git push origin vX.Y.Z
```

The tag workflow builds from the exact annotated tag commit. It publishes only
after Windows and macOS package verification, signature checks, native launch
smoke tests, internal hash validation, curation, and the protected release
environment gate pass.

Version 0.2.0 introduces the signed application update channel. The app checks
the fixed latest-release metadata endpoint without blocking setup, shows only
a verified newer version, and installs only after explicit user action. The
release environment signs the final Windows installer and final macOS app
archives with the dedicated updater key after platform signing.

Never move or reuse a published tag. Fix problems in a new version.

## GitHub repository settings checklist

Enable:

- Issues
- private vulnerability reporting
- dependency graph
- Dependabot alerts
- Dependabot security updates
- branch or repository rulesets
- automatic branch deletion after merge

Configure:

- default branch `main`
- squash merge enabled
- merge queue when contributor volume justifies it
- protected release environments
- repository description and topics
- social preview after final branding exists

Decide before first public release:

- open-source license
- maintainer contact
- support policy
- stable platform matrix
- release signing ownership
- whether Discussions should be enabled

## Reference material

- [Customizing a repository](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository)
- [Issue and pull request templates](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/about-issue-and-pull-request-templates)
- [CODEOWNERS](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners)
- [Dependabot version updates](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/configure-version-updates)
- [Security policy](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/add-security-policy)
