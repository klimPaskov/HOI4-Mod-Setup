# MCP setup

## Goal

MCP setup is derived from repository evidence. The app exposes each server's purpose, installation source, requirements, environment, state, and health.

## Current evidence

The inspected `.codex/config.toml` contains `hoi4_agent_tools` with command `hoi4-agent-tools.cmd`. The README describes a global npm install. The 3D bootstrap adds Meshy and Blender-related entries through repository wrappers.

These findings drive the example manifest. They do not authorize macOS equivalents.

The current source does not provide immutable executable, `cmd.exe`, or Node
hash/size evidence, package identity, or version for `hoi4-agent-tools.cmd`.
The app therefore records the Windows declaration and exposes it for review,
but reports its health route as `planned_unavailable` and never executes an
arbitrary same-named `PATH` entry.
No package, command, version, or macOS route is invented.

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

1. Require manifest executable, command-interpreter, and runtime SHA-256 and
   size evidence.
2. Resolve the reviewed bare target and verify the regular, link-free file's
   size and SHA-256.
3. Resolve and verify the regular, link-free `cmd.exe` and Node files against
   the same manifest evidence.
4. Recheck the interpreter identity immediately before starting the approved
   server with a sanitized environment.
5. Send MCP initialize.
6. Verify protocol response.
7. List tools or capabilities when safe.
8. Stop cleanly.

Do not run paid or mutating provider actions as generic health checks unless the repository declares a safe operation.

When immutable health-check evidence is unavailable, installed readiness may
report the exact configured bare command as present after a read-only PATH
lookup. This is a display state only; it does not run the command or weaken the
health-check identity requirements above.

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
