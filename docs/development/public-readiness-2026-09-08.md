# Public-readiness investigation — 2026-09-08

Status: not ready for unrestricted public release. The existing-project bridge
has been repaired, but the reviewed MCP runtime currently has an upstream
reproducibility failure. This report does not certify a complete native install.

## Reproduced failures and changes

- `scanner.rs` emitted absolute launcher paths, `.git/HEAD`, and `.` as semantic
  evidence references. The generic evidence validator correctly rejected them,
  preventing otherwise complete scans from reaching semantic review. Reserved
  summary identifiers now represent these facts and Git warnings. Actual file
  reads, bounds, and link protection remain unchanged. `commands.rs` tests the
  real scan-to-approval route, including forged paths, hashes, roots, scan IDs,
  linked worktrees, and malformed launcher conflicts.
- The observed installation journal stopped at blocking `mcp.hoi4` readiness
  without writing a success lock. `commands.rs` now retains a bounded, redacted
  bootstrap cause; `transaction.rs` carries it through readiness into recovery.
  Failure and rollback tests verify that no success lock or secret is persisted.
- `security.rs` checked serialized JSON punctuation for credentials. An already
  redacted assignment at a string boundary could therefore prevent a journal
  from being saved. Validation now examines decoded keys and string values,
  including nested arrays and embedded JSON strings. Real credential-shaped
  values and forbidden keys remain rejected. Reusable regexes are cached.
- The previous browser smoke script only inspected HTML. Playwright now renders
  15 synthetic wizard states and exercises signed-out management, keyboard
  environment selection, recovery, narrow viewports, and runtime-error checks.
  `scripts/start_browser_fixture.mjs` builds fixtures into ignored artifacts and
  serves them on a dedicated loopback port. Production builds do not enable
  these fixture routes. CI, preview, and release workflows install Chromium.

## Remaining installation blocker

The failing source revision was
`0bb4917dca228886aa9a52963bc09e7078666d64`. The reviewed MCP package version
was `hoi4-agent-tools@3.0.7`. Its top-level npm tarball matched the declared
integrity, and its 199 regular files matched the installed copies. However,
the manifest verifies the complete runtime tree, including dependencies:

| Evidence | SHA-256 |
| --- | --- |
| Reviewed tree, 4,848 files | `91893b13d6650e54704325a8ddf978127015afafd0e495e85d9bbcc585e5ebdf` |
| Observed tree, 4,848 files | `db70d6d4377ab2c0eeea97a2271212e6781cb7c3f67322ba08b709f9b76992d1` |

The tarball has no published shrinkwrap or complete bundled dependency closure.
Its transitive ranges resolved newer packages, including `fast-uri@3.1.7` and
`hono@4.13.7`, after the expected digest was produced. Pinning only the top-level
version cannot reproduce that digest. The app must continue to reject this tree.

The upstream package needs an immutable, integrity-complete dependency closure
or verified bundled runtime, followed by fresh generated manifest evidence and
a published exact source revision. Merely replacing the expected digest with
the current install would leave the same failure possible tomorrow. The live
workflow checkout contained substantial pre-existing edits; this investigation
did not modify or publish that checkout, npm packages, or source releases.

## Verification scope

The final parent-run Chaos Redux fixture completed a read-only scan in
1.4 seconds: 141 relevant files, 37 directories, and 2,445,552 detector bytes,
without partial results, limits, or app-owned metadata changes. Its Git
configuration is intentionally reported as an advisory unsupported inspection
state. No Kaiserredux checkout was tested. Large synthetic irrelevant-file and
budget/cancellation fixtures are included in the Rust suite.

Final validation results are recorded below after completion. Local logs and
screenshots remain in ignored `artifacts/public-readiness` and `test-results`;
private project data and transaction journals are not committed.

## Contract, documentation, and review

- No schemas, migrations, provider defaults, authentication protocol, credential
  storage, or platform routes changed. The explicitly approved launcher value
  still includes its actual path in model-visible input; the summary identifier
  is not a filesystem permission or a claim that this value is UI-only.
- Transaction ordering, backups, staging, apply gates, success-lock rules, and
  rollback semantics remain intact. Only failure diagnostics and JSON validation
  changed. Optional/core readiness requirements were not weakened.
- Windows Rust tests and isolated browser fixtures are the available evidence.
  macOS native flows, fresh-machine installers, live browser/device login, and
  a successful real MCP installation still require separate verification.
- Updated CHANGELOG, DEVELOPMENT, scanner/MCP/security/transaction/testing
  design documents, and the scanner, Codex integration, source manifest,
  security, transactions, testing, and release skills. The skill ownership
  review found changed validation, recovery, and browser commands covered by
  these owners; adjacent boundaries remain consistent. No new skill was needed.
- Reviewed the scanner audit handoff and Codex integration auditor response.
  The latter found no High/Critical boundary defects; its additional route-test
  requests were implemented. Separate MCP and transaction auditor requests did
  not start, so those changes received parent review and regression tests only.

## Final validation results

- Headless browser: 18 passed against the dedicated compiled synthetic fixture.
  Inspected the generated dry-run screenshot; runtime, narrow-width, keyboard,
  and recovery assertions passed. Initial Vite development-server cold-load
  timeouts led to the separate precompiled fixture server described above.
- Frontend unit tests: 129 passed; lint and TypeScript checking passed.
- Repository validation: 10 integrity groups passed. Workflow authority,
  accessibility contract, and 15 release-asset/registry tests passed.
- Real Chaos Redux read-only benchmark: passed, 1.4 seconds, no limits or
  app-owned metadata changes. This is not a full gameplay/Git inventory.
- Final Rust suite: 406 passed, one opt-in fixture ignored by the default suite
  and run separately above. Command: `cargo test --manifest-path
  src-tauri/Cargo.toml --all-features -- --test-threads=4`. This includes auth
  protocol fixtures, source integrity checks, scan budgets/cancellation, and
  transaction stage/operation/process-interruption and rollback faults.
- A cancellation deadline failed during an earlier heavily concurrent run;
  it passed in isolation and in the final four-thread suite with its original
  deadline unchanged. The long-diagnostic redaction boundary failure found
  during testing was fixed and its regression passed in the final suite.
- Secret-pattern check and Clippy (`--all-features --all-targets -- -D warnings`)
  passed. Rust formatting and `git diff --check` passed.
- Final Windows debug build passed with `pnpm tauri build --debug --no-bundle`.
  The rebuilt executable is `target/debug/hoi4-mod-setup.exe`. The installed
  application was not replaced. This artifact is not a signed release package
  or evidence of a successful native end-to-end installation.
