# MCP setup

## Goal

MCP setup is derived from repository evidence. The app exposes each server's purpose, installation source, requirements, environment, state, and health.

## Current evidence

The inspected `.codex/config.toml` contains `hoi4_agent_tools` with command `hoi4-agent-tools.cmd`. The README describes a global npm install. The 3D bootstrap adds Meshy and Blender-related entries through repository wrappers.

These findings drive the example manifest. They do not authorize macOS equivalents.

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

1. Start the approved server with a sanitized environment.
2. Send MCP initialize.
3. Verify protocol response.
4. List tools or capabilities when safe.
5. Stop cleanly.

Do not run paid or mutating provider actions as generic health checks unless the repository declares a safe operation.

## Capability display

The current HOI4 Agent Tools documentation describes focus, event, technology and doctrine, weighted logic, scripted GUI, and map support. Show these as declared capabilities. Refine them with the live tool list and never invent a missing viewer.

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
