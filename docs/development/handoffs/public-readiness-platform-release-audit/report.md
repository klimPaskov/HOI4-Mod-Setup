# Public readiness platform and release audit

Audit date: 2026-09-28  
Repository: `C:\Users\klimp\Documents\Projects\hoi4-mod-setup`  
Branch: `codex/public-readiness-2026-09-28`  
Audited release line: `0.3.5`  
Mode: read only; this handoff is the only written artifact.

## Verdict

The candidate is not release ready. The release implementation is substantially
specified and the workflow has useful source, signature, artifact, draft, and
tag gates, but this checkout has no release identity for the candidate, a dirty
worktree, no macOS package, no native signing evidence, no clean-machine update
evidence, and no real managed ChatGPT login completion evidence. The local
Windows package is an inspection artifact only.

The existing public `v0.3.5` release is a separate, completed release for
commit `f78d65dbd516a0366be2fecb3a6372a8bf3fb4b2`. The candidate is at
`249908a8f976fe5d2c1894eee2925f8fc0b007e5`, has no tag, and has uncommitted
changes. It must not reuse `v0.3.5`.

## Platform and artifact matrix

| Target | Workflow runner and bundle | Public artifact name | Updater target | Candidate checkout evidence | Status |
| --- | --- | --- | --- | --- | --- |
| Windows x64 | `windows-latest`, NSIS | `HOI4-Mod-Setup-windows-x64-setup.exe` | `windows-x86_64` | `dist/release/packages/nsis/HOI4 Mod Setup_0.3.5_x64-setup.exe`, 6,497,630 bytes; PE bootstrap machine `0x014c`; Authenticode `NotSigned`; `BUILD_METADATA.json` says `platform=win32`, `architecture=unresolved-local`, `signing=not_configured` | Inspection only; not release evidence |
| macOS arm64 | `macos-15`, DMG | `HOI4-Mod-Setup-macos-arm64.dmg` | `darwin-aarch64` | No local DMG or app archive | Missing candidate evidence |
| macOS x64 | `macos-15-intel`, DMG | `HOI4-Mod-Setup-macos-x64.dmg` | `darwin-x86_64` | No local DMG or app archive | Missing candidate evidence |

The native Windows executable beside the local package is
`target/release/hoi4-mod-setup.exe`: PE machine `0x8664`, PE optional header
`0x020b`, GUI subsystem `2`, and Authenticode `NotSigned`. The NSIS bootstrap
being 32-bit is allowed by the release skill; the native executable is the
architecture gate. No local macOS architecture, codesign, or notarization
result exists.

The checked-in package and updater contract is in
[`src-tauri/tauri.conf.json`](../../../src-tauri/tauri.conf.json): targets are
`nsis` and `dmg`, Windows updater mode is `passive`, the endpoint is the fixed
`releases/latest/download/latest.json` URL, and a public Tauri updater key is
committed. The candidate has no `latest.json`, updater archive, or `.sig` file
under `dist/release`.

## Source and version identity

The four application version sources agree at `0.3.5`:

- [`package.json`](../../../package.json): `0.3.5`
- [`src-tauri/tauri.conf.json`](../../../src-tauri/tauri.conf.json): `0.3.5`
- [`src-tauri/Cargo.toml`](../../../src-tauri/Cargo.toml): `0.3.5`
- [`Cargo.lock`](../../../Cargo.lock), package `hoi4-mod-setup`: `0.3.5`

The local release metadata records source revision
`249908a8f976fe5d2c1894eee2925f8fc0b007e5`, which equals the checked-out
`HEAD`, but it does not include the uncommitted transaction and schema edits.
`scripts/release_build.mjs` refuses release mode unless `GITHUB_SHA` is a full
40-character revision equal to `HEAD`, the ref is a semantic tag, the tag peels
to that revision, and the worktree is clean (lines 69-95). The candidate does
not satisfy that state.

`pnpm release:verify` passes only its non-native/frontend path and explicitly
prints that platform verification is a CI step. With
`HOI4_MOD_SETUP_REQUIRE_TAURI=1`, it fails because the local metadata has
`architecture=unresolved-local`. With strict release identity enabled it first
fails because `GITHUB_SHA` is absent. This is the intended fail-closed behavior.

The current `CHANGELOG.md` has an Unreleased section describing the transaction
root binding and schema 1.1 changes. Before publication, the parent should
choose the next semantic version, run the repository version script, update the
release notes and compatibility/migration notes, and create a new annotated tag
from the merged protected `main` commit.

The existing remote `v0.3.5` tag is annotated and points to `f78d65d`, but its
GitHub tag object reports an unsigned tag signature. The release workflow only
requires an annotated tag and exact commit binding. The tag is already
published and the tag ruleset blocks deletion and non-fast-forward changes.

## Signing and notarization status

The implementation has separate community and official routes:

- Community Windows signing imports the protected `ChaosX` PFX into the
  runner, applies untimestamped SHA-256 Authenticode, verifies the publisher,
  and removes the temporary PFX.
- Community macOS signing applies an ad-hoc signature, verifies it, and
  rebuilds the DMG without Apple secrets.
- Official Windows signing uses the pinned Azure Artifact Signing action and
  the reviewed DigiCert RFC 3161 timestamp URL.
- Official macOS signing uses the protected Developer ID certificate, a
  disposable keychain, `notarytool`, and `stapler` validation.
- Both routes sign the final Tauri updater package with
  `TAURI_SIGNING_PRIVATE_KEY` and verify it against the committed public key
  before curation.

The live release environment currently reports
`HOI4_MOD_SETUP_RELEASE_PUBLISH=true` and
`HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED=false`. Visible repository secrets
include the two community Windows PFX names; visible release-environment
secrets include only the two updater-key names. The official Azure and Apple
secret names required by [`RELEASING.md`](../../../RELEASING.md) are not listed
at repository or release-environment scope. Organization-level secret
inheritance is not observable from this audit, so official signing should be
treated as unverified. The configured publication route is the community route
until the protected variable is changed after the official credentials have
been independently verified.

The local package is definitively unsigned. No local certificate, notarization,
or signing evidence was found. This is expected for a local inspection build,
but it cannot support a public release claim.

## Workflow permissions and repository state

The workflow structure is sound and the repository-owned
`scripts/check_workflow_security.py` check passed in
`artifacts/public-readiness/workflows.log`:

- default and build/gate jobs use read-only contents permission;
- only the publication job receives `contents: write`;
- only Windows signing receives `id-token: write`;
- signing jobs use the protected `release` environment and do not check out
  source;
- third-party actions are pinned to full commit SHAs;
- the stable workflow is tag-only; the preview workflow owns manual dispatch;
- release publication consumes only the curated artifact.

The live GitHub rulesets are active. `Protect main` requires a pull request,
one approval, code-owner review, resolved threads, strict required checks, and
current branch state. Required checks include repository integrity, frontend,
Rust on Ubuntu/Windows/macOS, fuzz compilation, and desktop E2E on Windows,
macOS arm64, and macOS x64. `Protect stable release tags` blocks deletion and
non-fast-forward changes. The main ruleset also exposes a maintainer bypass
actor with `always`; this is broader than the documented recommendation to
limit administrator bypass to emergency recovery and should be reviewed.

There is no GitHub pull request for `codex/public-readiness-2026-09-28`; the
candidate is local. The four live Dependabot PRs are all review-blocked:

| PR | Scope | Live checks | Release implication |
| --- | --- | --- | --- |
| #72 | GitHub Actions group | Reported checks passed | Still needs required review and candidate/main comparison |
| #73 | Cargo group | Rust Ubuntu/Windows/macOS, fuzz, and desktop E2E failures; other checks passed | Must be repaired or explicitly deferred before a release based on it |
| #75 | Vitest 4.1.11 | Reported checks passed | Still needs required review; candidate already contains the Vitest 4.1.11 dependency change |
| #76 | npm group | Frontend, cargo-audit, and all three desktop E2E checks failed; other checks passed | Must be repaired or explicitly deferred before a release based on it |

The candidate itself also changes the release workflow to install the pinned
Playwright browser before the gate. That change is not in the local package
metadata and has no tag-bound CI result yet.

## Checksums, provenance, and draft verification

`scripts/release_build.mjs` writes `ARTIFACTS.sha256`, `BUILD_METADATA.json`, a
CycloneDX SBOM, and third-party notices. The local Windows manifest hashes the
unsigned installer as:

`d5ce48283b08950ce290c243e5eb01c1be9a5fec2e43cd5c8c46d7bb88ef4644`

That hash is useful for local inspection only. It is not final release
provenance because signing changes package bytes and the local metadata has no
runner architecture, signature evidence, or tag identity. The curation script
requires every platform manifest to cover every file, binds metadata to
`GITHUB_SHA` and the release tag, requires package-bound signing evidence, and
generates `latest.json` from the final updater signatures. Checksums and
provenance intentionally remain internal CI evidence rather than public root
files.

The stable workflow creates a draft with the exact six public assets (three
installers, two macOS updater archives, and `latest.json`), verifies the remote
asset-name set, downloads all six, compares every downloaded byte with the
curated bytes, and only then undrafts the release. It refuses to create a
release when that tag already has a release. The workflow and
`RELEASING.md` both require withdrawal plus a new version when an artifact is
wrong; they do not reuse a published tag.

The existing public `v0.3.5` release has the expected six assets and a remote
`latest.json` with all three updater platform entries and non-empty updater
signatures. This audit did not download and independently reverify those large
public packages, so that is historical release evidence, not candidate
evidence.

## Clean-machine and lifecycle evidence

The release workflow declares the right gates in
[`.github/workflows/release.yml`](../../../.github/workflows/release.yml):

- `pnpm release:build` and `pnpm release:verify` on each native runner;
- `pnpm desktop:e2e` to launch the built native binary and stop it through a
  bounded process route;
- `pnpm installer:e2e` to install, launch, and remove the package;
- native transaction tests for first-install resume with a launcher path and
  preserved thumbnail, modification-preserving repair/removal, and exact
  apply/rollback hash restoration.

The Windows installer helper fails closed if fixed HKCU product keys or a
matching legacy machine installation are present, bounds registry operations,
removes only state whose install path exactly belongs to its temporary root,
and verifies temporary-root cleanup. The macOS helper mounts the DMG, copies
the app to a temporary Applications folder, launches it, removes it, and
detaches the image.

The current workspace has no release-run log for `desktop:e2e` or
`installer:e2e`, no macOS package, and no clean-machine evidence for the
candidate. The available `artifacts/public-readiness` logs show frontend,
browser, release-asset unit, repository, and Rust results, but do not prove
native package install or update. The installer script does not exercise an
application update, updater retry, or restart; the release workflow likewise
verifies updater signatures and metadata but does not run an installed-app
update on a clean machine.

The Rust descriptor implementation is deterministic and has focused tests:

- `descriptor.mod` includes `picture="thumbnail.png"`;
- `render_launcher_descriptor` uses the canonical project root and Windows
  user-facing slash/case behavior;
- the placeholder is a valid 600x600 black PNG;
- transaction validation rechecks both descriptors and PNG bytes;
- the focused native lifecycle test names cover launcher path and preserved
  thumbnail, repair/removal preservation, and apply/rollback restoration.

There is still no native Windows or macOS clean-machine proof that a newly
created project is discovered by the HOI4 launcher, that both descriptors point
to the intended redirected Documents location, or that a user-replaced
thumbnail survives repair and rollback. Those are required release evidence,
not conclusions that can be inferred from Rust unit tests.

## Platform paths and credential stores

The platform-neutral core keeps path validation, descriptors, manifests,
transactions, hashing, readiness, and updater policy in Rust. Platform adapters
provide the native differences:

- Windows uses `SHGetKnownFolderPath` for Documents and Downloads, strips
  internal `\\?\\` prefixes only for user-facing launcher text, rejects
  reparse-point components, and uses `keyring` with the `windows-native`
  feature for Windows Credential Manager.
- macOS uses `NSSearchPathForDirectoriesInDomains` for Documents and Downloads,
  allows only the documented `/etc`, `/tmp`, and `/var` aliases while rejecting
  project links, and uses `keyring` with the `apple-native` feature for
  Keychain Services.
- application settings/cache paths are `%APPDATA%`/`%LOCALAPPDATA%` on
  Windows and `~/Library/Application Support`/`~/Library/Caches` on macOS.

`OsCredentialStore` uses the shared service
`com.klimpaskov.hoi4-mod-setup`, stores only opaque references, and injects
only scoped process environment names. Windows has a bounded legacy Meshy
credential enumeration path. macOS has no analogous discovery path; it relies
on the stable Keychain reference. The tests cover memory-store behavior,
reference validation, redaction, and a Windows bounded enumeration call, but
there is no native macOS Keychain test or clean-machine Windows save/read/delete
evidence in this checkout. Before public release, collect both platform vault
evidence without recording secret values.

## Unsupported external workflow states

The checked-in source manifest and source-side tests are honest about current
platform boundaries:

- `mcp.hoi4_agent_tools`, its bootstrap, and the client-specific MCP
  registrations declare Windows only. Their `.cmd`, current-user npm/Node, and
  Windows-oriented routes have no verified macOS equivalent.
- `workflow.3d` declares Windows only and explicitly says the inspected source
  is Windows-oriented with no macOS route. Missing or rejected `MESHY_API_KEY`
  leaves it incomplete without blocking core setup.
- Super Events declares `all` and has no external command route in the
  manifest, so it remains a platform-neutral optional workflow.
- The Rust source test
  `core_profile_keeps_windows_only_mcp_components_nonblocking_on_macos` asserts
  `unsupported_platform` for MCP and its bootstrap while leaving the rest of
  the core profile unblocked.

The app advertises only Windows and macOS as desktop platforms. It does not
declare Linux packaging, and no Linux release artifact should be inferred from
the Ubuntu validation job.

## Codex App Server and managed ChatGPT evidence

The implementation has the intended ownership boundary:

- `ProcessJsonlTransport::start` launches the reviewed, publisher-verified
  Codex executable with `app-server --stdio`, clears and rebuilds a bounded
  environment, sets Windows `CREATE_NO_WINDOW`, supervises the child, and
  closes stdio cleanly.
- `find_codex_executable` checks the Windows managed desktop locations, a
  narrow PATH lookup, and the reviewed macOS ChatGPT/Codex app-bundle paths;
  publisher and hash checks happen before account state or secrets are used.
- Protocol code enforces initialize before account/thread requests, requires
  ChatGPT account type, checks rate limits, supports `chatgpt` browser login
  and `chatgptDeviceCode`, waits for completion and account-update
  notifications, supports cancellation and logout, and redacts errors.
- The fixed system browser opener is Windows `explorer.exe` or macOS
  `/usr/bin/open`; returned login URLs are HTTPS-only and do not accept user
  info or ports.

The fixture and unit coverage is strong: initialize ordering, interrupted
stdio, browser/device start requests, cancellation, logout, rate limits,
redaction, and proposal schema rejection appear in `src-tauri/src/codex.rs`
tests and the final Rust log records 406 passing tests. It is not a release
compatibility proof yet:

1. `scripts/test_live_codex.mjs` is an opt-in Windows-only smoke. It starts the
   live process as `app-server` without the production `--stdio` argument and
   only starts/cancels browser and device-code attempts. It does not complete
   either login, test macOS, or exercise app launch from a signed package.
2. No `test_live_codex` result is present in the current evidence directory.
3. `RELEASING.md` requires clean browser and device-code completion, signed-out
   recovery, usage-limit handling, App Server interruption, output-schema
   rejection, redaction, and no-account-data persistence before publication;
   only fixture/unit evidence is present for those gates.
4. `validate_initialize_response` and the live smoke bind the response user
   agent to `hoi4-mod-setup/<application version>`. A real official Codex
   initialize response has not been captured in this checkout, so the claimed
   compatibility of that exact metadata contract remains unverified.

The parent should run a redacted, developer-owned live gate on both supported
platforms using the exact production invocation, complete one browser login and
one device-code fallback, then exercise logout, rate-limit and interruption
recovery. No account email, ID, plan, rate-limit values, login URL/code, or
tokens may enter the evidence.

## Recommended parent actions

1. Keep this candidate blocked from release. Review the transaction/schema 1.1
   edits, update the release version and migration notes, merge through the
   active main ruleset, and tag the resulting exact `main` commit with a new
   version.
2. Resolve or explicitly defer the failed Dependabot PRs (#73 and #76), review
   #72 and #75, and compare their dependency changes with the candidate before
   merging. Re-run the required status checks on the final merge commit.
3. Decide whether the first public build is the documented community route or
   official Windows/Apple signing. If community is used, record the ChaosX
   Authenticode and macOS ad-hoc limitations accurately. Do not claim Apple
   Developer ID or notarization while `HOI4_MOD_SETUP_RELEASE_SIGNING_CONFIGURED`
   is false.
4. Run the tag workflow and retain the three platform manifests, package-bound
   signature evidence, SBOM/notices, source/tag binding, updater signature
   verification, and exact draft-to-remote byte comparison.
5. Add or run the missing clean-machine evidence for Windows x64, macOS arm64,
   and macOS x64: install, launch, update, failed-update retry, uninstall/app
   removal, redirected Documents resolution, credential-store round trip,
   launcher descriptor discovery, thumbnail decode, modified-thumbnail repair,
   and rollback hash restoration.
6. Close the Codex compatibility gap with exact `app-server --stdio` live
   tests on Windows and macOS, including managed browser/device-code completion
   and safe recovery. Keep the live smoke's output fully redacted.
7. Review the main ruleset's `always` bypass actor and limit it to the intended
   emergency governance path if that actor is not deliberate.

## Exact evidence index

- Platform architecture: [`docs/16_platform_architecture.md`](../../../docs/16_platform_architecture.md)
- Release process and credential requirements: [`RELEASING.md`](../../../RELEASING.md)
- Release skill: [`SKILL.md`](../../../.agents/skills/hoi4-mod-setup-open-source-release/SKILL.md)
- Stable workflow: [`.github/workflows/release.yml`](../../../.github/workflows/release.yml)
- Release build/verify/curation: [`release_build.mjs`](../../../scripts/release_build.mjs), [`release_verify.mjs`](../../../scripts/release_verify.mjs), [`prepare_release_assets.mjs`](../../../scripts/prepare_release_assets.mjs)
- Native lifecycle: [`run_desktop_e2e.mjs`](../../../scripts/run_desktop_e2e.mjs), [`run_installer_e2e.mjs`](../../../scripts/run_installer_e2e.mjs), [`windows_installer_registry.mjs`](../../../scripts/windows_installer_registry.mjs)
- Paths and credentials: [`paths.rs`](../../../src-tauri/src/paths.rs), [`credentials.rs`](../../../src-tauri/src/credentials.rs), [`Cargo.toml`](../../../src-tauri/Cargo.toml)
- Codex bridge: [`codex.rs`](../../../src-tauri/src/codex.rs), [`commands.rs`](../../../src-tauri/src/commands.rs), [`test_live_codex.mjs`](../../../scripts/test_live_codex.mjs)
- Launcher artifacts: [`descriptors.rs`](../../../src-tauri/src/descriptors.rs), [`transaction.rs`](../../../src-tauri/src/transaction.rs)
- Existing local evidence: [`artifacts/public-readiness`](../../../artifacts/public-readiness), [`dist/release`](../../../dist/release)

No source, workflow, test, schema, packaging, or configuration file was
modified by this audit.
