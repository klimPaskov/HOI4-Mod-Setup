# Merge and conflict rules

## Core rule

Never overwrite a user-modified file silently.

Every touched path is classified with:

- base: previous installed content from the lock
- local: current project content
- incoming: target repository content or generated output

## Classification matrix

| Base | Local | Incoming | Default result |
| --- | --- | --- | --- |
| absent | absent | present | create |
| known | equals base | changed | replace after dry-run approval |
| known | changed | equals base | keep local |
| known | equals incoming | changed from base | accept current result and refresh lock |
| known | changed | changed | conflict |
| unknown | present | present | user-owned conflict |

## Resolution options

### Keep

Leave local content. Record a local override or skipped path.

### Replace

Back up local content and install incoming.

### Merge

Build a three-way or structured preview and validate it before selection.

### Rename

Install incoming at a reviewed alternate path. Validate references or report that the file is review-only.

### Skip

Do not install. Skipping a required file can block component readiness.

## Text merge

Use diff3. Normalize line endings only for comparison and preserve the selected result style. Show base, local, incoming, merged preview, unresolved regions, and validation.

Do not write conflict markers into the live project unless the user explicitly exports an unresolved merge for manual work.

## AGENTS.md

Preserve project restrictions and valid local paths. Highlight foreign project names, stale absolute paths, references to missing skills or agents, unresolved template tokens, and security-sensitive settings. Validate final size against Codex project-document limits.

## TOML

Parse tables and keys. Show semantic changes for approval policy, sandbox mode, model settings, project-document limits, feature flags, and every MCP server field. A duplicate server ID with a different command is a conflict.

## JSON

Use schema-aware object merge. Arrays require a manifest-declared identity key. Unknown array semantics require manual review.

## Binary files

Offer keep, replace, rename incoming, or skip. No automatic text merge.

## Directory and link conflicts

A file-directory collision blocks until resolved. Show symlink or junction targets. Never replace through a link that leaves the allowed root.

## Bulk decisions

Apply to similar is allowed only when component, ownership, base state, local state, incoming action, and file type match. Show the exact affected count.

## Validation

- no unresolved text markers
- TOML and JSON parse
- no AGENTS template tokens
- destination remains inside root
- renamed references are handled
- result hash is recorded

## Persistence

Store choice and result hash in the lock. Reuse a prior choice only when base, local, and incoming conflict signatures are identical.
