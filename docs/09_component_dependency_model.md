# Component dependency model

## Entities

### Component

A versioned installable unit from the remote manifest.

### Dependency edge

A directed requirement. The expanded graph must be acyclic.

### Profile

A named default set such as Core or Core plus 3D.

### Tool requirement

An external executable or package with version policy and health checks.

### Environment requirement

A named variable with secret and storage policy.

### Validation rule

A pass, warning, or blocking check.

## Resolution

1. Verify the selected provider configuration and confirmed semantic analysis; for Codex, verify the required App Server capability and ChatGPT authentication.
2. Start from profile defaults and confirmed user selections.
3. Add transitive dependencies.
4. Detect cycles.
5. Resolve platform support.
6. Mark blocked or unsupported nodes.
7. Compute reverse dependencies.
8. Produce install order.

The UI shows why every automatic dependency is selected.

The component IDs shown below are the current published graph, not a frozen app
enum. Source profiles and component definitions are loaded from the exact
resolved manifest. Newly published default components flow into new setup and
are added during Update only when they were absent from the installed source
profile; compatible new optional `workflow.*` declarations become explicit
choices. Dependencies, provider restrictions, platform support, checksums, and
conflict review still apply before selection becomes installable.

## Suggested component set

| ID | Ownership | Default | Current support |
| --- | --- | --- | --- |
| `ai.provider` | selected semantic capability | yes | Windows and macOS when the selected route is configured |
| `codex.app_server` | Codex-only external capability | Codex profile | Windows and macOS when compatible Codex is installed |
| `project.launcher_scaffold` | generated and external files | new projects | all |
| `core.agents` | merged template | yes | all |
| `core.skills` | managed tree | yes | all |
| `core.subagents` | managed tree | yes | all |
| `codex.config` | structured merge | yes | all |
| `mcp.hoi4_agent_tools` | MCP and external tool | verified profile | current repository Windows route |
| `docs.mcp_integration` | managed integration guide | yes | all |
| `wiki.snapshot` | managed tree | yes | all |
| `docs.source` | repository reference | optional | all |
| `template.chaos_redux_agents` | project-specific instruction example | optional | all |
| `workflow.3d` | optional workflow | no | current repository Windows route |
| `workflow.super_events` | optional workflow | no | manifest-declared all-platform managed skill tree |
| `workflow.portraits.core` | optional complete portrait contract | no | all |
| `workflow.portraits.router` | selected-provider router | dependency | all |

## File ownership

A file has one primary component. Shared output such as `.codex/config.toml` records structured contributions so removing one component does not delete unrelated local servers.

### Managed

Remote content with per-file lock records.

### Merged

Stores previous base and result for future three-way or structured merge.

### Generated

Built from reviewed state and previewed.

### External

Outside project through a dedicated adapter and backup.

## Structured destinations

TOML uses semantic tables and keys. JSON uses schema-aware paths and identity keys for arrays. AGENTS uses three-way text merge plus project adaptation. Directory trees use independent per-file ownership.

## Platform resolution

A component is supported only when its platform declaration and every command-bearing dependency have a verified route. Unsupported optional components remain visible. A macOS core profile can omit or warn on a current Windows-only MCP component while still installing platform-neutral instructions, skills, subagents, Codex base config, and wiki.

`mcp.hoi4_agent_tools` and `workflow.3d` do not depend on `codex.config`.
Their verified package, bootstrap, external-action review, and app-owned health
checks are provider-neutral. `codex.config` remains a separate Codex-only
component that supplies structural client registration when Codex is selected.

`workflow.super_events` depends on the core skills and subagents, has no tool or
environment requirement, and contributes only its manifest-declared
single selected-only skill and research-agent tree plus the reusable runtime. The
core trees exclude `hoi4-super-events*/**` and
`hoi4_super_event_*.toml`. Selecting it adds those packages and the
corresponding guidance to the adapted `AGENTS.md` and marked sections of the
ordinary planning, events, assets, text/audio, and subagent skills; declining it
strips those sections and adds none of the selected-only files.
The component is provider-neutral and remains optional on both supported
platforms when the verified manifest declares its `all` route.

Each enabled portrait provider depends on both the complete portrait contract
and `workflow.portraits.router`, then adds exactly one provider skill, the
bounded portrait subagent, and non-secret configuration. Dependency expansion
adds the router automatically at the resolved manifest revision. Disabled
projects omit the complete portrait component closure.

## Readiness aggregation

State derives from file integrity, dependencies, tools, environment, platform, conflicts, and validation. Selected-provider configuration, confirmed provider analysis, and required launcher artifacts flow into core readiness. ChatGPT authentication and confirmed Codex analysis flow into the Open in Codex gate only for the Codex profile. Unselected or incomplete optional workflows do not.

## Removal

A component cannot be removed while another selected component depends on it. Shared merged destinations remove only the selected component's contribution.

## Update dependency changes

New dependencies are shown and approved. Removed component files are deleted only when unmodified and solely managed. A changed command, package source, or credential requirement always receives review.

When a lock records `workflow.super_events` as `not_selected`, Update may add it
from the target manifest. Repair may add it only from the exact locked revision
when that revision declares the component; otherwise the dependency change is an
Update action. The selected state and managed file ownership are retained in
the lock and surfaced by scan.
