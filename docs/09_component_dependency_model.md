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

1. Verify the required Codex App Server capability, ChatGPT authentication, and confirmed semantic analysis.
2. Start from profile defaults and confirmed user selections.
3. Add transitive dependencies.
4. Detect cycles.
5. Resolve platform support.
6. Mark blocked or unsupported nodes.
7. Compute reverse dependencies.
8. Produce install order.

The UI shows why every automatic dependency is selected.

## Suggested component set

| ID | Ownership | Default | Current support |
| --- | --- | --- | --- |
| `codex.app_server` | required external capability | yes | Windows and macOS when compatible Codex is installed |
| `project.launcher_scaffold` | generated and external files | new projects | all |
| `core.agents` | merged template | yes | all |
| `core.skills` | managed tree | yes | all |
| `core.subagents` | managed tree | yes | all |
| `codex.config` | structured merge | yes | all |
| `mcp.hoi4_agent_tools` | MCP and external tool | verified profile | current repository Windows route |
| `wiki.snapshot` | managed tree | yes | all |
| `docs.repository_reference` | docs | optional | all |
| `workflow.3d` | optional workflow | no | current repository Windows route |
| `workflow.lora_comfyui_interest` | preference | no | all |

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

## Readiness aggregation

State derives from file integrity, dependencies, tools, environment, platform, conflicts, and validation. ChatGPT authentication, confirmed Codex analysis, and required launcher artifacts flow into the core Open in Codex gate. Unselected or incomplete optional workflows do not.

## Removal

A component cannot be removed while another selected component depends on it. Shared merged destinations remove only the selected component's contribution.

## Update dependency changes

New dependencies are shown and approved. Removed component files are deleted only when unmodified and solely managed. A changed command, package source, or credential requirement always receives review.
