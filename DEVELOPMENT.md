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

`pnpm-workspace.yaml` explicitly permits the reviewed `esbuild` install
script, which is required for Vite builds under pnpm 11's strict dependency
build policy. Review any future dependency that needs an install script and
add it explicitly; never enable all dependency scripts in CI.

Read `AGENTS.md` before coding. Then read the repo-local skill that owns the current work.

## Planned source responsibilities

- React and TypeScript render the desktop wizard and editable review state.
- Tauri commands provide a typed boundary between the UI and Rust.
- Rust core modules own source resolution, scanning, merging, transactions, credentials, Git, MCP health, and readiness.
- Platform adapters own Windows, macOS, and Linux differences.
- Schemas define persisted project, plan, lock, journal, scan, conflict, and readiness data.
- Tests use fake adapters so safety behavior can run without modifying real projects or using paid services.
- `crates/hoi4-setup-core` is the portable core shared with the web edition. It
  builds for `wasm32-unknown-unknown` and reaches files, HTTP, time, and
  randomness only through the traits in `adapters.rs`; `MemoryFs` holds a
  dropped folder or a fake project. It holds the models, manifest rules,
  merge rules, coding-client registry, live-testing routes, MCP formats and
  target, descriptors, the project profile, source adaptation, deterministic
  proposals, analysis validation with the hosted-provider request and reply
  rules, image-generation guidance and client routes (`image_generation.rs`),
  context-attachment rules (`context.rs`), portrait settings (`portrait.rs`),
  export and maintenance planning, and the ZIP writer. The desktop crate re-exports
  moved modules at their old `crate::` paths. CI builds and lints it with
  `cargo build -p hoi4-setup-core --target wasm32-unknown-unknown`.
- `crates/hoi4-setup-web` is the WebAssembly binding for the web edition
  (`Project` with scan, evidence, and managed removal; `Source` with preview
  and selection; `Export` with review, merge bases and previews, readiness,
  the launcher file, and the ZIP; `analysisTurn`, `readAnalysisReply`, and
  `contextDocument`). Its modules mirror the desktop plan builder: `build.rs`
  adapts each verified file with the shared pipeline and adds the generated
  project files. The
  page lives in `src/web/` and `web/index.html`; a module worker
  (`src/web/worker.ts`) runs the core, downloads selected files, and builds the
  ZIP. `vite.web.config.ts` adds the Content Security Policy to the built page,
  and `scripts/build_web.mjs` pins every bundle with SRI and writes
  `build-info.json` with each file's SHA-256.

- The desktop scan walks the project once (`ProjectWalk` in
  `src-tauri/src/scanner.rs`). Each directory is enumerated a single time and
  its names feed the targeted setup inventory and the project profile
  (`src-tauri/src/project_profile.rs` over the core `Profiler`), each with its
  own exclusions and budgets. Test the profile on its own through
  `project_profile::scan`, which runs the same walk with the setup inventory
  off.

The exact folder layout should be recorded in AGENTS and the owning skills after the source scaffold is accepted.

## Development commands

The repository should expose stable scripts for normal work:

```bash
pnpm lint
pnpm typecheck
pnpm test
pnpm test:rust
pnpm test:e2e:install
pnpm test:e2e
pnpm test:mcp-live
pnpm test:scratch-live
pnpm test:codex-live
pnpm tauri dev
pnpm web:dev
pnpm web:build
pnpm test:web
pnpm test:relay
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The web edition needs the `wasm32-unknown-unknown` Rust target and the
`wasm-bindgen` CLI at the exact version in `Cargo.lock` (`cargo install
wasm-bindgen-cli --version <version> --locked`); `scripts/build_web.mjs` stops
with that command when the versions differ. `pnpm web:dev` builds the core
and serves the page at `http://localhost:1430/`. `pnpm web:build` writes
`dist-web/`, and `pnpm test:web` runs the built page in Chromium, Firefox,
and WebKit (`pnpm exec playwright install chromium firefox webkit` first).
`node scripts/check_web_build.mjs` checks the CSP, SRI, connect hosts, and
size budgets. The Pages workflow publishes the build at `/app/`.

`pnpm test:relay` tests the free assistant relay in `relay/` with Node's test
runner; deploying it is the manual workflow described in `relay/README.md`.

`pnpm test:rust` is the stable Rust test entry point. It bounds the default
test concurrency at four threads so local runs do not exhaust the machine; set
`HOI4_TEST_RUST_THREADS` to an integer from 1 through 32 when a different
bound is needed. The script rejects values outside that range and forwards the
selected bound to the Rust test runner.

For opt-in offline lifecycle/source/scanner acceptance, set
`HOI4_MOD_SETUP_SOURCE_REPOSITORY` to an explicit absolute committed package
checkout and run `pnpm test:offline`. Keep its historical Git objects when
testing the public 0.3.5 upgrade. The script creates temporary application
profiles and a synthetic 50,000-gameplay-file mod, retains the installed Rust
toolchain homes, and runs source checks and real core transactions serially.
Provider replies and external install actions are fakes. No shared desktop,
real project, provider credential, paid service, or user MCP registration is
used. `pnpm docs:screenshots` regenerates the sanitized public walkthrough
images through Playwright without focusing a desktop window.
The fixture also bypasses real Meshy-vault discovery: redirected profile
folders alone do not isolate Credential Manager or Keychain. Missing source
history and failed prerequisites stop the gate before scanner fixture creation.

`pnpm test:e2e:install` installs Playwright's Chromium and its platform
dependencies. `pnpm test:e2e` runs the actual React wizard in an isolated
headless browser against synthetic development fixtures, including keyboard
selection, signed-out management, recovery details, runtime errors, and narrow
viewports. For an existing local browser, set `HOI4_TEST_BROWSER_PATH` to its
absolute executable path instead of installing Chromium. These browser tests
do not replace native IPC, authentication, installation, or rollback tests.
`pnpm test:mcp-live` is a Windows-only smoke for the manifest-pinned HOI4 Agent
Tools package. It installs into a temporary prefix with npm user/global
configuration isolated, verifies registry integrity, the complete package
tree, the runtime entry, and the live initialize/tools-list contract. It does
not invoke a tool or require an AI provider credential.
Its fresh npm installation has a ten-minute bound for cold Windows dependency
extraction; metadata requests retain a two-minute bound. A timeout names the
stage and a retry creates another isolated prefix.
The initialize/tools-list probe has a two-minute cold-start bound and uses the
version in `package.json` for its client identity.
`pnpm test:codex-live` is a manual Windows smoke for the signed installed
Codex executable named by `HOI4_CODEX_EXECUTABLE`. It reads only the account
type from the current Codex home, then uses a temporary home to start and cancel
browser and device-code login attempts. It never opens a login URL or prints an
email, account ID, verification code, token, or rate-limit value; completing a
sign-in still requires a user-owned browser session and is not automated.

`pnpm test:scratch-live` requires an explicit committed source checkout in
`HOI4_MOD_SETUP_SOURCE_REPOSITORY`. It runs real setup transactions with Claude
Code and Codex as the primary client in turn. Each scratch mod includes both
clients, the full selected skill set, Agent Tools and GPTImage. It installs their
actual runtimes and performs initialize/tools-list handshakes. Only setup
analysis is a fixture; no sign-in, paid image call, or game operation runs.
Profiles, registrations and projects are temporary. Other optional servers
need their own prerequisites and separate checks.

On Windows, activate the MSVC environment before the all-feature Rust gates
when using PowerShell:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && pnpm test:rust'
cargo check --manifest-path fuzz/Cargo.toml --bins
$env:HOI4_MOD_SETUP_TAURI="1"; $env:HOI4_MOD_SETUP_BUNDLE="nsis"; pnpm release:build
pnpm release:verify
pnpm desktop:e2e
pnpm installer:e2e
```

The Windows installer smoke test is destructive only to the temporary install
root it creates. It fails closed if its fixed current-user registry keys cannot
be inspected, if either key already exists, or if a matching legacy
machine-wide installation is present. Run it from a normal user session; do
not clear a real installation to make the test pass.

On Linux x64, use Ubuntu 22.04 for release compatibility, or Ubuntu 24.04 for
native development. The dependency script installs GTK, WebKitGTK 4.1,
libsecret, D-Bus, `secret-tool`, and the virtual display runtime:

```bash
bash scripts/install_linux_dependencies.sh
pnpm install --frozen-lockfile
RUNNER_OS=Linux RUNNER_ARCH=X64 HOI4_MOD_SETUP_TAURI=1 HOI4_MOD_SETUP_BUNDLE=appimage,deb pnpm release:build
HOI4_MOD_SETUP_REQUIRE_TAURI=1 HOI4_MOD_SETUP_BUNDLE=appimage,deb pnpm release:verify
WEBKIT_DISABLE_DMABUF_RENDERER=1 xvfb-run -a dbus-run-session -- pnpm desktop:e2e
WEBKIT_DISABLE_DMABUF_RENDERER=1 xvfb-run -a dbus-run-session -- pnpm installer:e2e
```

The local installer command extracts, launches, and removes both packages in a
fresh temporary profile. Only a disposable dedicated runner may set
`HOI4_MOD_SETUP_DEDICATED_LINUX=1` to perform actual system `dpkg` installation
and purge; it refuses an existing application or package registration. Never
set it on a shared workstation. The native harness supervises process groups,
bounds launch/termination, and isolates XDG application data. GTK/WebKit runtime
and a working Secret Service remain OS prerequisites for AppImage users;
`/usr/bin/secret-tool` is required for hosted API-provider credential storage.
Linux package source/version/architecture/hash checks are independent of real
launcher/game loading or real-account sign-in. Locally rebuilt packages are
inspection artifacts; stable release evidence must bind a clean annotated-tag
checkout and pass final updater-signature verification.

`scripts/test_linux_credentials.sh` runs the exact ignored native vault test
inside a new HOME/XDG profile and D-Bus session, with a temporary unlocked
keyring and a synthetic value. It verifies save/read/delete and opaque-reference
serialization without touching the desktop vault. CI explicitly removes
`HOI4_TEST_IMAGE_VAULT_ADAPTER`; local developers may set it only to crosscheck
an explicitly selected source Python vault adapter. Core-only Linux fuzz builds
still need `pkg-config` and `libdbus-1-dev` because Secret Service is a native
dependency of the core, independent of the optional Tauri desktop feature.

The Tauri CLI uses the workspace-root `target/release` directory for native
bundles; the release and launch-smoke scripts also accept the alternate
`src-tauri/target/release` layout used by standalone Tauri projects.

Transaction fault tests, security tests, accessibility tests, and release verification should also have repository-owned commands. Update this file and the owning skill when command names change.

## Planning and template validation

Before application source exists, or after changing planning, GitHub, AGENTS, skills, subagents, schemas, or examples:

```bash
python -m pip install jsonschema PyYAML
python scripts/validate_repository.py
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
- pre-existing CLAUDE/AGENTS instructions, canonical or legacy skills,
  subagents, native coding-client, and MCP configuration
- local modifications to managed files
- symlinks, junctions, case collisions, locked files, and cloud-synced folders
- large projects and large wiki trees

Never commit a real `MESHY_API_KEY` or a paid-provider response containing private data.

## Agent workflow

Repo-local skills under `.agents/skills/` are living workflow documentation. Update the owning skill in the same pull request when commands, paths, invariants, platform behavior, validation, or common recovery steps change.

Narrow subagents under `.codex/agents/` can audit source manifests, scans, transactions, security, UI, platforms, docs, or skills. They use `fork_context=false`, explicit scope, and handoffs under `docs/development/handoffs/`.

## UI development

Use the seven-phase navigation and minimal screen references. Screens that hold
no decision are skipped in both directions: `nextScreen` and `previousScreen`
in `src/App.tsx` share one predicate per screen (for example
`integrationReviewNeeded` for the MCP and credentials review), and the browser
flow in `tests/browser/wizard.spec.ts` checks Back and Next together. Do not turn developer diagnostics into permanent visible panels. Test keyboard behavior, focus order, screen readers, reduced motion, long paths, 200 percent scaling, and platform-specific file pickers.

## Platform work

Windows, macOS, and Linux adapters must be tested on their actual platforms. Do not mark a route supported based only on compilation or similarity to another operating system.

Signing, notarization, credential stores, filesystem links, application paths,
and opening the selected coding environment need clean-machine evidence.

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

The browser harness builds development-only synthetic routes into the ignored
`artifacts/browser-fixture` directory, then serves that fixture on loopback
port 1421. It never reuses a running user app or enables fixtures in the normal
production output. Build readiness is awaited before browser tests start, so
Vite cold module compilation is not charged to the first wizard assertion.
