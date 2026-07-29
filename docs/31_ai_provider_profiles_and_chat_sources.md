# AI provider profiles and flattened Chat sources

This document records the provider-neutral planning boundary and the optional
Codex-only flattened source export. It extends the planning package; it does
not replace the source manifest, schemas, security model, or transaction
contract.

## Provider selection

The first setup screen selects a planning provider and model. Codex is the
default. The selected profile is carried through the prompt, adapted
`AGENTS.md`, generated `README.md`, project state, installation plan, lock,
readiness report, and maintenance reanalysis.

The bounded registry currently contains:

| Profile | Transport | Credential route | Endpoint rule |
| --- | --- | --- | --- |
| Codex | official local Codex App Server | ChatGPT browser or device-code login owned by Codex | App Server owns its route |
| Claude | Anthropic messages | user-supplied API key in the OS vault | user-supplied HTTPS endpoint |
| Kimi | OpenAI-compatible | user-supplied API key in the OS vault | user-supplied HTTPS endpoint |
| GLM | OpenAI-compatible | user-supplied API key in the OS vault | user-supplied HTTPS endpoint |
| DeepSeek | OpenAI-compatible | user-supplied API key in the OS vault | user-supplied HTTPS endpoint |
| Local model | OpenAI-compatible | no hosted credential | user-supplied loopback HTTP endpoint |
| Other provider | OpenAI-compatible | user-supplied API key in the OS vault | user-supplied HTTPS endpoint |

The application does not invent provider URLs, OAuth routes, package names,
commands, model names, MCP servers, or platform support. A hosted provider is
shown as configured only after its endpoint and credential reference pass
local validation; the first semantic request is the capability check. Local
models are explicitly configuration-based and are not described as hosted
accounts.

All providers use the same `codex-analysis` response schema and approved-input
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

## Codex-only flattened Chat export

The Git phase shows the optional checkbox only when the selected provider is
Codex:

> Prepare a flattened ChatGPT project-sources folder

It is the last optional setup operation. When selected, the core stages a
`<mod_project>/chatgpt_project_sources/` folder containing:

- the adapted project `AGENTS.md`;
- the created or existing project `README.md`;
- every selected `.agents/skills/<skill>/SKILL.md` as `<skill>.md`;
- every selected `.codex/agents/*.toml` subagent file; and
- only the additional project-relative files explicitly entered by the user,
  under `extras/`.

The normal `.agents/skills/` and `.codex/agents/` trees remain intact. The
flattened folder is generated through the same plan, conflict, backup,
staging, validation, apply, readiness, journal, and rollback contract. Rust
rejects traversal, links, secret-shaped paths or content, case-insensitive
destination collisions, and bounded file or aggregate-size violations. A
modified existing flattened file is never replaced silently.

The final screen recommends:

> After setup, start planning using ChatGPT "Chat".

This is only a recommendation. Version 1 does not upload the folder, open a
ChatGPT conversation, or start planning automatically.

## Migration and readiness

Older state defaults to the Codex profile and its existing App Server behavior.
State written by an earlier provider-neutral build is migrated to the matching
`provider_api` or `local_endpoint` mode without turning a disconnected provider
into a successful one.
New plans and locks persist the selected provider, model, optimization profile,
endpoint when applicable, and flatten preference without persisting a secret.
Readiness uses generic AI checks for non-Codex profiles. `Open in Codex` and
flattened Chat export remain Codex-only; other providers receive an honest
ready or incomplete state without a fake Codex opener.
