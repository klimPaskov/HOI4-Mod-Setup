# Installed source-packaging tool

The Agentic repository owns the optional, all-platform `runtime.chatgpt_sources`
component: the Python script and Windows batch launcher under `.tools/`, with
generated Git-blob evidence. No profile selects it; the user checks it on
Components. The shared `runtime.agent_sync` component no longer carries it.
Never embed a second app-owned copy or hand-maintain hashes. This tool uses
ordinary managed-file conflict handling, staging, rollback, and update
behavior; export is invoked separately by the user and never by the app.

Run the source repository's `scripts/test_chatgpt_sources_package.py` through
unittest discovery to verify manifest-selected installation and tiny-project
CLI output, archive bytes, configuration, dry-run, containment, overwrite
refusal, and links. After committing source, regenerate the canonical manifest
with `scripts/generate_manifest_evidence.py`, validate it, then synchronize the
app's bundled manifest and exact audit digests. Public publication remains the
source repository's existing workflow.

See [the app contract](../../../docs/33_chatgpt_source_packaging.md) and the
installed `.tools/README.md` for selection defaults and export boundaries.
