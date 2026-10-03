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
