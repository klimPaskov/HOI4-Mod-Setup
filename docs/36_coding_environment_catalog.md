# Coding environment catalog

## Status and scope

This document is the accepted catalog of coding clients for the Components
phase and the contract for manifest format 6. It supplements the selection
contract in `AGENTS.md`, `docs/09_component_dependency_model.md`, and
`docs/31_ai_provider_profiles_and_chat_sources.md`.

Every path, file format, and MCP route below was checked against the vendor's
official documentation on 2026-10-17. Documentation proves that a client
reads a file; it does not prove that the client runs well on every machine.
HOI4 Mod Setup never starts a coding client during setup, so client behavior
beyond reading these files is outside its verification.

The source package publishes these clients only in manifest format 6. Until
the Agentic HOI4 Modding source merges format 6 together with an app release
that reads it, Latest installs offer the original five clients.

## Selection contract

Coding-environment selection stays independent of the setup assistant and of
the Core, 3D, portrait, Super Events, and live-testing choices:

- New installs have exactly one primary environment. The resolved manifest
  default is the initial primary; the package default is Claude Code.
- Any other offered environment may be selected as an additional client. The
  primary is never listed as an additional choice.
- Existing projects keep their recorded selection. A legacy project with no
  recorded primary uses Codex as the compatibility migration default; this
  does not change the new-install default.
- Primary and additional IDs are persisted in project state, plans, locks, and
  journals. Update, repair, reinstall, rollback, and removal preview client
  changes and preserve existing or modified files.
- Choosing Gemini or Grok as the setup assistant never selects, installs, or
  hides the Gemini CLI or Grok Build coding client, and the reverse.
- If a project records a client that the chosen source version does not
  offer, maintenance shows it as installed but unavailable and planning stops
  with an actionable message. Its files are never removed implicitly.
- The app never writes a user's global, IDE, or cloud client settings. A
  manual MCP route is shown as a to-do with the reviewed vendor guide.

## Manifest format 6

Format 6 adds two declarations. Earlier app releases (v0.6.2 and older)
reject a format 6 manifest during schema validation, so they never offer a
client they cannot register; their error is a generic schema failure. From
this release on, a newer major is reported as needing an app update before
any schema is applied.

Each `coding_environments` entry declares `mcp_support`:

| Value | Meaning | Extra fields |
| --- | --- | --- |
| `project` | A client-recognized project file that the installer manages. | `mcp_registration`: `component_id`, `path`, and `format` |
| `manual` | The client reads MCP servers only from settings it owns. | Optional `documentation_url` |
| `none` | The client has no native MCP support. | Optional `documentation_url` |

`format` is a closed vocabulary: `json_mcp_servers` (`mcpServers` with
`command` and `args`), `vscode_servers` (`servers` with `type: "stdio"`),
`local_command_array` (`mcp` with `type: "local"` and an array `command`),
and `toml_mcp_servers` (`[mcp_servers.<id>]`). The registration component must
belong to the same client and install exactly that path.

A component may also list `shared_coding_environments`: other clients that
read its files from the same path. One installed copy then serves the owner
and every listed client, so selecting several of them never duplicates a
destination. Deselecting one client keeps the copy while another selected
client still uses it.

The app offers a format 6 client only when:

- for `project`, its built-in client table has an adapter whose path and format
  match the declaration. An unknown route is shown under "Not available" with
  "needs a newer version of HOI4 Mod Setup";
- for `manual` or `none`, the client's non-optional components exist. These
  guidance-only packages need no app release;
- every non-optional component of the client supports the current platform.

Format 5 and older manifests offer only the five clients those formats
shipped.

## Catalog

| ID | Client | Project files | Skills read from | MCP |
| --- | --- | --- | --- | --- |
| `claude_code` | Claude Code | `CLAUDE.md`, `.claude/` settings, agents, map | `.claude/skills/` (canonical) | Project `.mcp.json` |
| `codex` | Codex | `AGENTS.md`, `.codex/` agents and config | `.agents/skills/` (shared copy) | Project `.codex/config.toml` |
| `cursor` | Cursor | `.cursor/` agents, settings, map | `.claude/skills/` | Project `.cursor/mcp.json` |
| `qoder` | Qoder | `.qoder/` agents, settings, map | Qoder's own roots | Project `.qoder/mcp.json` |
| `opencode` | OpenCode | `opencode.json`, `.opencode/` agents and map | `.claude/skills/` | Project `opencode.json` (`mcp`) |
| `github_copilot` | GitHub Copilot (VS Code) | `.github/instructions/hoi4-mod-setup.instructions.md`; reads `AGENTS.md` and `.claude/agents/` | `.claude/skills/` | Project `.vscode/mcp.json` |
| `gemini_cli` | Gemini CLI | `GEMINI.md` importing `@./AGENTS.md`, `.gemini/agents/` | `.agents/skills/` (shared copy) | Project `.gemini/settings.json` |
| `antigravity` | Antigravity | `AGENTS.md`, `.agents/agents/` | `.agents/skills/` (shared copy) | Project `.agents/mcp_config.json` |
| `grok_build` | Grok Build | `AGENTS.md` | `.grok/skills/` (copy) | Project `.grok/config.toml` |
| `kilo_code` | Kilo Code | `AGENTS.md`, `.kilo/agents/` | `.agents/skills/` (shared copy) | Project `.kilo/kilo.json` (`mcp`) |
| `kiro` | Kiro | `AGENTS.md`, `.kiro/agents/` | `.kiro/skills/` (copy) | Project `.kiro/settings/mcp.json` |
| `qwen_code` | Qwen Code | `AGENTS.md`, `.qwen/agents/` | `.qwen/skills/` (copy) | Project `.qwen/settings.json` |
| `cline` | Cline | `AGENTS.md`, `.clinerules/hoi4-mod-setup.md` | `.claude/skills/` | Manual (Cline settings) |
| `devin_desktop` | Devin Desktop | `AGENTS.md`, `.devin/agents/` | `.agents/skills/` (shared copy) | Manual (Devin settings) |
| `augment` | Augment (Auggie CLI) | `AGENTS.md`, `.augment/agents/` | `.claude/skills/` | Manual (Auggie settings) |
| `continue` | Continue | `.continue/rules/hoi4-mod-setup.md` (always applied) | Read on demand from `.claude/skills/` | Manual (Continue settings) |
| `aider` | Aider | `.aider.conf.yml` reading `AGENTS.md` | Not applicable | None |

Generated subagents for Gemini CLI, Antigravity, Kiro, Qwen Code, Devin
Desktop, and Augment carry `name` and `description`; Kilo Code agents carry
`description` and `mode: subagent`. They set no model or tool field, so each
client applies its session model and tools, and the prompt body states a
read-only role. Grok Build, Cline, Continue, and Aider receive no subagent
projection because no verified project format exists for them.

### Platform notes

- Every package installs on Windows, macOS, and Linux except Augment, which
  is declared for macOS and Linux. Auggie reaches Windows only through WSL,
  and no WSL route is verified here.
- Project MCP files are installed only where the HOI4 Agent Tools package is
  verified (Windows and Linux). `opencode.json` is installed everywhere; its
  route is removed on platforms without a verified launch.

### Manual and absent MCP routes

These boundaries were confirmed rather than assumed:

- **Cline**: only a global MCP settings file is documented.
- **Devin Desktop**: the default Devin Local agent's documentation names
  `.devin/config.json`, while the Devin CLI documents `.devin/mcp_config.json`.
  The project route stays manual until the two agree.
- **Augment**: the project `.augment/settings.json` route is referenced by the
  CLI configuration page but was not verified on a fetched page.
- **Continue**: the documented native format is a YAML file per server, and
  the shape of a copied JSON file is not documented.
- **Aider**: no native MCP client exists.

### Product status notes

- Gemini CLI remains available for enterprise licences and paid API keys.
  Since 2026-06-18, free and Google One users are moved to the Antigravity
  CLI, which is the separate `antigravity` entry.
- Windsurf was renamed Devin Desktop on 2026-06-02.
- Roo Code shut down on 2026-05-15 and is excluded.

## Corrections to earlier research

The pre-implementation draft of this document contained errors that the
2026-10-17 review corrected:

- OpenCode reads MCP servers only from `opencode.json(c)` under `mcp`. The
  format 5 package installed `.opencode/mcp.json`, which OpenCode never read.
  Format 6 moves the route into `opencode.json` and retires that component.
  Readiness keeps the old structural check only while a format 5 lock still
  records `.opencode/mcp.json` for that component; Update removes the route
  from the unchanged file and retires the component.
- Augment has project instructions (`CLAUDE.md`, `AGENTS.md`,
  `.augment/rules/`), skills (`.augment/skills/`, `.claude/skills/`,
  `.agents/skills/`), and agents (`.augment/agents/`), and runs on macOS,
  Linux, and Windows through WSL.
- Devin Desktop reads skills from `.agents/skills/` and agents from
  `.devin/agents/` or `.agents/agents/`; only its MCP route is unresolved.
- Gemini CLI reads `AGENTS.md` only when `context.fileName` names it, so the
  package imports it from `GEMINI.md`.
- Copilot agent files may use `*.agent.md`; VS Code also reads
  `.claude/agents/`. The Copilot CLI reads `.mcp.json`, not `.vscode/mcp.json`.
- Kilo Code loads `.kilo/rules/` only when listed in `instructions`; the
  package relies on `AGENTS.md` instead.

## Known limitations

- A file of a deselected client that the user changed and kept is released
  from management: it stays on disk untouched and no longer counts as a local
  modification.
- A deselected client's MCP file that the installer merged with content the
  user already had keeps the setup's own non-route keys (for example a
  `context` setting) when the shared route is removed: the lock records the
  merged result, not a byte-exact base. Only the `hoi4_agent_tools` route is
  removed.
- A file of a deselected client that the installer created before the lock
  recorded creation evidence (locks written before 2026-10-17) is never
  deleted: it keeps its empty server table and leaves management.
- MCP files whose lock entry belongs to an image-generation or live-testing
  component keep the existing reverse-merge review on removal, because their
  own planners regenerate those routes for the clients that stay selected.
- `.vscode/mcp.json`, `opencode.json`, and `.kilo/kilo.json` may contain JSONC
  comments. They are parsed tolerantly, but a merge writes plain JSON, so a
  reviewed merge drops those comments.

## Scanner, readiness, and transaction rules

- The read-only scanner detects each client from its markers inside the
  project only. Client folders are observed by presence, without reading
  files that could hold credentials; only the small native MCP files are read
  and parsed, and an unparseable one is a blocking conflict.
- Readiness checks each selected client's layout. Where the client's MCP
  component and HOI4 Agent Tools are both installed on Windows or Linux, it
  requires the registered `hoi4_agent_tools` route, not just parseable JSON.
  Manual and absent routes never block readiness.
- The MCP health check verifies the launch in every selected project route and
  skips manual and absent routes.
- Native MCP files are merged with local content. Only the
  `hoi4_agent_tools` route is retargeted to the verified launch; other servers
  and keys are preserved.
- When a client is deselected, removed, or dropped by a newer manifest, its
  unchanged merged MCP file goes through a reviewed reverse merge:
  `native_mcp::retarget(format, bytes, None)` removes only the shared route
  (`mcp_route_removal`). The file is deleted instead only when the lock
  records that the installer created it (`installer_created`) and nothing but
  an empty server table would remain. A file without the route is released
  untouched (`mcp_route_release`). Either way the file leaves the lock, and a
  component with no remaining files is retired. A file the user changed keeps
  today's behavior: it is kept and, for a deselected client, released from
  management.
- Readiness applies the legacy OpenCode check only while the legacy
  component still owns its locked `.opencode/mcp.json`.
- Image-generation and live-testing routes are adapted only for the five
  original clients until the source provider tables declare the others.

## Evidence

- Rust: registry invariants, every MCP format's read, retarget, and removal,
  reverse merge on removal and update for every format, creation evidence,
  component retirement, the staged route removal with rollback to the
  original bytes, and every stage, operation, and quarantine fault boundary,
  format 6 schema and semantic validation, availability gating, shared
  components, readiness layouts and routes, and the legacy OpenCode check.
- UI: MCP cues, the unavailable list, the Ready to-do, maintenance handling
  of unavailable recorded clients, and setup-assistant independence.
- Source: generated declarations, destination uniqueness, projection
  coverage, MCP file shapes, and pull-request evidence previews built by the
  owned generator.
- Hosted candidate acceptance (`clients`): Latest install, update to the
  pinned format 6 source adding every client, client deselection with a
  modified agent and a user MCP server kept, repair of a deleted projection,
  and refusal to move to an older format that lacks the recorded clients.

No real coding client is started and no client sign-in is tested.

## Primary documentation sources

- Gemini CLI: [GEMINI.md](https://geminicli.com/docs/cli/gemini-md/), [imports](https://geminicli.com/docs/reference/memport/), [skills](https://geminicli.com/docs/cli/skills/), [subagents](https://geminicli.com/docs/core/subagents/), [MCP](https://geminicli.com/docs/tools/mcp-server/), [transition notice](https://developers.googleblog.com/en/an-important-update-transitioning-gemini-cli-to-antigravity-cli/)
- Antigravity: [rules](https://antigravity.google/docs/rules/), [skills](https://antigravity.google/docs/skills), [subagents](https://antigravity.google/docs/subagents/), [MCP](https://antigravity.google/docs/mcp)
- Grok Build: [project rules](https://docs.x.ai/build/features/project-rules), [skills](https://docs.x.ai/build/features/skills-plugins-marketplaces), [MCP](https://docs.x.ai/build/features/mcp-servers), [settings](https://docs.x.ai/build/settings/reference)
- GitHub Copilot: [instructions](https://code.visualstudio.com/docs/copilot/customization/custom-instructions), [skills](https://code.visualstudio.com/docs/copilot/customization/agent-skills), [agents](https://code.visualstudio.com/docs/copilot/customization/custom-agents), [MCP configuration](https://code.visualstudio.com/docs/agents/reference/mcp-configuration), [Copilot CLI MCP](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)
- Kilo Code: [AGENTS.md](https://kilo.ai/docs/customize/agents-md), [skills](https://kilo.ai/docs/customize/skills), [subagents](https://kilo.ai/docs/customize/custom-subagents), [MCP](https://kilo.ai/docs/automate/mcp/using-in-kilo-code)
- Kiro: [steering](https://kiro.dev/docs/steering/), [skills](https://kiro.dev/docs/skills/), [agents](https://kiro.dev/docs/custom-agents/configuration-reference/), [MCP](https://kiro.dev/docs/mcp/configuration/)
- Qwen Code: [memory](https://qwenlm.github.io/qwen-code-docs/en/users/features/memory/), [skills](https://qwenlm.github.io/qwen-code-docs/en/users/features/skills/), [subagents](https://qwenlm.github.io/qwen-code-docs/en/users/features/sub-agents/), [MCP](https://qwenlm.github.io/qwen-code-docs/en/users/features/mcp/)
- Cline: [rules](https://docs.cline.bot/customization/cline-rules), [skills](https://docs.cline.bot/customization/skills), [MCP](https://docs.cline.bot/mcp/configuring-mcp-servers)
- Devin: [Devin Local](https://docs.devin.ai/desktop/devin-local), [skills](https://docs.devin.ai/cli/extensibility/skills/overview), [subagents](https://docs.devin.ai/cli/subagents), [CLI MCP](https://docs.devin.ai/cli/extensibility/mcp/configuration)
- Augment: [rules](https://docs.augmentcode.com/cli/rules), [skills](https://docs.augmentcode.com/cli/skills), [subagents](https://docs.augmentcode.com/cli/subagents), [MCP](https://docs.augmentcode.com/cli/integrations), [install](https://docs.augmentcode.com/cli/setup-auggie/install-auggie-cli)
- Continue: [rules](https://docs.continue.dev/customize/deep-dives/rules), [MCP](https://docs.continue.dev/customize/deep-dives/mcp)
- Aider: [conventions](https://aider.chat/docs/usage/conventions.html), [configuration](https://aider.chat/docs/config/aider_conf.html)
- OpenCode: [rules](https://opencode.ai/docs/rules/), [agents](https://opencode.ai/docs/agents/), [skills](https://opencode.ai/docs/skills/), [MCP](https://opencode.ai/docs/mcp-servers/)
- Roo Code: [repository notice](https://github.com/RooCodeInc/Roo-Code)
