# MCP setup

## Goal

MCP setup is derived from repository evidence. The app exposes each server's purpose, installation source, requirements, environment, state, and health.

## Current evidence

The inspected `.codex/config.toml` contains `hoi4_agent_tools` with command `hoi4-agent-tools.cmd`. The README describes a global npm install. The verified repository also publishes `docs/systems/hoi4_agent_tools_mcp_integration.md`, which the core profile installs through the managed `docs.mcp_integration` component. The guide documents capability, evidence, probability, rewrite-recovery, and troubleshooting behavior; it does not itself authorize an executable. The 3D bootstrap adds Meshy and Blender-related entries through repository wrappers.

These findings drive the example manifest. They do not authorize macOS equivalents.

The current source pins `hoi4-agent-tools@2.5.2` by npm SHA-512 integrity,
canonical package-tree SHA-256 and file count, runtime-entry SHA-256 and size,
and required tool names. The bootstrap and app independently verify the full
installed package tree, so changing an imported sibling module fails before
Node starts. The verified bytes are materialized into a private temporary tree
and Node executes that copy, preventing a change between verification and
spawn. The `.cmd` wrapper is used only to locate the current-user npm
prefix and is never executed. Node must be a regular link-free executable with
a valid OpenJS Foundation signature; its actual SHA-256 is captured and
rechecked immediately at spawn. No package, command, version, or macOS route is
invented.

## MCP component fields

- server ID and display name
- capabilities
- source component
- platform support
- command and args
- cwd rule
- environment bindings
- required tools
- install source and version policy
- startup and tool timeouts
- health operation
- removal behavior

## Structured merge

Parse `.codex/config.toml` and preserve unrelated local servers. Validate command, cwd, secret handling, timeouts, duplicate IDs, and platform support.

Security-sensitive root values such as `approval_policy` and `sandbox_mode` receive explicit review. They are not copied silently.

## Health check

1. Require exact package name/version/integrity, canonical package-tree
   SHA-256/file count, runtime-entry SHA-256/size, and required tool evidence.
2. Resolve the reviewed wrapper only to its regular, link-free npm prefix; do
   not execute it.
3. Verify package metadata, npm lock integrity, every installed package byte,
   and the runtime entry.
4. Require the resolved Node executable's OpenJS Foundation signature, capture
   its SHA-256, and recheck that identity immediately at spawn.
5. Send the MCP JSON-RPC initialize request with protocol version, empty client
   capabilities, and client info, followed by `notifications/initialized`.
6. Require the exact negotiated protocol version and an advertised `tools`
   capability.
7. Call `tools/list` and require every source-advertised route, including all
   three Technology Tree routes.
8. Stop cleanly.

Do not run paid or mutating provider actions as generic health checks unless the repository declares a safe operation.

When any package-tree, entry, publisher, or tool evidence is unavailable, the
route remains unavailable and no same-named command is executed.

## Capability display

The current HOI4 Agent Tools documentation and manifest describe focus, event,
weighted-logic, scripted-GUI, map, technology, and doctrine routes. The
Technology Tree Viewer is exposed through `hoi4.tech_inspect`,
`hoi4.tech_render`, and `hoi4.tech_compare`. Show only the capabilities declared
by the resolved manifest and refine them with the live MCP tool list; a future
addition or removal comes from that exact source revision rather than an app
hardcode.

## Credentials

Display only name, required state, secret state, source, and available or missing. Never display the value after entry.

## Status model

- not selected
- unsupported platform
- tool missing
- install planned
- installed, health not run
- healthy
- degraded
- blocked

## Update

Compare version policy, package evidence, command, configuration, environment requirements, and live tool list. Any package-source, command, or credential change requires review.

## Removal

Remove only managed configuration contributions. Do not automatically uninstall a global package that may be shared by other projects. Report its external state instead.

## Security review

Highlight global package installation, network code download, writes outside the project, broad Codex sandbox settings, and servers that receive secrets.
