# Optional workflow architecture

## Goal

Optional workflows use the same component, transaction, update, and readiness architecture as core components. They differ in selection and blocking policy.

The current optional registry includes:

| Component | Order and title | Destination | Credentials and actions | Core blocking |
| --- | --- | --- | --- | --- |
| `workflow.3d` | first: **3D models workflow** | manifest-declared 3D files | Meshy reference and repository-declared actions when selected | no |
| `workflow.super_events` | second: **Super Events workflow** | skill plus manifest-declared `interface/`, `common/`, `events/`, `localisation/`, `gfx/`, and guide files | none | no |
| `workflow.portraits.<provider>` | third: **ComfyUI portrait production** | provider-neutral contract plus the selected provider skill, subagent, and pinned config | provider-specific setup guidance | no |

`workflow.super_events` is provider-neutral. Provider configuration is still
required for a planning session, but this component does not change provider
selection, require a provider credential, or add a provider-specific health
route. Selecting it expands to hidden optional dependencies that form one
working, namespace-adapted visual runtime with editable DDS and Photoshop
templates. Declining it installs none of those dependencies.

## Workflow interface

Every optional workflow provides:

- metadata and title text
- explanation
- platform support resolver
- dependencies
- credential requirements
- preflight
- plan contribution
- health checks
- readiness contribution
- update and repair actions
- removal behavior
- migration behavior

## State model

| State | Meaning |
| --- | --- |
| `not_selected` | User declined |
| `selected_pending` | Selected, setup not complete |
| `ready` | Selected requirements passed |
| `incomplete` | Files may exist, but a requirement is missing |
| `planned_unavailable` | Product has no implementation yet |
| `unsupported_platform` | No verified route for the current platform |
| `removed` | Previously managed workflow removed |

## Blocking policy

Optional checks use `blocking: false` for the core readiness gate. A selected workflow can still fail its own Ready state. The final report separates:

- core ready for the selected AI provider
- 3D workflow state
- Super Events workflow state
- portrait provider state

This prevents a missing optional workflow key from blocking normal provider use and prevents an incomplete workflow from looking ready. A missing selected-provider configuration still blocks semantic planning; Codex-only controls remain hidden for other providers.

Portraits use the same non-blocking core gate. Generic projects support Cloud,
Local, RunPod, and Disabled. A selected portrait provider has its own honest
state and cannot be reported as final while it is unauthorized, unsubscribed,
missing models/workflows, unreachable, or waiting for a RunPod URL/workflow.
Disabled removes the provider components and all ComfyUI-specific marker
content while retaining source-based portrait handling. Non-sourced fictional
or impossible portraits always use native ImageGen and never use the ComfyUI
workflow. See
`docs/32_comfyui_portrait_pipeline.md` for the provider contract and current
upstream evidence.

## Dependencies

Selecting a workflow expands its dependency DAG. Automatically selected dependencies remain visible with an explanation. Deselecting a required dependency either deselects the workflow or explains the reverse dependency.

## External actions

Optional workflows can add commands only when the manifest or verified repository script declares them. The typed dry-run action shows the reviewed executable/argument description, cwd, environment variable names, source, manifest-declared expected writes, network and privilege evidence, risk level, and rollback boundary. Unknown values remain explicitly `not_declared`; the UI never fills them by guesswork.

## Credentials

Only opaque references enter project state or lock files. The secret remains in the OS vault and is read into memory only for an approved process.

`workflow.super_events` has no credential requirement, so its selected,
unselected, incomplete, and ready records contain no credential reference. A
deselected install receives neither its skill, runtime dependencies, nor its
Super Events-specific `AGENTS.md` guidance.

## Future workflows

A new workflow should require no wizard redesign. The manifest adds a component and the UI renders its title, explanation, dependencies, credentials, readiness, and maintenance actions.

The portrait workflow implements this interface. Its provider choice enters
project state, plans, locks, maintenance, and readiness as non-secret state;
provider credentials do not. The canonical repository and exact revision are
recorded as source evidence rather than copied as unpinned instructions.

## Acceptance rules

- declining generates no hidden operations
- unsupported commands never run
- missing credentials are never serialized as empty strings
- state persists across update and repair
- removal respects reverse dependencies
- core and optional readiness remain distinct
- the optional title order remains **3D models workflow**, **Super Events workflow**, then **ComfyUI portrait production**
- adding Super Events during Repair is allowed only when the locked manifest at
  the same immutable revision declares the component; otherwise Update is the
  required route
- portrait provider selection persists across import, settings, Update, and
  Repair
- disabled portrait projects contain no provider components, Cloud MCP entry,
  marker section, or ComfyUI-specific guidance
- selected portrait providers remain non-blocking to core AI readiness but do
  not claim final portrait completion before their provider checks pass
