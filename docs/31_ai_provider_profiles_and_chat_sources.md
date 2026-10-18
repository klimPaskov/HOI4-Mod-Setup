# AI provider profiles and flattened Chat sources

This document records the provider-neutral planning boundary and the optional
flattened ChatGPT source export. It extends the planning package; it does
not replace the source manifest, schemas, security model, or transaction
contract.

## Provider selection

The first setup screen selects a setup assistant and model. Claude, signed in
through the user's own Claude Code, is the default, with Claude Haiku 5.5
(`claude-haiku-5-5`) at medium effort as its default model. The checked-in
catalog contains 14 verified Claude Code choices, including legacy Haiku 4.5;
the Advanced field also accepts a bounded exact Claude model ID without an app
release. The choice is used for the read-only semantic turn and is retained as
analysis provenance in app-managed project state, the installation plan, lock,
readiness report, and maintenance reanalysis. It is not a development-client
selection. Generated `AGENTS.md` and README content, installed components, MCP
tools, Open in Codex, and ChatGPT packaging do not follow or identify it.

The stable bounded registry currently contains:

| Profile | Transport | Credential route | Endpoint rule |
| --- | --- | --- | --- |
| Claude (`claude_account`, default) | the user's installed, unmodified Claude Code in isolated print mode | Claude account sign-in owned by Claude Code (`claude auth login`) | no endpoint; Claude Code owns its route |
| Codex | official local Codex App Server | ChatGPT browser or device-code login owned by Codex | App Server owns its route |
| Claude API key (`claude`) | Anthropic messages | provider API key in the OS vault | verified default; editable under Advanced |
| Gemini API key (`gemini`) | Google's OpenAI-compatible API | provider API key in the OS vault | `https://generativelanguage.googleapis.com/v1beta/openai/chat/completions`; `gemini-3.8-flash` |
| Grok API key (`grok`) | xAI Responses API | provider API key in the OS vault | `https://api.x.ai/v1/responses`; `grok-4.7` |
| Kimi | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| GLM | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| DeepSeek | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| Gemini account (`gemini_account`, experimental) | pinned official Gemini CLI | CLI-owned Sign in with Google in an app-private home | no app-owned endpoint; live account validation remains open |
| Grok account (`grok_account`, experimental) | pinned official Grok CLI | CLI-owned `grok login` in an app-private home | no app-owned endpoint; live account validation remains open |
| Local model | OpenAI-compatible | no hosted credential | user-supplied loopback HTTP endpoint |
| Custom provider (`custom`) | OpenAI-compatible | user-supplied API key in the OS vault | user-supplied HTTPS endpoint |

**No setup assistant** (`none`) is not a provider profile. It is the clearly
labelled deterministic-only route. No provider is contacted, and the plan, lock, project
state, and readiness report record model `deterministic`, effort `none`,
optimization profile `Deterministic setup`, no endpoint, and no analysis
record. Values the scan decides fill what they can and the user reviews the
rest. Readiness reports a non-blocking `analysis.deterministic` check instead
of the provider sign-in and analysis checks. A `none` plan or lock carrying an
analysis record is rejected, and an assistant plan without its confirmed
analysis still fails.

The two `*_account` rows are selectable experimental routes in the current
setup screen. They are not a release claim that real accounts or every
platform route has passed validation. They become stable only after the
account-adapter completion gate in
`docs/34_provider_account_cli_architecture.md` passes.

The known hosted profiles ship with checked-in model and HTTPS address defaults
verified against the providers' official documentation. Gemini uses the
official Google OpenAI-compatible endpoint and Grok uses the official xAI
Responses endpoint. After connection, the app reads the provider's model
catalog through its official Models API. Codex uses App Server `model/list`, including each model's
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

Claude Code has no live model-catalog command. Its 14 built-in choices are
labelled as built-in, not live; Haiku 5.5 is the default at medium effort and
Haiku 4.5 remains a one-effort legacy choice. A manually entered Claude model
ID is validated as an exact bounded identifier and is sent to Claude Code
without waiting for a catalog entry. An unknown model does not block an
authenticated Claude route; effort is forwarded only when the catalog knows
that the selected model supports it.
For hosted API profiles, the first screen asks the user to open the provider's
fixed official API-key page, paste the key, and choose **Connect**. Model and
address remain available under **Advanced**. This is a simple provider
connection, not a claim of third-party OAuth support. The
application does not invent provider URLs, login routes, package names,
commands, model names, MCP servers, or platform support. A hosted provider is
shown as connected only after its address and credential reference pass local
validation; the first semantic request is the capability check. Local
models are explicitly configuration-based and are not described as hosted
accounts.

All setup assistants use the same `codex-analysis` response schema and approved-input
boundary. Codex receives the schema through App Server `outputSchema`; the
Anthropic, Gemini, and Grok API adapters use their native structured-output
formats; other adapters receive the exact checked-in schema in their system
request. Native output is an input constraint, not a substitute for runtime
validation: Rust validates the complete checked-in Draft 2020-12 schema and
the semantic invariants before accepting a response. The core assigns its own
UUID analysis ID after validation instead of requiring the model to invent a
valid UUID. It accepts an empty `evidence_refs` array when a suggestion is
derived from the brief alone, while still rejecting unknown references and
retaining the exact scan, hash, and explicit evidence approval binding. A
provider response cannot write files, approve a transaction, resolve a
conflict, or pass readiness by itself.

A completed response that is private/account-shaped, truncated, refused,
has an unreadable response envelope, is interrupted, transport-failed,
unauthenticated, or usage-limited is discarded without a corrective retry and
is mapped to a bounded user-facing message. One corrective turn is reserved
for a completed response that fails deterministic serialization,
identifier/value, or path validation, including a readable envelope whose
text is not valid JSON or carries no analysis object. Provider
output and failure text never enters project state, plans, locks, or logs.

Provider-neutral text filtering rejects setup-provider attribution and internal
protocol terms, not ordinary mod guidance. Words such as `platform`, `Windows`,
or `constraint` are allowed when they describe a real user action or the mod's
subject matter; broad word bans are not a substitute for semantic validation.

Credential references are deterministic, opaque, and scoped to the selected
provider in the operating-system vault, so a restart can reconnect without
putting a key in project state. The reference is never accepted for another
provider. AI provider references never enter project files, plans, locks, logs,
or analysis output. Hosted requests use a bounded client with no redirects, no
endpoint userinfo, HTTPS only, and a bounded response body. Local requests are
limited to loopback HTTP.

For a fresh Codex choice, a successful App Server catalog replaces an absent
built-in fallback with its advertised default and supported default effort.
An explicitly selected model remains selected if unavailable, with a clear
model-availability message. An outdated client advertising a retired model is
reported as an update/model issue rather than a sign-in failure.

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
  Homebrew prefixes. Windows requires Anthropic Authenticode subject
  `Anthropic, PBC`; macOS requires Developer ID `Anthropic PBC (Q6L2SF6YDW)`.
  Linux requires the exact binary SHA-256 and size from Anthropic's official
  version manifest before executing it, then binds that digest again at spawn.
  The app never downloads, bundles,
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
  fields are discarded before any value leaves the adapter. Only a Claude plan
  sign-in (`authMethod = "claude.ai"` with the first-party provider) is the
  Claude account route; a Console or API-key sign-in is shown as such and
  points to the Claude API key option, and a session routed to another
  inference provider is not accepted.
- **Sign out** runs `claude auth logout`, clears pending proposals and the
  approved scan evidence like Codex sign-out, and advances a session
  generation so an analysis that was running is discarded. Switching provider
  cancels a pending Claude sign-in, and each sign-in attempt is waited on
  once.
- Analysis runs one print-mode turn:
  `claude --print --output-format json --json-schema <codex-analysis schema>
  --model <model> --tools "" --strict-mcp-config --safe-mode
  --no-session-persistence --system-prompt <bounded instructions>`. The prompt
  travels on standard input. The working directory is a stable, app-owned
  empty directory (`claude-analysis` under the app data root) that must stay
  empty, so no project path or directory-scoped configuration reaches the
  session and runs do not add new project entries to Claude Code's state. The
  reply is parsed and validated before any redaction, so credential-shaped
  content is rejected instead of masked. No tools, MCP servers, user or project customizations, or session
  history are available. Claude Code's schema validator rejects the draft
  2020-12 `$schema` declaration, so the copy passed to `--json-schema` omits
  `$schema` and `$id`; the schema-shaped `structured_output` is then validated
  against the complete Draft 2020-12 schema by the same deterministic
  validator as every other provider.
- The child environment is cleared. Besides the standard safe process
  variables, only `USER`, `LOGNAME`, `ProgramData`, the user's
  `CLAUDE_CONFIG_DIR`, proxy settings, and `NODE_EXTRA_CA_CERTS` pass through,
  and `DISABLE_AUTOUPDATER=1` plus `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`
  keep the reviewed binary from updating itself mid-session. `ANTHROPIC_*` keys and tokens, Claude
  Code host-session variables, and alternate-provider selectors are never
  forwarded, so the user's own Claude Code sign-in is what authenticates.
- Claude Code exposes no remaining-usage reading, so the panel states that
  analysis uses the user's Claude plan. A usage-limit result is mapped to the
  same visible usage-limited category used for Codex, and the draft is kept.
- Claude Code has no model-catalog command. The checked-in choices are labelled
  as built-in rather than live; Haiku 5.5 is the default at medium effort,
  while Haiku 4.5 remains a legacy one-effort choice. A bounded exact model ID
  may be entered under Advanced. Unknown IDs remain usable after identifier
  validation, but the adapter forwards an effort only when its catalog knows
  that the selected model supports one.

Plans, locks, readiness reports, and project state record the route as
provider `claude_account`, integration `claude_code_cli`, and auth mode
`claude_account`. The analysis record uses engine `claude_code_cli`. None of
them contains an account identity, and generated project guidance stays
provider-neutral. The Anthropic API-key route remains available as **Claude API
key** for users who prefer Console billing; it also defaults to Haiku 5.5.
Existing records keep their stored provider: a legacy record without a provider
field still means Codex.

## Experimental Gemini and Grok account adapters

`gemini_account` and `grok_account` are separate from the hosted API profiles.
They are an experimental implementation surface for live testing, not a claim
that the app has completed account support or that real accounts have been
tested. The app does not ask the user to paste a credential. After an explicit
**Sign in** action, it fetches only the pinned official CLI package named in
`src-tauri/resources/account-cli-pins.json`, verifies the package integrity and
entry digest, and starts the provider's own interactive sign-in flow.

- Gemini follows the official CLI's **Sign in with Google** flow. The CLI owns
  OAuth, token storage, refresh, and logout. The adapter uses the official CLI
  configuration to disable extensions, MCP, skills, hooks, telemetry, and
  write-capable tools for setup analysis.
- Grok follows the official CLI's `grok login` and `grok logout` commands. The
  current worker uses an account-only policy and a single read-only analysis
  turn; the hosted API profile remains the separate xAI Responses route.
- Both adapters run in app-private provider homes and an empty workspace. Grok
  receives approved input through a private ephemeral `--prompt-file`, never
  argv; the file is cleaned up on success, error, and cancellation. The app
  never reads, copies, logs, or persists CLI tokens or raw model output.
  Provider-owned session persistence is permitted only for approved setup
  inputs in the isolated data home; session files are never copied into the
  project, lock, plan, logs, or source package. No target-project path is
  exposed to the CLI.
- Current worker support is Windows/Linux terminal integration. macOS remains
  unavailable in this build until a verified runtime and terminal route exists,
  even where a platform package pin is present. The account CLI uses the
  official Grok Build fallback `grok-build`; this is distinct from the accepted
  hosted API default `grok-4.7`.

The provider record uses engine `provider_cli` and auth mode
`provider_account`; it contains only the provider ID, selected model, and
non-secret analysis provenance. Capability checks and sign-in failures preserve
the local draft and do not start a transaction. See
`docs/34_provider_account_cli_architecture.md` for the accepted boundary,
official references, and unresolved live-test gate.

## Flattened ChatGPT export

Superseded (2026-10-18): the app no longer packages ChatGPT sources or prepares a flattened folder. The packager is the optional `runtime.chatgpt_sources` manifest component, a `.tools` script the user checks on Components and runs. See `docs/33_chatgpt_source_packaging.md`.

## Existing-project ChatGPT source package

The initial-install flatten option is independent of the setup assistant.
Separately, when an
existing-project scan finds an `AGENTS.md`, flattened skill, or subagent,
**Manage an existing project** can package its detected ChatGPT sources
regardless of the current planning-provider screen or installation lock. The
page defaults to the native Downloads folder and shows:

- `AGENTS.md` and `README.md`;
- every direct canonical skill `SKILL.md` as `<skill>.md`, with the legacy
  `.agents/skills/` root supported;
- direct canonical Claude Markdown subagents, or legacy Codex TOMLs; and
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
credential-vault entry. Experimental account-CLI records use
`provider_cli`/`provider_account` and must not be migrated from an API-key
record or treated as authenticated merely because a package pin exists.
