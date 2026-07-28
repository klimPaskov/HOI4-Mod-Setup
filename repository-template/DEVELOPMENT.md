# Development setup

This document is for contributors building HOI4 Mod Setup from source. End users should use `README.md` and official GitHub Releases.

## Planned toolchain

- Git
- Node.js 22 or the version declared by the repository toolchain file
- Corepack and pnpm
- Rust 1.88.0 toolchain from `rust-toolchain.toml`
- Tauri platform prerequisites
- Python 3.13 or the version declared by repository automation for planning and schema validation

The repository must pin or declare tool versions before implementation depends on them. Do not copy a temporary local version into contributor instructions.

## Clone and prepare

```bash
git clone https://github.com/klimPaskov/HOI4-Mod-Setup.git
cd HOI4-Mod-Setup
corepack enable
pnpm install --frozen-lockfile
rustup toolchain install 1.88.0 --profile minimal
rustup default 1.88.0
rustup component add rustfmt clippy --toolchain 1.88.0
```

Read `AGENTS.md` before coding. Then read the repo-local skill that owns the current work.

## Planned source responsibilities

- React and TypeScript render the desktop wizard and editable review state.
- Tauri commands provide a typed boundary between the UI and Rust.
- Rust core modules own source resolution, scanning, merging, transactions, credentials, Git, MCP health, and readiness.
- Platform adapters own Windows and macOS differences.
- Schemas define persisted project, plan, lock, journal, scan, conflict, and readiness data.
- Tests use fake adapters so safety behavior can run without modifying real projects or using paid services.

The exact folder layout should be recorded in AGENTS and the owning skills after the source scaffold is accepted.

## Development commands

The repository should expose stable scripts for normal work:

```bash
pnpm lint
pnpm typecheck
pnpm test
pnpm test:e2e
pnpm tauri dev
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

On Windows, activate the MSVC environment before the all-feature Rust gates
when using PowerShell:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace --all-features'
cargo check --manifest-path fuzz/Cargo.toml --bins
$env:HOI4_MOD_SETUP_TAURI="1"; $env:HOI4_MOD_SETUP_BUNDLE="nsis"; pnpm release:build
pnpm release:verify
pnpm desktop:e2e
```

The Tauri CLI uses the workspace-root `target/release` directory for native
bundles; the release and launch-smoke scripts also accept the alternate
`src-tauri/target/release` layout used by standalone Tauri projects.

Transaction fault tests, security tests, accessibility tests, and release verification should also have repository-owned commands. Update this file and the owning skill when command names change.

## Planning and template validation

Before application source exists, or after changing planning, GitHub, AGENTS, skills, subagents, schemas, or examples:

```bash
python -m pip install jsonschema PyYAML
python scripts/validate_repository_templates.py
python scripts/check_committed_secrets.py
```

## Test projects

Use synthetic fixtures and temporary directories. Do not use a private personal mod as the only reproduction.

Fixtures should cover:

- clean new project
- established existing project
- malformed descriptors
- nested Git repositories
- dirty Git state
- pre-existing AGENTS, skills, subagents, Codex, and MCP configuration
- local modifications to managed files
- symlinks, junctions, case collisions, locked files, and cloud-synced folders
- large projects and large wiki trees

Never commit a real `MESHY_API_KEY` or a paid-provider response containing private data.

## Agent workflow

Repo-local skills under `.agents/skills/` are living workflow documentation. Update the owning skill in the same pull request when commands, paths, invariants, platform behavior, validation, or common recovery steps change.

Narrow subagents under `.codex/agents/` can audit source manifests, scans, transactions, security, UI, platforms, docs, or skills. They use `fork_context=false`, explicit scope, and handoffs under `docs/development/handoffs/`.

## UI development

Use the seven-phase navigation and minimal screen references. Do not turn developer diagnostics into permanent visible panels. Test keyboard behavior, focus order, screen readers, reduced motion, long paths, 200 percent scaling, and platform-specific file pickers.

## Platform work

Windows and macOS adapters must be tested on their actual platforms. Do not mark a route supported based only on compilation or similarity to another operating system.

Signing, notarization, credential stores, filesystem links, application paths, and Open in Codex behavior need clean-machine evidence.

## Git hooks

Local hooks may run formatting and quick tests, but repository correctness must not depend on an uncommitted local hook. CI remains the shared gate.

## Troubleshooting

When a command fails:

1. record the exact command and tool versions
2. determine whether the failure is source, dependency, platform, fixture, or environment related
3. reduce it to a synthetic reproduction
4. add a regression test when the failure belongs to the repository
5. update the owning skill when the recovery method is reusable

Do not put private absolute paths or secrets into issues or logs.

## Codex App Server development

A compatible official Codex installation is required for real authentication and semantic-analysis tests. Verify `codex app-server` before starting the desktop app. Ordinary CI uses redacted protocol fixtures and never uses a real ChatGPT credential.

Do not add tokens or account exports to `.env`, fixtures, snapshots, or logs. Manual authentication tests use a developer-owned ChatGPT account outside CI.

## Required auth fixtures

Development uses a fake App Server for normal tests. Real-account tests are opt-in and must not run in CI. The fake covers browser login, device code, account updates, usage limits, schema-valid turns, malformed turns, process exit, and redacted logs.
