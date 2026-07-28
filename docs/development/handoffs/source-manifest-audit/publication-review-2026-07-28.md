# Source-manifest publication review

Date: 2026-07-28

The live source path was explicitly supplied by the maintainer:

`C:\Users\klimp\OneDrive\Documents\Paradox Interactive\Hearts of Iron IV\mod\agentic_hoi4_modding`

The only live working-tree change was `hoi4-mod-setup.manifest.json`. It was
regenerated with `scripts/generate_manifest_evidence.py` from tracked bytes at
`27128a7b311d728a959afff7238a9aeeb9987f2b`, validated against
`schemas/remote-manifest.schema.json`, and checked with `git diff --check`.
It was committed and pushed to `Agentic-HOI4-Modding/main` as:

`54da3e7b311d728a959afff7238a9aeeb9987f2b chore(manifest): publish revision-bound setup manifest`

The manifest honestly retains `repository.license_evidence = not_found` and
wiki provenance/license states of `repository_only` / `not_found`; no upstream
license or provenance was invented.

## Publication-commit binding

The manifest's `generated_for_revision` is the tracked source snapshot used to
generate file evidence. It may precede the commit that publishes the manifest,
because changing a Git commit's manifest field changes the commit hash. The
runtime now requires that field to be an immutable commit-shaped value, but
binds the manifest bytes and every selected source blob to the one resolved
commit and verifies each declared size and SHA-256. A manifest regenerated from
an older snapshot therefore fails at selective download if any source byte has
changed.

The Rust test
`source::tests::published_manifest_is_consumed_at_the_resolved_revision` covers
remote consumption at a later publication commit. Existing bundled-bootstrap
locks remain readable, but the resolver does not substitute the bundle for a
newly resolved remote manifest.
