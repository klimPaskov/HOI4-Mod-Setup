# Data models and JSON schemas

## Codex data boundary

ChatGPT authentication is required for Create, Import, Update, and Repair planning. Codex owns OAuth tokens and refresh. Project state, plans, locks, readiness reports, logs, and support bundles never contain tokens, full account identity, plan type, usage, rate limits, thread history, or hidden reasoning.

Persist only the integration type, auth state needed by the current application session, analysis ID, schema version, input and output digests, confirmed proposal keys, confirmation time, and a proof that account identity was not persisted.

## Included schemas

| Schema | Purpose |
| --- | --- |
| `codex-analysis.schema.json` | Schema-constrained semantic proposals from a ChatGPT-authenticated App Server turn |
| `scan-result.schema.json` | Deterministic findings plus required, separately classified Codex suggestions |
| `project-state.schema.json` | Wizard progress, non-secret preferences, and transient Codex integration state |
| `installation-plan.schema.json` | Confirmed semantic analysis, planned project and external operations, approvals, and rollback behavior |
| `installation-lock.schema.json` | Installed revision, confirmed analysis digests, file ownership, hashes, merge choices, and local modifications |
| `readiness-report.schema.json` | Evidence-backed core, launcher, Codex, wiki, Git, MCP, and optional workflow status |
| `remote-manifest.schema.json` | Source components, files, dependencies, tools, environments, validation, and update rules |
| `conflict-record.schema.json` | Base, local, incoming, resolution, and evidence for one conflict |
| `transaction-journal.schema.json` | Durable stage and operation checkpoints for recovery and rollback |

`scan-result.schema.json` requires bounded-scan completion metadata (`partial`, `cancelled`, `limits_hit`, and file/directory/byte counters). Live stage/path progress is delivered only through correlated Tauri events and is not persisted as project content.

## Separation

### Codex analysis

The model output contains proposals, confidence, concise reasons, evidence references, component recommendations, and warnings. The app adds engine, transport, auth mode, input manifest, and response digest metadata outside the model output.

### Scan result

Deterministic findings use `origin: deterministic`. Codex proposals use `origin: codex_suggested`. User-edited or accepted values use `origin: user_confirmed`. The two evidence classes are never merged into one confidence score.

### Plan

A plan records one confirmed Codex analysis, the exact manifest
`wiki.required_pages` list for its resolved revision, and exact operations. It
may target the project, the external HOI4 launcher directory, or application
data. It contains no account metadata. Optional workflow credentials are
represented only by an opaque OS-vault reference; the value is never serialized
into the plan, state, lock, journal, or logs.

Manifest-declared external actions also carry a secret-free argument list,
working-directory placeholder, environment variable names, expected writes,
network/privilege evidence, and rollback boundary. Missing declarations remain
visible as `not_declared` rather than becoming inferred capabilities.

### Lock

The lock records source revisions, the exact required wiki-page list for that
revision, generated and downloaded files, external launcher ownership, installed
hashes, merge decisions, optional states, and confirmed analysis digests. A
legacy lock missing the list is readable but readiness remains incomplete until
source evidence is refreshed. It must never be used as an authentication cache.

### Project state

Project state records wizard progress, preferences, App Server integration state, and opaque references for non-ChatGPT secrets such as `MESHY_API_KEY`. It can be recreated without changing installation ownership.

### Readiness

Readiness records whether ChatGPT authentication was verified during setup, whether required analysis was confirmed, whether account metadata stayed out of artifacts, and which checks block Open in Codex.

### Journal

The journal records transaction state only. Semantic analysis is complete before dry-run approval and is not rerun inside apply or rollback.

## Generated launcher artifacts

Generated `descriptor.mod`, the external `<project_id>.mod`, and `thumbnail.png` are first-class operations and lock rows. Every row records `location_scope`, generator source, installed hash, ownership, platform, and rollback behavior. The external descriptor receives backup and restoration like any other managed destination.

## Credential references

Only optional external workflow credentials use opaque OS-vault references. ChatGPT tokens remain fully owned by Codex and have no project credential reference.

## Evolution

Schemas use explicit versions. The 1.0 lock migration inserts an empty
`wiki_required_pages` marker for legacy locks; readiness treats that marker as
incomplete rather than guessing from the current application bundle. Add
migrations before changing required fields. Unknown major versions block.
Unknown minor fields can be ignored only when the schema permits them.

## Examples

Every schema has a realistic example. Examples must pass Draft 2020-12 validation in CI. New examples include ChatGPT-authenticated analysis, launcher-ready generated files, external destinations, and account-data exclusion.

## Atomic writes

Write project state, plans, locks, readiness, and journals to a sibling temporary file, flush, atomically replace, and retain recovery evidence. Never expose a partial JSON document as current state.
