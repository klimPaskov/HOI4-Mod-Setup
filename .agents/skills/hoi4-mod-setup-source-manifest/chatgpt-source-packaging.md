# Installed source-packaging tool

The Agentic repository owns the all-platform `runtime.agent_sync` shared-tools
component. Both core profiles and coding environments include it. Alongside its
existing synchronizers, selectively install only its declared `.tools/README.md`, Python script, and Windows batch
launcher, using generated Git-blob evidence. Never embed a second app-owned copy
or hand-maintain hashes. This tool uses ordinary managed-file conflict handling,
staging, rollback, and update behavior; export is invoked separately by the user.

Run the source repository's `scripts/test_chatgpt_sources_package.py` through
unittest discovery to verify manifest-selected installation and tiny-project
CLI output, archive bytes, configuration, dry-run, containment, overwrite
refusal, and links. After committing source, regenerate the canonical manifest
with `scripts/generate_manifest_evidence.py`, validate it, then synchronize the
app's bundled manifest and exact audit digests. Public publication remains the
source repository's existing workflow.

See [the app contract](../../../docs/33_chatgpt_source_packaging.md) and the
installed `.tools/README.md` for selection defaults and export boundaries.
