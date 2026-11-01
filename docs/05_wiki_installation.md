# Offline wiki installation and update design

## Verified distribution

The live repository stores the kit's HOI4 wiki (pages derived from the Hearts
of Iron IV Wiki's modding pages, plus the kit's own pages and corrections) as a
`hoi4_wiki/` tree containing Markdown pages and a `media/` directory. Kit
manifests before the rename declared the same tree as `paradox_wiki/`. The
current published manifest declares `snapshot_marker: null`; do not require or
invent a marker file.

No formal wiki source manifest or license file was verified at the inspected revision. The application must display that limitation and must not invent a source, official status, or license.

## Destination

```text
<mod_project>/hoi4_wiki/
```

A manifest may declare `hoi4_wiki/` or, for older kit revisions, `paradox_wiki/`.
No other destination is accepted, and the wiki component's destination must
equal the manifest's `wiki.destination`. The destination is the default because the project instructions and skills refer to it. The user can review the path, but moving it requires corresponding project-instruction adaptation.

## Installation

1. Expand only the manifest's wiki folder (`hoi4_wiki/`) at the selected commit.
2. Reject entries outside that subtree.
3. Download Markdown and media files.
4. Verify SHA-256 for every file.
5. Build a complete staging tree.
6. Check required page coverage.
7. Check declared media policy.
8. Apply transactionally.
9. Record every installed file in the lock.

A future release bundle is allowed only when the remote manifest declares its URL, hash, internal index, and extraction rules. The application does not invent an archive source.

## Required core page coverage

The remote manifest should require exact filenames for at least:

- Data structures
- Triggers
- Effects
- Modifiers
- Localisation
- Scopes
- On actions
- Event modding
- Decision modding
- Idea modding
- AI modding

Selected skills can add page requirements. Examples:

- focus work adds National focus modding
- GUI work adds Interface modding and Scripted GUI Modding
- 3D work adds Graphical asset modding and Entity modding
- country creation adds Country creation and State modding
- technology work adds Technology modding

The plan and lock carry the exact `manifest.wiki.required_pages` list for the
resolved source revision. Readiness shows each page, declaring component, path,
hash, and status; it does not substitute the application's current bundled
manifest for a pinned install. A legacy lock without this evidence remains
readable but is incomplete until update or repair refreshes the source evidence.

## Media policy

The manifest chooses one:

- `all_declared`: every media file in the resolved component index is required
- `referenced_only`: only media referenced by installed Markdown is required
- `none`: media is intentionally excluded

For the current repository tree, `all_declared` is the safest first policy because later agent work may need pages and images not referenced by the initial core subset.

## Link validation

Check that relative links remain within the wiki folder, referenced media exists, path case matches, and no unresolved extraction path appears. Broken external links are warnings because the snapshot is intended to work offline.

## Provenance display

Show repository, commit, optional marker state, source metadata state, and
license metadata state. At the inspected revision:

- source status: repository-only
- license status: not found
- snapshot marker: not declared

These are evidence states, not legal conclusions.

## Update

1. Compare old and target indexes.
2. Replace unmodified managed files.
3. identify local modifications.
4. Show added and removed pages.
5. Show required-coverage changes.
6. Preserve locally added files.
7. Require a choice for modified managed pages.

A modified page can use three-way merge when a previous base exists. The safer default is keep local or install incoming under a renamed review path.

## Repair and removal

Repair restores missing or corrupted unmodified files at the locked revision. Modified files remain untouched until review. Removal deletes only unmodified managed files, preserves local additions and modifications, then removes empty component-owned directories.

## Performance

Use bounded downloads, streaming hashes, immutable cache reuse, same-volume staging when possible, and aggregate plus per-file progress.

## Readiness

The wiki is blocking when selected by the core profile. Missing required pages or failed hashes block Open in Codex. Unverified provenance or license metadata remains a visible warning rather than an integrity failure.

## Folder rename migration

The kit renamed `paradox_wiki/` to `hoi4_wiki/`. Setup reads both names
everywhere (scan exclusions, readiness, classification, the managed
`.gitignore` block). An update or repair against a manifest that declares
`hoi4_wiki/` migrates a project that the lock records under `paradox_wiki/`
with the ordinary obsolete-file rules, so no new mechanism is involved:

- every page of the new folder is created;
- every old page that still holds exactly the bytes the lock recorded is
  removed (`obsolete_managed_remove`), and the old folder and its subfolders
  are removed once they are empty;
- an old page the user edited is never deleted or overwritten. It stays where
  it is and the review lists it as a conflict (`obsolete_review`) for the user
  to keep or discard;
- readiness and the readiness report read `hoi4_wiki/` when it exists, and fall
  back to `paradox_wiki/` for a project that has not been updated yet;
- the managed `.gitignore` block lists both folders, and lines the user wrote
  outside the block are unchanged.
