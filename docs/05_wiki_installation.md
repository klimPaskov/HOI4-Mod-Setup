# Offline wiki installation and update design

## Verified distribution

The live repository stores the offline Hearts of Iron IV Paradox wiki as a `paradox_wiki/` tree containing Markdown pages, a `media/` directory, and an observed marker named `_last_updated_on_27_Nov_2025.txt`.

No formal wiki source manifest or license file was verified at the inspected revision. The application must display that limitation and must not invent a source, official status, or license.

## Destination

```text
<mod_project>/paradox_wiki/
```

The destination is the default because the project instructions and skills refer to it. The user can review the path, but moving it requires corresponding project-instruction adaptation.

## Installation

1. Expand only `paradox_wiki/` at the selected commit.
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

Check that relative links remain within `paradox_wiki/`, referenced media exists, path case matches, and no unresolved extraction path appears. Broken external links are warnings because the snapshot is intended to work offline.

## Provenance display

Show repository, commit, marker filename, source metadata state, and license metadata state. At the inspected revision:

- source status: repository-only
- license status: not found

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
