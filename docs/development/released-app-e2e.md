# Remote released-application acceptance

`Released app E2E` runs only on disposable GitHub-hosted Windows runners. It
does not launch anything on a contributor's desktop. The repository command is
`pnpm test:released-e2e <new|kaiserreich|recovery>`; the runtime guard rejects
local and self-hosted execution. `pnpm test:released-harness` checks the harness
boundaries separately in CI.

## Evidence boundaries

- Install the exact public NSIS artifact pinned in
  `tests/remote/released-app-pins.json`, checking its SHA-256, size and publisher.
  Do not rebuild or instrument the binary and call it the published release.
- Exercise the production WebView2 UI using Playwright's documented
  [CDP connection](https://playwright.dev/docs/webview2). Debugging is enabled
  through child environment variables and an executable-specific HKLM policy on
  the disposable runner. WebView2 150+ ignores environment overrides in elevated
  hosts, as explained by [Microsoft](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5645).
  Refuse pre-existing policy values and remove only exact test-owned values at
  shutdown. No wildcard policy, user-machine change or application backdoor.
  Empty policy container keys may remain on the disposable runner; no configured
  value remains. Policy cleanup runs even if process/browser shutdown fails.
- Use real renderer-to-Rust IPC, source downloads, scanner, schema validation,
  confirmation, conflict review, transactions, MCP installers and readiness.
  Do not mock Tauri commands, seed approval state or write project state from
  the browser driver.
- Planning uses a deterministic loopback HTTP responder through the supported
  Local model route. This tests transport, output rejection/correction and
  approval binding, not model quality, hosted authentication or provider quotas.
  No personal provider account, token or paid generation is used.
- Keep app-data, home, temporary files and WebView2 data in an owned profile.
  Capture the CI token only for fixed public GitHub metadata requests, remove
  credential-shaped environment names before spawning children, and use an
  explicit environment allowlist for the application.
- Download Kaiserreich from the official `Kaiserreich/Kaiserreich-HOI4`
  repository at immutable pinned release commits. Validate the complete archive
  inventory before extraction, reject links/traversal/collisions/special files,
  bound entry count, compressed/expanded size, compression ratio, depth and path
  lengths, remove incomplete extraction, and compare every file with the pinned
  Git tree's blob evidence. For the source's text `eol=crlf` extensions, an LF
  normalization may authenticate the original Git blob while keeping the archive
  bytes intact; binary files require exact bytes. Independently pin the LF
  descriptor and README SHA-256 values. These are
  mod inputs, not the Agentic workflow source; the application continues to
  selectively fetch the latter from its public manifest.
- Upload only explicitly named screenshots and a bounded result report. Never
  upload the mod, profiles, credentials, source caches or transaction backups.
  Redact the known metadata token and credential-shaped values from every report
  field before writing it. Run only for same-repository pull requests or manual
  dispatch; fork PRs do not receive this workflow's metadata token.

## Scenarios

1. New project: create from a brief, reject one malformed proposal and accept
   its one permitted correction, confirm suggestions, choose Claude Code plus
   Codex, retain default GPTImage, review and install. Verify both descriptors,
   launcher path, decoded thumbnail, source-bound lock, real MCP setup and Ready.
   Delete one owned skill, repair it through the UI, and verify its original hash.
2. Kaiserreich: read-only scan a complete official 1.7 archive, confirm proposals,
   select fal/Ideogram V3 Transparent without a key, preserve existing conflicts,
   install and reach core readiness without inventing a launcher registration.
   Verify every original file; the sole permitted non-identical original is an
   explicitly preserved-rule `.gitignore` merge. Repair a missing owned skill.
3. Recovery: import the full official 1.6.4 archive, interrupt the owned app after
   an observed live apply checkpoint, and relaunch with a fresh UI profile while
   retaining backend journals. Stop the planner before recovery. Use Manage and
   Undo changes without provider authentication, and require exact restoration
   of the complete original inventory with no success lock.

The scenarios do not run Hearts of Iron IV or validate gameplay. Browser
screenshots, successful creation and core readiness do not prove a real
launcher/game session. Findings from an immutable published version must be
reported separately from any later candidate fix; never move its release tag.

## Maintenance

The workflow also compares the public HOI4 Agent Tools runtime under Node 22
and 24 and independently pinned npm 10/11 on Windows and Linux. The published
UI lane uses Node 24; the separate matrix retains the older-runtime regression.
`HOI4_MCP_DIAGNOSTICS=1` is restricted to hosted CI and retains only
public-package paths, sizes, hashes and runtime versions when checking a tree
mismatch. It never approves changed hashes or uploads installed package bytes.

The workflow is `.github/workflows/released-app-e2e.yml`. The driver is
`scripts/run_released_e2e.mjs`, with bounded support functions and an archive
validator beside it. NSIS registration preflight and cleanup reuse the existing
repository-owned Windows installer helpers. Keep artifact uploads allowlisted,
permissions read-only, action references pinned, and source/mod identities in
the report. A failed scenario must preserve an honest failure result rather than
silently skipping a screen or replacing the production IPC result.

When invoking native Windows PowerShell from a PowerShell 7 runner, import the
signature module from that child's `$PSHOME`; inherited module search paths can
otherwise select an incompatible PowerShell 7 assembly.
