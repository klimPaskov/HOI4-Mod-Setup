# Project scanner and required Codex analysis

## Purpose and authority

The scanner creates an evidence-backed project profile without changing the selected project. It receives one explicit root. Companion paths outside the root are allowed only when user-confirmed, such as the launcher descriptor, vanilla game directory, or credential vault.

The scanner is deterministic Rust code. It is the sole authority for observable structural facts, including file existence, descriptor validity, paths, hashes, encodings, Git state, identifiers, namespaces, and conflicts. Read-only means no project writes, metadata normalization, Git index operations, package installation, launcher changes, or commands that create project caches.

## Two-layer analysis contract

The Rust scanner produces observable facts. The Codex App Server produces semantic proposals from approved facts and text excerpts.

The deterministic layer owns file inventory, descriptors, launcher registration, thumbnail decoding, Git state, identifier indexes, encoding, component inventory, conflicts, and platform support.

The Codex layer owns project purpose, normalized description, display name and ID proposals, prefix and namespace proposals, folder profile, `AGENTS.md` adaptation direction, component recommendations, convention interpretation, and concise conflict explanation.

The Codex layer uses ChatGPT-managed authentication, a user-reviewed input manifest, a read-only turn, and `schemas/codex-analysis.schema.json`. It has no write access.

## Deterministic scan phases

### 1. Filesystem boundary

Detect canonical root, case sensitivity, future write capability, links, large directories, descriptors, and approved external destinations. Do not follow a symlink or junction outside the root.

### 2. Descriptors, launcher registration, and thumbnail

Parse the internal `descriptor.mod` and approved external launcher descriptors with a dedicated key-value parser. Detect name, path, supported version, version, tags, picture, duplicate keys, invalid quoting, mismatch, missing files, duplicate launcher registrations, and multiple descriptors pointing to the same project.

Resolve the referenced thumbnail inside the project. Decode it with a memory-safe image library, record dimensions and color mode, hash it, and classify it as missing, valid, invalid, managed placeholder, or user-modified.

### 3. Folder structure

Create a normalized tree summary, standard HOI4 surface list, counts, and representative files. Missing optional folders are not defects.

### 4. Git

Use bounded read-only Git commands (`--no-optional-locks`) to detect repository root, branch, detached state, commit, modified, staged, untracked, remotes, submodules, hook names, ignore files, tracked secret-like path names, sparse checkout, worktrees, and ignore behavior. A linked worktree whose gitdir is outside the approved root is reported without following it; unavailable or partial probes stay visible as review evidence.

### 5. IDs and namespaces

Index event namespaces, focus IDs, decision IDs, scripted effects and triggers, ideas, characters, country and cosmetic tags, technologies, localisation keys, sprite names, and file prefixes. Return frequency, coverage, exceptions, and confidence.

### 6. Naming patterns

Detect lowercase snake_case, feature folders, ID and slug patterns, prefix placement, localisation filenames, docs folders, asset folders, plans, and handoff conventions.

### 7. Localisation

For every file record encoding, BOM, language header, duplicate keys, prefix distribution, filename pattern, line endings, and parse errors. Setup reports these states and does not repair them silently.

### 8. Documentation and instructions

Find AGENTS, READMEs, source-of-truth statements, specs, plans, manifests, absolute paths, foreign project names, and references to missing skills or agents.

### 9. Skills

Parse frontmatter, description, companion skills, helper files, scripts, asset directories, commands, absolute paths, project-specific tokens, and MCP operation references. Detect incoming ID collisions.

### 10. Subagents

Parse name, description, model settings, sandbox mode, instructions, referenced skills, ownership, fork-context rules, duplicate names, and project-specific paths.

### 11. Codex and MCP configuration

Parse TOML structurally. Detect approval policy, sandbox mode, document size, feature flags, server IDs, command, args, cwd, environment, timeouts, duplicate IDs, and platform-specific executable suffixes.

### 12. Conflict synthesis

Combine scan evidence with the selected remote manifest. Produce path, namespace, platform, dependency, ownership, generated-destination, launcher-registration, thumbnail, and Git risk conflicts.

## Required Codex semantic review

The Codex pass runs only after a completed deterministic scan or after a user enters a new-project brief. It is a separate analysis layer and never a scanner phase.

### AI used

The application uses the official local `codex app-server` and the user’s ChatGPT-managed Codex account. It does not expose a provider selector, hardcode a model, ship an application-owned AI credential, or require an API key.

### Consent and input preview

Before invocation, show a request manifest containing every scan field and text excerpt that would be provided. The user can remove entries or cancel. Allowed inputs include normalized scan JSON, the mod description, selected descriptor and README excerpts, AGENTS and skill frontmatter, TOML excerpts, localisation headers, representative naming samples, and incoming component metadata.

Exclude binaries, secrets, credential stores, `.git/objects`, files outside approved roots, ignored secret files, and any content the user deselects.

### Allowed suggestions

Codex may suggest:

- project purpose and scope
- primary namespace or prefix candidate
- relevant component and skill selection
- AGENTS adaptation choices
- naming and localisation conventions
- suitable initial folder profile
- project-specific paths that require review
- conflicts or ambiguities that require human judgment

Codex cannot override deterministic facts, create installation operations, write files, select conflict resolutions, or mark readiness checks passed.

### Result states

- `detected`: deterministic and evidence-backed
- `codex_suggested`: semantic inference awaiting review
- `confirmed`: accepted or edited by the user

Codex output must validate against a strict response schema. Store only non-secret audit metadata, approved input field or path names, model identifier when reported by App Server, response hash, validated suggestions, timestamps, and user decisions. Do not request, expose, or store hidden chain-of-thought.

Codex missing, signed out, usage-limited, cancelled, failed, or malformed-response states block creation of a new installation plan. They preserve the deterministic scan and local draft. Recovery, rollback, backup inspection, and managed removal remain available.

## Evidence model

```json
{
  "id": "namespace.primary",
  "category": "namespace",
  "key": "primary_namespace",
  "value": "cwc",
  "origin": "deterministic",
  "status": "needs_review",
  "evidence": [
    {
      "detector": "identifier_frequency",
      "path": "events/",
      "confidence": 0.91,
      "note": "91 percent of custom identifiers use cwc"
    }
  ]
}
```

Evidence excerpts are hashed. Large result sets store counts plus a separate evidence file.

## Confidence

- 1.00: explicit user selection or exact descriptor field
- 0.90 to 0.99: strong repeated deterministic pattern
- 0.70 to 0.89: plausible deterministic pattern requiring review
- below 0.70: suggestion only

Codex suggestion confidence is stored separately and never increases deterministic confidence.

## Incremental cache

Cache file metadata and hashes in application data. Before installation, recompute every touched local hash. Cache is advisory and cannot prove conflict state.

## Large repositories

Use bounded concurrency, exclude `.git/objects` and known caches, stream phase and path progress, support backpressure, keep temporary evidence outside the project, and cancel promptly. The desktop command assigns each scan an opaque request ID; every `scan-progress` event carries that ID plus the current stage, relative path, file and directory counters, and bytes read. The UI must ignore events for another request and must not invent a percentage when no total is known.

Cancellation is cooperative and checked before directory traversal, between entries, and before detector work. A cancelled scan returns `partial: true` and `cancelled: true`, emits a terminal `cancelled` progress event, and clears the in-memory approved-evidence binding so a stopped scan cannot be used for Codex analysis. Any other partial result also clears that binding and blocks semantic analysis until an untruncated scan completes. Limits remain visible through `limits_hit` and the counters; they are never presented as an unqualified full scan.

## Required fixtures

- empty new project
- normal existing mod
- local AGENTS rules
- mixed localisation encodings
- multiple namespaces
- nested worktree
- cloud-synced root
- link escape
- locally modified wiki
- skill name collision
- MCP ID collision
- macOS project with Windows-only incoming component
- valid descriptors with a missing thumbnail
- mismatched external launcher path
- duplicate launcher descriptor for the same project root
- invalid and user-modified thumbnails
- ChatGPT sign-out, login cancellation, device-code fallback, usage limits, App Server interruption, malformed response, and schema-valid suggestions
- proof that Codex suggestions cannot replace deterministic findings

## App Server session data

The scan result may store analysis ID, input digest, output digest, proposal keys, model identifier when reported, and confirmation state. It must not store account email, account ID, ChatGPT plan, rate-limit details, tokens, raw thread history, or hidden reasoning.
