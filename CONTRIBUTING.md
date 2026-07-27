# Contributing to HOI4 Mod Setup

Thank you for helping build HOI4 Mod Setup. Contributions should preserve its main promises: read before write, no silent overwrite, exact source identity, secrets outside projects, reversible transactions, honest optional states, and a restrained desktop interface.

## Before starting

1. Read `README.md` for the user-facing product boundary.
2. Read `DEVELOPMENT.md` for the local toolchain.
3. Read the relevant documents under `docs/`.
4. Read `AGENTS.md` and the repo-local skill that owns the surface under `.agents/skills/`.
5. Search existing issues and pull requests before opening duplicate work.

## Git setup

Fork the repository when you do not have direct write access, then clone your fork:

```bash
git clone https://github.com/<your-account>/HOI4-Mod-Setup.git
cd HOI4-Mod-Setup
git remote add upstream https://github.com/klimPaskov/HOI4-Mod-Setup.git
git fetch --all --prune
```

Contributors with direct access may clone the canonical repository and use `origin` as the shared remote.

## Branches

Create one branch per focused change from the latest `main`:

```bash
git switch main
git pull --ff-only
git switch -c feat/short-description
```

Use these prefixes:

- `feat/` for user-visible capabilities
- `fix/` for defects
- `security/` for security hardening
- `refactor/` for behavior-preserving restructuring
- `test/` for test-only work
- `docs/` for documentation
- `chore/` for tooling and maintenance

Do not commit directly to `main`. Do not combine unrelated cleanup with a feature or fix.

## Commit messages

Use Conventional Commit style:

```text
feat(scanner): detect nested launcher descriptors
fix(transaction): preserve journal after interrupted apply
docs(readme): clarify supported release packages
test(security): cover symlink swap during staging
```

A commit should describe one coherent change. Use `git add -p` to avoid staging unrelated files.

## Keeping a branch current

Before opening or updating a pull request:

```bash
git fetch upstream
git rebase upstream/main
```

Resolve conflicts locally, rerun the required checks, then push the branch. Do not force-push shared branches. A force push to your own review branch is acceptable only with `--force-with-lease` and after checking that nobody else is using it.

## Pull requests

A pull request must include:

- the user or maintainer problem being solved
- the chosen design and important tradeoffs
- files and systems changed
- tests run and meaningful skipped tests
- screenshots for visible UI changes
- schema, migration, lock, or transaction impact
- security and credential impact
- documentation and skill updates
- known limitations and follow-up work

Keep pull requests reviewable. Split a large change by stable architectural boundary when each part can be merged safely.

## Required checks

Run the commands defined by the repository scripts. The planned baseline is:

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

UI changes also require the visual and accessibility checks defined in `docs/20_testing_strategy.md`. Transaction, merge, credential, source-resolution, and filesystem changes require the relevant fault or security suites.

## Skill maintenance

When a contribution changes a repeated workflow, command, invariant, file location, validation step, or common failure mode, update the owning repo-local skill in the same pull request. Do not put one-off feature decisions into general skills.

The meta workflow is defined in `.agents/skills/hoi4-mod-setup-skill-maintenance/SKILL.md` and `docs/27_repo_local_skill_strategy.md`.

## Agent and subagent workflow

The root `AGENTS.md` is authoritative for Codex work in this repository. Use the owning skill for recurring workflows. Spawn project subagents only when their narrow audit or documentation role materially improves confidence. Every subagent uses `fork_context=false` and receives explicit files, constraints, allowed writes, tests, and handoff path.

The parent contributor or agent reviews every subagent handoff and remains responsible for the pull request.

## Documentation

Keep the root `README.md` user-facing. Do not add build commands, branch policy, internal architecture, or contributor-only troubleshooting to it. Put those details in `CONTRIBUTING.md`, `DEVELOPMENT.md`, `RELEASING.md`, or `docs/`.

Update documentation in the same pull request when behavior, commands, configuration, schema, security policy, supported platforms, or release behavior changes.

## Security

Never commit real credentials, private project data, signing keys, Apple notarization credentials, Windows certificate material, or unredacted logs. Follow `SECURITY.md` for vulnerability reports.

## Merge policy

`main` should require a pull request, passing required checks, and review. Squash merge is the preferred default for ordinary contributions. Preserve a multi-commit history only when the commits are intentionally reviewable and useful independently.
