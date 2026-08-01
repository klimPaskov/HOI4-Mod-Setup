# Optional workflow architecture

## Goal

Optional workflows use the same component, transaction, update, and readiness architecture as core components. They differ in selection and blocking policy.

The current optional registry includes:

| Component | Question order | Destination | Credentials and actions | Core blocking |
| --- | --- | --- | --- | --- |
| `workflow.3d` | first | manifest-declared 3D files | Meshy reference and repository-declared actions when selected | no |
| `workflow.super_events` | immediately after the 3D question | skill plus manifest-declared `interface/`, `common/`, `events/`, `localisation/`, `gfx/`, and guide files | none | no |

`workflow.super_events` is provider-neutral. Provider configuration is still
required for a planning session, but this component does not change provider
selection, require a provider credential, or add a provider-specific health
route. Selecting it expands to hidden optional dependencies that form one
working, namespace-adapted visual runtime with editable DDS and Photoshop
templates. Declining it installs none of those dependencies.

## Workflow interface

Every optional workflow provides:

- metadata and question text
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

This prevents a missing optional workflow key from blocking normal provider use and prevents an incomplete workflow from looking ready. A missing selected-provider configuration still blocks semantic planning; Codex-only controls remain hidden for other providers.

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

A new workflow should require no wizard redesign. The manifest adds a component and the UI renders question, explanation, dependencies, credentials, readiness, and maintenance actions.

External workflow links shown after completion do not implement this interface
and do not enter project state, plans, locks, maintenance, or readiness. In
version 1 the only such link is the fixed HTTPS GitHub destination
`https://github.com/klimPaskov/comfyui-hoi4-portraits`, opened through the typed
browser action rather than a shell or dynamic URL.

## Acceptance rules

- declining generates no hidden operations
- unsupported commands never run
- missing credentials are never serialized as empty strings
- state persists across update and repair
- removal respects reverse dependencies
- core and optional readiness remain distinct
- the optional question order remains 3D first and Super Events immediately
  after it
- adding Super Events during Repair is allowed only when the locked manifest at
  the same immutable revision declares the component; otherwise Update is the
  required route
