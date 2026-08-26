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
| Claude | Anthropic messages | provider API key in the OS vault | verified default; editable under Advanced |
| Kimi | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| GLM | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| DeepSeek | OpenAI-compatible | provider API key in the OS vault | verified default; editable under Advanced |
| Local model | OpenAI-compatible | no hosted credential | user-supplied loopback HTTP endpoint |
| Custom provider (`custom`) | OpenAI-compatible | user-supplied API key in the OS vault | user-supplied HTTPS endpoint |

The known hosted profiles ship with checked-in model and HTTPS address defaults
verified against the providers' official documentation. After connection, the
app reads the provider's model catalog through its
official Models API. Codex uses App Server `model/list`, including each model's
advertised reasoning levels. The model and reasoning controls remain visible on
the first screen; labels run from Light (`low`) through Max (`max`). Codex
defaults to `gpt-5.6-luna` at `xhigh`. DeepSeek defaults to
`deepseek-v4-flash`; its current official API accepts explicit effort control.
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

The Components screen shows the optional checkbox only when the selected
provider is Codex:

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

The initial-install flatten option remains Codex-only. Separately, when an
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
New plans and locks persist the selected provider, model, reasoning effort,
optimization profile,
endpoint when applicable, and flatten preference without persisting a secret.
Readiness uses generic AI checks for non-Codex profiles. `Open in Codex` and
flattened Chat export remain Codex-only; other providers receive an honest
ready or incomplete state without a fake Codex opener.
