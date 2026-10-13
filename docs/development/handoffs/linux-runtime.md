# Linux runtime handoff

## Scope and result

Implemented native Linux storage/path discovery, Secret Service credentials,
executable provenance, supervised process environment, HOI4 Agent Tools health,
all five coding-client registrations, and manual terminal-folder handoff.
Windows/macOS signing boundaries remain active. No project scan or path
resolution creates a directory, and no secret enters a project, plan, lock,
source manifest, diagnostic preview, or Git artifact.

The parent owns platform/schema integration, native Clausewitz selection,
readiness/UI, release packaging, source-pin adoption, and final full-suite gates.

## Files and ownership

- `src-tauri/src/paths.rs`: XDG settings/data/cache, native and both Flatpak
  HOI4 user directories, bounded Steam libraries, safe Downloads parsing.
- `src-tauri/src/credentials.rs`, Cargo metadata: Linux Secret Service with
  `crypto-rust`, stable opaque scopes, and an isolated native integration test.
- `src-tauri/src/linux_trust.rs`, `process.rs`: protected Debian/Ubuntu package
  receipts, publisher HTTPS client identities, canonical executable discovery,
  fixed system PATH, desktop/session plumbing, and fail-closed validation.
- `src-tauri/src/codex.rs`, `claude_code.rs`: official standalone/current npm
  Linux candidates, publisher-bound hashes through direct account-bearing
  execution, and unchanged token/provider isolation.
- `src-tauri/src/mcp.rs`: platform-specific package evidence, exact declared
  internal links, bounded private-copy verification, metadata-only health, and
  the pure `client_registration` parser.
- `src-tauri/src/commands.rs`: Linux bootstrap execution, immutable platform
  action evidence, all-client native retargeting and installed-registration
  checks, and terminal-folder handoff without secondary client execution.
- `ExternalAction.verified_package_symlinks`: non-secret reviewed evidence;
  constructor defaults are included. Other model/platform changes belong to
  the parent.
- `scripts/test_linux_credentials.sh`: repeatable isolated D-Bus/keyring smoke.
- Security/platform design docs and the owning security skill are updated.

## Security and transaction evidence

The threat boundary includes hostile PATH entries, modified package files,
undeclared or escaping package links, malformed publisher metadata, untrusted
client configs, and missing desktop credential services. Native system tools
require root ownership, protected ancestors, executable permissions, and exact
protected dpkg receipts. MD5 is only the OS database's file-change receipt;
spawn identity uses SHA-256. Other distro package databases are unavailable.

Account clients use fixed publisher HTTPS origins and content digests. Codex's
published Sigstore payload is used as publisher-origin checksum evidence; the
app does not claim independent attestation verification. No untrusted client
runs to reveal its version. Publisher-verified digests are retained through
spawn rather than replaced by independently observed bytes. Redirect origins,
response sizes, digest/size mismatches, malformed payloads, and candidate
enumeration have deterministic regression coverage.

Native npm locator links are never executed by the app or installed clients.
Clients use trusted absolute Node plus the absolute source entry. Only exact
manifest-declared internal links enter the package identity and private runtime
copy; missing, changed, undeclared, escaping, or linked targets fail closed.
All selected primary/additional clients must parse and match the locked launch
before installed MCP health proceeds. Unrelated routes survive adaptation and
normal conflict review.

Linux subprocess PATH is `/usr/bin:/bin`; credential and Node-loader influence
variables do not survive. Browser and JSONL helpers preserve reviewed desktop
session names. Linux terminal handoff returns `opened=false` with an explicit
manual-start instruction. It does not claim a terminal later verifies an
account client's user-writable executable.

No transaction filesystem mutation/rollback algorithm changed. Plans and
journals carry the new non-secret link evidence through the existing review,
backup, post-install checkpoint, and rollback contract. External runtime
installation retains its source-declared rollback boundary. Windows-only 3D
and HOI4 Live Tools behavior remains unchanged.

## Verification performed

Native checks ran on Ubuntu 24.04 in WSL, Rust 1.88.0, root-owned Node 22.23.3,
and npm 10.9.9. They establish native process/library behavior, not a graphical
clean-machine acceptance run.

Passed:

- Windows `cargo check --workspace --all-features --tests`, without warnings.
- Linux all-features test compilation.
- Path suite: 8 tests, including native/Flatpak ambiguity, XDG traversal,
  literal Downloads expansion, and no creation before apply.
- Linux trust suite: 8 ordinary tests, plus the explicit native distro-tool
  receipt smoke for real system Python and Node.
- Standalone/nested/hoisted native Codex candidate regression.
- Exact internal-link private-copy and drift rejection regression.
- Five-client native retargeting and strict registration-parser regressions.
- Linux supervisor and JSONL environment regressions.
- The corrected whitespace-independent provider-error routing regression.
- Real native package 4.0.3 private-copy health against the frozen source
  candidate: 5,253 entries and exactly two declared links; initialize and
  tools/list only. No MCP tool, game, provider, or paid operation was called.
- Real isolated Secret Service store/read/delete with a synthetic value. The
  source Python `read_credential` read the same Rust-created opaque reference;
  after delete, the reference was unavailable. The checked-in runner was also
  executed successfully with that source adapter.

The first vault-smoke setup attempted a daemon before Secret Service finished
initialization and failed. The corrected runner uses an explicit new control
directory, a foreground daemon, and initialization in its own D-Bus session.
It never accesses or replaces the user's desktop daemon or vault.

Repeat the credential integration with an explicit source runtime path:

```bash
HOI4_TEST_IMAGE_VAULT_ADAPTER=/absolute/source/.tools/image_generation/runtime/server.py \
  bash scripts/test_linux_credentials.sh
```

The ignored native MCP health test uses `HOI4_TEST_LINUX_MCP_MANIFEST` and the
source-installed isolated XDG npm prefix; it never invokes a tool.

## Remaining parent gates and limits

The parent must rerun final full Windows/Linux suites on the combined exact
revision, native packaging/install/updater gates, and synthetic lifecycle
scratch tests. Native desktop browser/device-code completion, real launcher
and game loading, and Clausewitz gameplay/ptrace permissions were not exercised
here. No game-readiness claim is made. Other Linux distro security routes and
user-installed arbitrary Python/Node binaries are unsupported until audited.

Skill ownership check: repeated path, credential, executable, process,
MCP-link, native-client, and validation workflows changed. The security skill
and platform/security docs were updated; the parent maintains adjacent global
product/testing/release consistency and the Codex audit handoff.

## Follow-up: shadowed Windows npm prefix

The combined real Windows scratch gate found a genuine resolution defect:
the reviewed bootstrap installed current-user package 4.0.3, while health chose
an inherited PATH locator for older global 3.9.0. The fix binds Windows wrapper
lookup, loaded-native checks, and coding-client registration to the exact
`APPDATA/npm` prefix. Client configs use its absolute source-declared wrapper;
the app's private-copy probe continues to use verified Node and the pinned
JavaScript entry. A missing reviewed-prefix wrapper never falls back to PATH.

This preserves authorized post-plan Node installation and missing-tool states.
Historical Windows fixtures without immutable MCP identity remain unchanged,
and foreign-platform memory fixtures retain their source-declared command.
The parent owns structural readiness compatibility for legacy bare commands;
full installed health requires the exact current-user launch for all clients.
No real-user npm prefix, mod, or account was changed by the regression. The
isolated subprocess test supplies fresh synthetic current/stale prefixes so
process-wide test environment and real desktop state remain untouched.

Follow-up validation passed on Windows: the isolated shadowed-prefix/no-fallback
regression, all five absolute client-registration formats, the existing image
provider adaptation regression, all-features/test compilation, and strict
all-targets/all-features Clippy. The parent must rerun the real combined scratch
gate to establish end-to-end post-install success with the fixed binding.
