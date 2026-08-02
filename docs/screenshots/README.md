# Wizard screenshots

These full-window captures are used beside the matching steps in the
user-facing [README](../../README.md). They show the current Project, Review,
Components, Integrations, Git, Install, Ready, and maintenance screens at a
1280 by 960 desktop viewport.

No credential, account identity, private mod, or secret value appears in a
public capture. The optional portrait workflow is linked only after a
successful setup and is not an installation option.

The captures are implementation evidence, not the design references in
[`ui-references/`](../ui-references/). They use the sanitized Atlantis Rising
documentation fixture in `src/documentation-fixtures.ts`; the fixture is
available only in a development build and cannot activate in a packaged app.

With `pnpm dev` running, open `http://localhost:1420/?screenshot=<name>` where
`<name>` is one of `welcome`, `provider`, `existing`, `description`, `identity`,
`components`, `workflows`, `mcp`, `git`, `dry-run`, `ready`, or `maintenance`.
Capture the complete 1280 by 960 application viewport from the top of the page.
