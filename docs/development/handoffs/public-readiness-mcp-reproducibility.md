# MCP runtime reproducibility gate

Parent verification date: 2026-10-03.

The workflow source remains at exact commit
`f13f462708618660700bb6881a3cfdf8927e932c`. Its canonical manifest is Git blob
`4429eebd1a98b822f1b88a6a618a1529d680fbb5`, 403,242 bytes, SHA-256
`e179d81fd6d9fa38b3a24b739ec6dcc103fe876d6825565a402918990c06b2a0`.
The bundled manifest now contains those exact LF bytes. The source audit
verified all 1,390 declared Git blobs, 45 components, and 34 required MCP
routes at this revision.

`pnpm test:mcp-live` freshly installed `hoi4-agent-tools@3.6.0` in an isolated
prefix on 2026-10-03 and failed its complete package-tree check. The registry
top-level integrity matched the manifest. The failure occurred before the MCP
server was started. The temporary installation was removed by the test's
bounded cleanup. Earlier successful live probes do not establish current
clean-install reproducibility.

The published npm metadata has exact direct dependency versions but no bundled
dependencies. The source release has a `package-lock.json`; the published
bootstrap performs a global npm install and does not consume an immutable
transitive dependency closure. A changed transitive resolution can therefore
make this route fail closed for a new user.

## Final source-auditor follow-up

The source auditor confirmed that main and v3.6.0 have no published shrinkwrap
or bundled-runtime correction. The reviewed package's source `overrides` do
not constrain a root consumer's dependency install, and its ordinary lock is
not enforced for registry consumers. A current dry resolution showed these
differences:

| Dependency | Reviewed source | Current consumer resolution |
| --- | --- | --- |
| `@hono/node-server` | 2.1.0 | 1.19.17 |
| `hono` | 4.13.8 | 4.13.12 |
| `ip-address` | 10.4.0 | 10.7.3 |
| `fast-uri` | 3.1.7 | 3.1.8 |

Primary evidence: [registry artifact](https://registry.npmjs.org/hoi4-agent-tools/3.6.0),
[v3.6.0 package](https://raw.githubusercontent.com/klimPaskov/hoi4-agent-tools/v3.6.0/package.json),
[v3.6.0 lock](https://raw.githubusercontent.com/klimPaskov/hoi4-agent-tools/v3.6.0/package-lock.json).

Publish a new immutable version, such as 3.6.1, with a reviewed complete bundled
production runtime or an enforced `npm-shrinkwrap.json`. Verify that the packed
tarball actually contains the closure evidence, then require repeated isolated
Windows installs to produce identical tree bytes/count and all 34 working
routes. Pin and verify architecture, Node/npm, and Sharp's native optional
packages. Update the workflow bootstrap and regenerate its manifest only after
the corrected artifact exists. The existing 3.6.0 version cannot be repaired
in place. No app checksum/count relaxation is warranted.

Keep the public release gate open until the MCP source publishes a bundled
runtime or an integrity-complete pinned closure, the workflow source publishes
matching source-owned manifest evidence, and repeated clean installation
reproduces that declared tree. Do not replace the expected checksum with a new
mutable tree hash or weaken tree verification.

Rust MCP health now requires the running server to advertise the reviewed
package version, in addition to protocol, capability, complete package/tree,
entry, publisher, and required-route checks. Its negative version test passes.
The Codex live probe now invokes the same `app-server --stdio` arguments as the
production bridge; the live authenticated-account and browser/device-code
start/cancel probe passes. Managed login completion remains a separate gate.

## Enforced closure prepared locally (2026-10-03, not published)

Branch `fix/enforced-production-closure` in the local worktree `hoi4-agent-tools-shrinkwrap`, on top of `origin/main` `fba03d6` (the unreleased 3.7.0, which adds a 35th route, `hoi4.script_validate`), replaces `package-lock.json` with a published `npm-shrinkwrap.json`. The change adds it to `files` and the required packed files, switches the Dockerfile, and adds a metadata test that every exact direct dependency and override pin matches the shrinkwrap.

Findings that shape the release gate:

- npm honors a dependency's shrinkwrap only when the registry manifest carries `_hasShrinkwrap: true`, which the npm registry derives from the uploaded tarball (for example `@salesforce/cli`). Installing a local tarball ignores the shrinkwrap, so pre-publication evidence used a localhost registry shim that serves the packed tarball with that flag and redirects everything else to npmjs.
- With the flag, two isolated `npm install --global --prefix` runs with separate caches on Windows produced the same tree, 5,294 files with tree digest `9ddcc3af…3256`. They installed `hono` 4.13.8 and `@hono/node-server` 2.1.0 as reviewed, with no dev tools, Sharp loaded, and the stdio server reporting 3.7.0 with 35 tools. Without the flag the same tarball resolved `hono` 4.13.12 and `@hono/node-server` 1.19.17.
- npm inflates a dependency's shrinkwrap after its platform check, so all 27 optional `@img/sharp-*` packages are installed on every platform: about 298 MB, compared with 85 MB for a platform-resolved install. Sharp has no install script, so the tree is byte-identical across platforms. That allows one `package_tree_sha256` for Windows and macOS, which a platform-resolved tree cannot provide. This trade-off was accepted as the only closure npm enforces natively.

Remaining steps, each needing approval: push the branch and merge, publish 3.7.0 with provenance, confirm `_hasShrinkwrap: true` on the registry, repeat the isolated installs from the registry (including on macOS), then update the workflow bootstrap to 3.7.0 and 35 routes, regenerate its manifest, and refresh the app's bundled manifest and required-route list.

## 3.7.0 published; 3.8.0 owned by the HOI4 Agent Tools release session (2026-10-04)

klimPaskov/hoi4-agent-tools#21 merged and `v3.7.0` was released from `84eb653`. All seven release jobs passed, and the npm registry reports `_hasShrinkwrap: true` with provenance (`dist.integrity` `sha512-VZYgDCbkgwdSvcVO1hsBgeg8g7x/B/guwyg1mO+TPSvxdFnBI0SBZhiu0j1jT3wVjXvXU5o7nksZL2xT6s2vJg==`). A Windows `npm install --global --prefix` from the registry did not emit `node_modules/.package-lock.json`, so the app's optional hidden-lock check stays optional.

The separate HOI4 Agent Tools release session then reported two 3.7.0 defects and took ownership of MCP releases. First, the shrinkwrap carries the `@hono/node-server` 2.1.0 override, which is outside `@modelcontextprotocol/node`'s `^1.19.9` range, so `npm ls` fails in consumer installs. Second, GUI inspect and render refuse windows with more than 512 unresolved asset names. 3.7.0 is therefore not pinned anywhere. That session releases 3.8.0 with the fixes, still 35 tools, and updates the Agentic-HOI4-Modding bootstrap and manifest itself, including the bootstrap's own 256 MiB tree bound. The app then refreshes its bundled manifest and source-audit inventory from that exact blob and reruns `pnpm test:mcp-live`. The app's 512 MiB total-tree bound for the shrinkwrapped tree is already on the candidate.

## 3.8.1 bundled (2026-10-05)

The HOI4 Agent Tools release session published 3.8.1, with all seven release jobs passing. Agentic-HOI4-Modding pins it at `8c13135`, and the bot's manifest refresh is `824082e`, blob `c58254f8658a64db6933c41be3ba97e1d66e9e3d`. The app bundles those exact LF bytes (SHA-256 `9cca68ea47dfa77bcb41a9d525177213a75a98fb4be549690a15ec1ce74cab07`), and the source-audit inventory names the same revision and digest. The manifest's MCP health rule declares 3.8.1, integrity `sha512-ZwALO4VX…lfgQ==`, which the npm registry independently confirms along with `_hasShrinkwrap: true`. It also declares tree `25702844…7497` with 5,353 files and the unchanged runtime entry `08c66fbe…`, 2,194 bytes, and its 35 required tools include `hoi4.script_validate`. `pnpm test:mcp-live` installed 3.8.1 from the registry into an isolated prefix and passed the app's complete verification: integrity, full tree identity and file count, runtime entry, protocol, advertised package version, and all 35 routes. That gate is closed for Windows. The script now waits for the server to exit before removing its temporary tree, because the removal had failed with `EBUSY` while the server still held its working directory.
