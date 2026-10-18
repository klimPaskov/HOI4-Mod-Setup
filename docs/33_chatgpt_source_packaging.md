# ChatGPT source packaging

Accepted product decision (2026-10-18): HOI4 Mod Setup does not package
ChatGPT project sources. It has no packaging screen, export command, Review or
Update callout, or flattened-folder option, and new installation plans never
prepare a `chatgpt_project_sources/` folder. This supersedes the earlier
external-export tool, the Components flattening option, and the install-review
check described in the user flows, UI requirements, provider/source
requirements, and acceptance criteria.

## The optional packager component

The Agentic repository publishes the packager as the optional, all-platform
`runtime.chatgpt_sources` manifest component:
`.tools/package_chatgpt_project_sources.py` and its Windows `.bat` launcher,
with source-owned exact Git-blob size and SHA-256 evidence. No profile selects
it. The Components screen lists it like any other optional manifest
component, unchecked by default, and installs it only when the user checks it.
It uses ordinary managed-file conflict handling, staging, rollback, update, and
removal. Setup never runs it; the user runs it from the project when they want
an export. The shared `runtime.agent_sync` component no longer carries it.

Earlier manifests shipped the script inside `runtime.agent_sync`. An update to a
manifest with the optional component removes an unchanged script unless the
user checks the new component; a modified script follows normal review.

## Older installs

A lock written before this decision may record `flatten_chat_sources: true` and
generated `chatgpt_project_sources/` files. Maintenance keeps that recorded
folder as before, and readiness still reports its legacy check, until a
separate migration retires it. No new plan sets the flag.

## Boundaries

The packager uses no provider login, model, or credentials, performs no upload,
and writes only to a new folder outside the project. Its selection defaults and
export rules are documented in the installed `.tools/README.md`.
