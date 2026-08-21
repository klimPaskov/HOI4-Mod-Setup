# 3D workflow setup and credential design

## Verified repository surface

The inspected route includes:

- `.agents/skills/hoi4-3d-model-pipeline/SKILL.md`
- `.codex/agents/hoi4_3d_model_pipeline.toml`
- `.tools/3d_pipeline/bootstrap_3d_workflow.py`
- `.tools/3d_pipeline/adapter/hoi4_blender_mcp.py`
- `.tools/3d_pipeline/adapter/blender_worker.py`
- `.tools/3d_pipeline/adapter/pyproject.toml`
- `.tools/3d_pipeline/config/asset_profiles.json`
- `.tools/3d_pipeline/config/meshy_tool_contract.json`
- `.tools/3d_pipeline/wrappers/run_meshy_mcp.cmd`
- `.tools/3d_pipeline/wrappers/run_blender_lab_mcp.cmd`
- `.tools/3d_pipeline/wrappers/run_blender_hoi4_adapter.cmd`

`dependencies.lock.json`, runtime discovery files, caches, virtual
environments, downloaded dependencies, and Blender extensions are bootstrap
outputs. They are not immutable source payload files.

The workflow requires a non-empty `MESHY_API_KEY` before provider work. It specifies one final Meshy reference image, provider lineage, Blender processing, PDX materials, `io_pdx_mesh`, skeletal actions, export, reimport evidence, and parent-owned runtime wiring.

HOI4 Mod Setup installs and verifies this route. It does not replace the production pipeline.

## Workflow title

**3D models workflow**

When yes, explain:

- Meshy.ai requires an API key
- provider actions may consume credits
- the key stays outside the project
- approved processes receive it as `MESHY_API_KEY`
- setup can be completed later
- platform support comes from the remote manifest

## Secret storage

### Windows

Use Windows Credential Manager. Protect fallback metadata with DPAPI. Store service name, credential label, opaque reference, creation time, and last validation time. Do not default to persistent user environment variables.

The repository's PowerShell command can be displayed as a manual fallback, but the desktop app should prefer the credential vault and process injection.

### macOS

Use Keychain Services with an application-scoped service and account label. Do not edit shell profiles.

## Validation

### Local check

- reference resolves
- value is non-empty after whitespace handling
- value never returns to UI after initial entry

### Provider or MCP check

Run only through the repository-declared route. Report initialize result, safe account or balance result when declared, and sanitized error category. Never show environment dumps, headers, or command strings containing a secret.

The reviewed transaction invokes the verified `3d.bootstrap` target during
post-install checks when the workflow is selected. It checks the installed
bootstrap file's hash and size, resolves the source-declared Python tool from
the approved PATH, passes the explicit project root and reviewed-config check,
and injects the vault credential only as `MESHY_API_KEY`. Success persists the
workflow as `ready`; a missing credential, missing tool, or bootstrap failure
persists `incomplete` and never changes core readiness. Installed-project health
checks repeat the same lock, source, path, hash, argument, and credential
binding.

## Process injection

1. Resolve credential reference.
2. Read the secret into protected memory.
3. Build child environment with `MESHY_API_KEY`.
4. Spawn only the approved executable and arguments.
5. Redact diagnostics and crash context.
6. Drop the value after process creation and result handling.

The lock stores only an opaque reference and state.

The transaction persists only the sanitized readiness state and bounded action
evidence in its journal and lock. An explicit later health result is also kept
as a small core-owned session cache. It stores only `ready` or `incomplete`, the
canonical project root, and a fingerprint of the locked workflow revision,
manifest hash, and installed workflow file hashes. Neither surface stores the
Meshy value, raw process output, or provider response. A changed lock,
Meshy-vault reference, or a new desktop session makes the cached result stale
and requires another explicit health check.

The UI's explicit “Delete stored key” action deletes only the current
platform-generated Meshy reference from the OS vault. It does not receive or
display the secret, and managed component removal never invokes this action.

## Repository bootstrap behavior

The inspected script gates on the key, discovers local tools and Blender,
ensures `uv` when missing, resolves Meshy package metadata, prepares the
repository-owned bounded Blender adapter, resolves `io_pdx_mesh`, downloads the
declared dependencies when needed, verifies the app-prepared MCP configuration,
writes dependency evidence, and runs bridge checks.

These are high-impact external actions. The app shows the reviewed target,
arguments, cwd, environment names, declared writes, network/privilege evidence,
and rollback boundary in the dry run, then executes the verified script only
after approval. Managed project files remain journaled and reversible. The
source explicitly declares that downloaded dependencies, user-level `uv`, and
Blender extensions are external tool state and are reported but not removed by
project rollback.

## No invented tools

Never substitute another Meshy package, Blender MCP, exporter, adapter, version, installation command, or health check. An absent route produces `unsupported_platform` or `required installation/verification`.

## Platform behavior

The inspected route is Windows-oriented. The manifest example marks it Windows-only. On macOS, show the workflow, explain the limitation, keep core setup usable, and do not translate commands by guesswork.

## Health matrix

| Requirement | Check |
| --- | --- |
| Meshy key | credential exists and non-empty |
| Python | approved executable and invocation |
| Git | executable and version output |
| Node and npx | executable and package route |
| Blender | executable, build, extension location |
| Blender MCP | source revision, add-on, bridge |
| `io_pdx_mesh` | release, archive hash, installation |
| Codex MCP config | TOML parse and initialize |
| 3D skill and subagent | files and hashes |

## Missing-key flow

Install the selected workflow shell only when approved, skip key-dependent actions, mark `incomplete`, show Configure key in Update and Repair, and keep Open in Codex enabled when core checks pass.

## Repair actions

- configure or replace key
- rerun dependency preflight
- rerun MCP health
- reinstall repository files
- rerun bootstrap
- inspect generated dependency evidence
- remove 3D-only components

## Logging

Allowed: credential reference, present or missing, health status, sanitized task IDs from later production use.

Forbidden: value, prefix, suffix, hash, environment dump, authorization header, or revealing length.
