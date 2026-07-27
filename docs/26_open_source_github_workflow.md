# Open-source Git and GitHub workflow

This document covers the source repository for HOI4 Mod Setup. It is separate from the Git setup that the application can perform inside a user's mod project.

## Repository identity

Recommended repository name:

```text
HOI4-Mod-Setup
```

Recommended description:

```text
A Windows and macOS desktop wizard that prepares Hearts of Iron IV mods for agentic development in Codex.
```

Recommended topics:

```text
hearts-of-iron-iv
hoi4
modding
codex
tauri
rust
react
desktop-app
```

Keep the repository public only after a real `LICENSE` file is selected and the initial security review is complete.

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

Use weekly updates by default. Group routine compatible updates where it keeps pull request volume manageable. Never auto-merge a dependency update that affects filesystem access, cryptography, archive handling, Git, credentials, Tauri, updater code, or release signing without review and tests.

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

The repository-owned workflows call `pnpm release:build`, `pnpm release:verify`, and `pnpm desktop:e2e`; the native desktop job builds and launches the application on Windows x64 and both currently supported macOS runner architectures. These checks must remain required only after the corresponding runners are active and their check names are stable.

## Releases

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

The tag workflow builds from the exact tag commit. It should publish a draft release first. Verify Windows and macOS installation, signatures, notarization, checksums, updater metadata, and clean-machine behavior before publication.

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
