# Wizard screenshots

These full-window captures are used beside the matching steps in the
user-facing [README](../../README.md). They show the current Project, Review,
Components, Integrations, Git, Install, Ready, maintenance, and ChatGPT source
package screens at a 1280 by 960 desktop viewport.

No credential, account identity, private mod, or secret value appears in a
public capture. The optional portrait workflow is provider-selectable for generic
projects. Chaos Redux uses RunPod API-first; computer control is opt-in and is
not shown as a default setup route. Non-sourced portraits use native ImageGen.
The portrait workflow can be selected during setup and is linked from Ready
after a successful setup.

The captures are implementation evidence, not the design references in
[`ui-references/`](../ui-references/). They use the sanitized Atlantis Rising
documentation fixture in `src/documentation-fixtures.ts`; the fixture is
available only in a development build and cannot activate in a packaged app.
The Integrations and Ready captures show the generic RunPod route with setup
still required; Cloud and Local remain available in the generic app but are
not Chaos Redux routes.

With `pnpm dev` running, open `http://localhost:1420/?screenshot=<name>` where
`<name>` is one of `welcome`, `provider`, `existing`, `description`, `identity`,
`components`, `workflows`, `mcp`, `git`, `dry-run`, `ready`, `maintenance`, or
`chat-sources`, or `recovery`.
Capture the complete 1280 by 960 application viewport from the top of the page.
