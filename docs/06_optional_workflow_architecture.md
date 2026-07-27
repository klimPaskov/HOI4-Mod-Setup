# Optional workflow architecture

## Goal

Optional workflows use the same component, transaction, update, and readiness architecture as core components. They differ in selection and blocking policy.

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
| `interest_recorded` | Preference saved without install capability |
| `planned_unavailable` | Product has no implementation yet |
| `unsupported_platform` | No verified route for the current platform |
| `removed` | Previously managed workflow removed |

## Blocking policy

Optional checks use `blocking: false` for the core readiness gate. A selected workflow can still fail its own Ready state. The final report separates:

- core ready for Codex
- 3D workflow state
- LoRA and ComfyUI state

This prevents a missing provider key from blocking normal Codex use and prevents an incomplete workflow from looking ready.

## Dependencies

Selecting a workflow expands its dependency DAG. Automatically selected dependencies remain visible with an explanation. Deselecting a required dependency either deselects the workflow or explains the reverse dependency.

## External actions

Optional workflows can add commands only when the manifest or verified repository script declares them. The typed dry-run action shows the reviewed executable/argument description, cwd, environment variable names, source, manifest-declared expected writes, network and privilege evidence, risk level, and rollback boundary. Unknown values remain explicitly `not_declared`; the UI never fills them by guesswork.

## Credentials

Only opaque references enter project state or lock files. The secret remains in the OS vault and is read into memory only for an approved process.

## Future workflows

A new workflow should require no wizard redesign. The manifest adds a component and the UI renders question, explanation, dependencies, credentials, readiness, and maintenance actions.

The LoRA and ComfyUI placeholder deliberately uses this interface so a future implementation can replace `planned_unavailable` without changing the main flow.

## Acceptance rules

- declining generates no hidden operations
- unsupported commands never run
- missing credentials are never serialized as empty strings
- state persists across update and repair
- removal respects reverse dependencies
- core and optional readiness remain distinct
