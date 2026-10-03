# AI provider profiles and flattened Chat sources

This document records the provider-neutral planning boundary and the optional
flattened ChatGPT source export. It extends the planning package; it does
not replace the source manifest, schemas, security model, or transaction
contract.

## Provider selection

The first setup screen selects a setup assistant and model. Claude, signed in
through the user's own Claude Code, is the default, with Claude Haiku 4.5
(`claude-haiku-4-5-20251001`) as its default model. The choice is used for the read-only semantic turn and is retained as
analysis provenance in app-managed project state, the installation plan, lock,
readiness report, and maintenance reanalysis. It is not a development-client
selection. Generated `AGENTS.md` and README content, installed components, MCP
tools, Open in Codex, and ChatGPT packaging do not follow or identify it.

The bounded registry currently contains:

| Profile | Transport | Credential route | Endpoint rule |
| --- | --- | --- | --- |
| Claude (`claude_account`, default) | the user's installed, unmodified Claude Code in isolated print mode | Claude account sign-in owned by Claude Code (`claude auth login`) | no endpoint; Claude Code owns its route |
| Codex | official local Codex App Server | ChatGPT browser or device-code login owned by Codex | App Server owns its route |
| Claude API key (`claude`) | Anthropic messages | provider API key in the OS vault | verified default; editable under Advanced |
| Kimi | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| GLM | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| DeepSeek | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| Local model | OpenAI-compatible | no hosted credential | user-supplied loopback HTTP endpoint |
| Custom provider (`custom`) | OpenAI-compatible | user-supplied API key in the OS vault | user-supplied HTTPS endpoint |

The known hosted profiles ship with checked-in model and HTTPS address defaults
verified against the providers' official documentation. After connection, the
app reads the provider's model catalog through its
official Models API. Codex uses App Server `model/list`, including each model's
advertised reasoning levels. A missing, empty, or temporarily unreachable live
catalog never removes the model control: Codex and the known hosted profiles
retain their checked-in verified default as a selectable option, and a later
live result augments that option. The fallback exposes only the verified default
reasoning effort until live per-model capability metadata is available, and the
screen identifies whether it is using a live result or the built-in choice.
Local and custom profiles retain an editable
model control and use a successfully read endpoint catalog as suggestions
without inventing a model. The model and reasoning controls remain visible on
the first screen; labels run from Light (`low`) through Max (`max`). Codex
defaults to `gpt-5.6-luna` at `xhigh`. DeepSeek defaults to
`deepseek-flash`; its authenticated Models API supplies the supported effort
levels for each returned model. The provider catalog can change independently
of an app release, so a successful live catalog is authoritative for model
availability and effort support.
The first screen asks
the user to open the provider's fixed official API-key page, paste the key, and
choose **Connect**. Model and address remain available under **Advanced**. This
is a simple provider connection, not a claim of third-party OAuth support. The
application does not invent provider URLs, login routes, package names,
commands, model names, MCP servers, or platform support. A hosted provider is
shown as connected only after its address and credential reference pass local
validation; the first semantic request is the capability check. Local
models are explicitly configuration-based and are not described as hosted
accounts.

All setup assistants use the same `codex-analysis` response schema and approved-input
boundary. Codex receives the schema through App Server `outputSchema`; the
other adapters receive the exact checked-in schema in their system request.
The core requires an explicit approval hash for the exact evidence vector after
each completed scan. A provider response cannot write files, approve a
transaction, resolve a conflict, or pass readiness by itself.

Credential references are deterministic, opaque, and scoped to the selected
provider in the operating-system vault, so a restart can reconnect without
putting a key in project state. The reference is never accepted for another
provider. AI provider references never enter project files, plans, locks, logs,
or analysis output. Hosted requests use a bounded client with no redirects, no
endpoint userinfo, HTTPS only, and a bounded response body. Local requests are
limited to loopback HTTP.

## Claude account route

Anthropic's Claude Code legal and compliance terms
(`https://code.claude.com/docs/en/legal-and-compliance`) do not permit a
third-party application to offer Claude.ai login, to route requests through a
Free, Pro, or Max plan on its users' behalf, or to collect, store, or
intermediate Claude credentials or session tokens; sign-in must complete
through Anthropic's own flow. The same terms do not prevent an end user from
signing in to the unmodified Claude Code binary with their own Claude
subscription. The `claude_account` profile is built on that permitted route:

- The app locates the user's own `claude` executable from PATH, the documented
  native-installer launcher directory (`~/.local/bin`), or, on macOS, the
  Homebrew prefixes. The resolved binary must carry Anthropic's signature:
  Authenticode subject `Anthropic, PBC` on Windows, or Developer ID
  `Anthropic PBC (Q6L2SF6YDW)` on macOS. The app never downloads, bundles,
  modifies, or replaces Claude Code; when it is missing the app links to the
  official setup page (`https://code.claude.com/docs/en/setup`).
- Before use, the app checks that `claude --help` advertises every isolation
  flag it relies on. An older build is reported as needing `claude update`
  instead of being run with weaker isolation.
- **Sign in to Claude** runs `claude auth login --claudeai`. Claude Code opens
  the browser and completes Anthropic's sign-in itself. Standard input is
  closed, so the app never receives or forwards a sign-in URL, authorization
  code, or token. If the browser callback cannot complete, the user signs in
  from a terminal with `claude auth login` and chooses **Check again**.
  Sign-in can be cancelled, and it times out after ten minutes.
- Status comes from `claude auth status --json`. Only `loggedIn`, a bounded
  `authMethod` token, and `apiProvider` are read; email, organization, and plan
  fields are discarded before any value leaves the adapter. A session that
  Claude Code routes to another inference provider is not treated as a Claude
  account sign-in.
- **Sign out** runs `claude auth logout` and clears pending proposals and the
  approved scan evidence, like Codex sign-out.
- Analysis runs one print-mode turn:
  `claude --print --output-format json --json-schema <codex-analysis schema>
  --model <model> --tools "" --strict-mcp-config --safe-mode
  --no-session-persistence --system-prompt <bounded instructions>`. The prompt
  travels on standard input. The working directory is a fresh, empty temporary
  directory, so no project path or directory-scoped configuration reaches the
  session. No tools, MCP servers, user or project customizations, or session
  history are available. The schema-shaped `structured_output` is validated by
  the same deterministic validator as every other provider.
- The child environment is cleared. Besides the standard safe process
  variables, only the user's `CLAUDE_CONFIG_DIR`, proxy settings, and
  `NODE_EXTRA_CA_CERTS` pass through. `ANTHROPIC_*` keys and tokens, Claude
  Code host-session variables, and alternate-provider selectors are never
  forwarded, so the user's own Claude Code sign-in is what authenticates.
- Claude Code exposes no remaining-usage reading, so the panel states that
  analysis uses the user's Claude plan. A usage-limit result is mapped to the
  same visible usage-limited category used for Codex, and the draft is kept.
- Claude Code has no model-catalog command. The built-in model choice is Claude
  Haiku 4.5. Haiku 4.5 does not accept an effort level, so no effort is
  forwarded and the effort control is hidden for it.

Plans, locks, readiness reports, and project state record the route as
provider `claude_account`, integration `claude_code_cli`, and auth mode
`claude_account`. The analysis record uses engine `claude_code_cli`. None of
them contains an account identity, and generated project guidance stays
provider-neutral. The Anthropic API-key route remains available as **Claude API
key** for users who prefer Console billing; it also defaults to Haiku 4.5.
Existing records keep their stored provider: a legacy record without a provider
field still means Codex.

## Flattened ChatGPT export

The Components screen shows this optional development-client checkbox
regardless of which setup assistant is selected:

> Prepare a flattened ChatGPT project-sources folder

It appears alongside the other choices under **Choose what to install**. The
row shows the file count and an expandable list of flattened filenames and
sizes. Source-declared sizes can appear before planning; generated-file sizes
and the exact total appear after the plan is prepared. The Install review shows
the selected package as read-only. When selected, the core stages a
`<mod_project>/chatgpt_project_sources/` folder containing:

- the adapted project `AGENTS.md`;
- the created or existing project `README.md`;
- every selected `.agents/skills/<skill>/SKILL.md` as `<skill>.md`; and
- every selected `.codex/agents/*.toml` subagent file.

The normal `.agents/skills/` and `.codex/agents/` trees remain intact. The
offline wiki, wiki media, descriptors, configuration, and workflow assets are
not copied into this folder and do not count against its flattening limits. The
flattened folder is generated through the same plan, conflict, backup,
staging, validation, apply, readiness, journal, and rollback contract. Rust
rejects links, secret-shaped content, case-insensitive
destination collisions, and bounded file or aggregate-size violations. A
modified existing flattened file is never replaced silently.
If conflict review keeps a selected local skill or subagent, the flattened view
uses those reviewed local bytes. Unrelated project skills and subagents are not
enumerated or copied.

The final screen recommends:

> After setup, start planning using ChatGPT "Chat".

This is only a recommendation. Version 1 does not upload the folder, open a
ChatGPT conversation, or start planning automatically.

## Existing-project ChatGPT source package

The initial-install flatten option is independent of the setup assistant.
Separately, when an
existing-project scan finds an `AGENTS.md`, flattened skill, or subagent,
**Manage an existing project** can package its detected ChatGPT sources
regardless of the current planning-provider screen or installation lock. The
page defaults to the native Downloads folder and shows:

- `AGENTS.md` and `README.md`;
- every direct `.agents/skills/<skill>/SKILL.md` as `<skill>.md`;
- every direct `.codex/agents/*.toml`; and
- every other immediate root `*.md` file as an unchecked optional entry.

Required entries are selected and disabled. Optional root Markdown can be
selected before packaging. Rust re-discovers and revalidates the files, writes
a new ZIP outside the project, refuses overwrite, and reports the archive path,
included files, byte count, and SHA-256. This export does not upload to ChatGPT,
change provider state, or modify the project.

## Migration and readiness

Older state defaults to the Codex profile and its existing App Server behavior.
State written by an earlier provider-neutral build is migrated to the matching
`provider_api` or `local_endpoint` mode without turning a disconnected provider
into a successful one.
New plans and locks persist the selected setup provider, model, reasoning
effort, optimization profile, endpoint when applicable, and flatten preference
without persisting a secret. The provider fields are audit and maintenance
provenance only. Legacy optimization labels are normalized to setup-analysis
labels during migration. Readiness uses generic setup-analysis checks for
non-Codex profiles. `Open in Codex` follows installed `codex.config` plus
locally recomputed core readiness, and flattened Chat export follows its
independent checkbox; neither is enabled or disabled by the setup assistant.
The opener never rechecks the setup assistant's App Server session or
credential-vault entry.
