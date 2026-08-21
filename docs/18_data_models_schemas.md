# Data models and JSON schemas

## AI provider data boundary

The selected provider is required for Create, Import, Update, and Repair
planning. Codex owns ChatGPT OAuth tokens and refresh. Non-Codex keys are
owned by the OS credential vault. Project state, plans, locks, readiness
reports, logs, and support bundles never contain tokens, keys, full account
identity, plan type, usage, rate limits, thread history, or hidden reasoning.

Persist only the integration type, auth state needed by the current application session, analysis ID, schema version, input and output digests, confirmed proposal keys, confirmation time, and a proof that account identity was not persisted.

## Included schemas

| Schema | Purpose |
| --- | --- |
| `codex-analysis.schema.json` | Schema-constrained semantic proposals from a selected provider turn |
| `scan-result.schema.json` | Deterministic findings plus separately classified provider suggestions |
| `project-state.schema.json` | Wizard progress, non-secret preferences, and transient AI integration state |
| `installation-plan.schema.json` | Confirmed semantic analysis, planned project and external operations, approvals, and rollback behavior |
| `installation-lock.schema.json` | Installed revision, confirmed analysis digests, file ownership, hashes, merge choices, and local modifications |
| `readiness-report.schema.json` | Evidence-backed core, launcher, selected AI provider, wiki, Git, MCP, and optional workflow status |
| `remote-manifest.schema.json` | Source components, files, dependencies, tools, environments, validation, and update rules |
| `conflict-record.schema.json` | Base, local, incoming, resolution, and evidence for one conflict |
| `transaction-journal.schema.json` | Durable stage and operation checkpoints for recovery and rollback |
| `git-online-record.schema.json` | Secret-free record of a separately approved online Git action |

`scan-result.schema.json` requires bounded-scan completion metadata (`partial`, `cancelled`, `limits_hit`, and file/directory/byte counters). Live stage/path progress is delivered only through correlated Tauri events and is not persisted as project content. The desktop bridge preserves deterministic finding origins and maps core conflicts into the review surface; provider input uses bounded normalized finding/conflict summaries and their core-bound hashes, not raw inventoried file text.

## Separation

### Provider analysis

The model output contains proposals, confidence, concise reasons, evidence references, component recommendations, and warnings. The app adds engine, transport, auth mode, input manifest, and response digest metadata outside the model output.

### Scan result

Deterministic findings use `origin: deterministic`. Provider proposals use
`origin: provider_suggested`. User-edited or accepted values use
`origin: user_confirmed`. The two evidence classes are never merged into one
confidence score.

### Plan

A plan records one confirmed provider analysis, the selected provider/model/profile, the exact manifest
`wiki.required_pages` list and snapshot/media/provenance/license metadata for its resolved revision, and exact operations. It
may target the project, the external HOI4 launcher directory, or application
data. It contains no account metadata. Optional workflow credentials are
represented only by an opaque OS-vault reference; the value is never serialized
into the plan, state, lock, journal, or logs. Managed-removal plans may carry
`codex_analysis: null` because removal is a local recovery operation and does
not require provider authentication.

Manifest-declared external actions also carry a secret-free argument list,
working-directory placeholder, environment variable names, expected writes,
network/privilege evidence, and rollback boundary. Missing declarations remain
visible as `not_declared` rather than becoming inferred capabilities.

### Lock

The lock records source revisions, the exact required wiki-page list and
snapshot/media/provenance/license metadata for that revision, generated and
downloaded files, external launcher ownership, installed hashes, merge
decisions, optional states including `workflow.super_events`, and confirmed
analysis digests bound to the same exact source revision and manifest SHA-256.
A legacy lock may backfill those analysis fields only from valid source
evidence already present in that lock; absent provenance remains blocked. A legacy lock
missing either the list or metadata is readable but readiness remains
incomplete until source evidence is refreshed. It must never be used as an
authentication cache.
Removal clears optional-workflow credential references and does not retain a
Meshy reference outside the selected `workflow.3d` entry. A
`workflow.super_events` entry has no credential reference because the component
has no credential or environment requirement.

### Project state

Project state records wizard progress, provider/model/profile, non-secret
endpoint and flatten preferences, App Server integration state when Codex is
selected, and opaque references for secrets such as `MESHY_API_KEY` or a
provider key. It can be recreated without changing installation ownership.

### Readiness

Readiness records whether the selected provider was configured (or ChatGPT
authentication was verified during a Codex setup), whether required analysis
was confirmed, whether account metadata stayed out of artifacts, and which
checks block core readiness. Open in Codex is a Codex-only action.

### Journal

The journal records transaction state only. Semantic analysis is complete before dry-run approval and is not rerun inside apply or rollback.

## Generated launcher artifacts

Generated `descriptor.mod`, the external `<project_id>.mod`, and `thumbnail.png` are first-class operations and lock rows. Every row records `location_scope`, generator source, installed hash, ownership, platform, and rollback behavior. The external descriptor receives backup and restoration like any other managed destination. `script_prefix` and `primary_namespace` are optional installation-plan and installation-lock metadata used by generated guidance and maintenance; they are not HOI4 descriptor fields. Legacy locks remain readable because both fields default to null.

New-project plans additionally record whether the root is an existing path or a
single `create_leaf`, the canonical parent, and the reviewed leaf name. A
`create_leaf` is created only at apply. The journal records whether this
transaction created it and whether rollback removed it or retained it because
unknown content remained.

## Credential references

All secrets use opaque OS-vault references. ChatGPT tokens remain fully owned
by Codex and have no project credential reference. Hosted-provider references
are deterministic, provider-scoped vault handles used only by the core and
never serialized into project state, a plan, or a lock.

## Evolution

Schemas use explicit versions. The 1.0 lock migration inserts an empty
`wiki_required_pages` marker for legacy locks; missing `wiki_metadata` is also
treated as incomplete rather than guessing from the current application
bundle. It also binds older confirmed analysis metadata to the lock's own
existing source identity when both values are valid. Add migrations before changing required fields. Unknown major versions
block.
Unknown minor fields can be ignored only when the schema permits them.

## Examples

Every schema has a realistic example. Examples must pass Draft 2020-12 validation in CI. Examples cover Codex and provider analysis metadata, launcher-ready generated files, external destinations, flattened source preferences, and account/key-data exclusion.

## Atomic writes

Write project state, plans, locks, readiness, and journals to a sibling temporary file, flush, atomically replace, and retain recovery evidence. Never expose a partial JSON document as current state.
