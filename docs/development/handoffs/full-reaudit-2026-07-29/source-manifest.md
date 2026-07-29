# Source manifest audit — full re-audit 2026-07-29

## Outcome

Current HEAD is **not source-contract complete**. The exact-revision and
selective-download implementation is materially present, and the checked-in
manifest is internally coherent, but two high-severity contract failures remain:

1. the runtime does not validate the remote manifest against the authoritative
   JSON Schema and therefore does not enforce all provenance, update-policy, or
   signing declarations; and
2. installed readiness reads provenance from the current bundled manifest
   instead of the exact locked manifest and does not report the locked wiki
   license state.

No critical finding was identified. This audit records 2 high, 6 medium, and 4
low findings. Completion of this report means the parent has an
evidence-backed audit; it does not mean the source system is complete.

## Scope and source revision

- Repository: HOI4 Mod Setup only.
- Audited HEAD:
  `bcfe329dd9ab0ae0d86e48b1a46ed21c83e36603`
  (`codex/final-state`).
- Working tree before audit: clean.
- Audit mode: bounded read-only review. The only written file is this handoff.
- Upstream state was not queried over the network. Public source revisions
  below are repository-contained evidence, not a fresh claim about current
  GitHub state.
- Bundled/example manifest:
  - `generated_for_revision`:
    `27128a7b311d728a959afff7238a9aeeb9987f2b`
    (`docs/source-manifest/hoi4-mod-setup.manifest.json:4`);
  - SHA-256:
    `cddb7ece7235d033888d85508455c255ffe320f0f28bc924999e8f4ddd1c19b5`;
  - the repository-manifest example and bundled manifest are byte-identical.
- Repository-contained publication evidence says the public manifest was later
  observed at commit `54da3e7a43cce43f15edc54ef80fb0099822b3e2`
  with raw SHA-256
  `0e8db882f4ae61f7b030b415d4a575f643fd1a5c5dc475f7a0dcccc6933bd3ba`
  (`docs/00_source_audit.md:37-44`). This audit did not independently
  re-resolve that remote state.

## Audited files and tests

Governing and design sources:

- `AGENTS.md`
- `.agents/skills/hoi4-mod-setup-source-manifest/SKILL.md`
- `docs/GOAL_PROMPT.md`
- `docs/04_remote_repository_manifest.md`
- `docs/05_wiki_installation.md`
- `docs/09_component_dependency_model.md`
- `docs/11_mcp_setup.md`
- directly relevant evidence in `docs/00_source_audit.md` and
  `docs/source-audit/live_repository_inventory.json`

Schemas, examples, and manifests:

- `docs/schemas/remote-manifest.schema.json`
- `docs/examples/repository-manifest.example.json`
- `docs/source-manifest/hoi4-mod-setup.manifest.json`
- `docs/schemas/installation-plan.schema.json`
- `docs/examples/installation-plan.example.json`
- `docs/schemas/installation-lock.schema.json`
- `docs/examples/installation-lock.example.json`
- `CHECKSUMS.sha256`

Runtime and supporting evidence:

- `src-tauri/src/source.rs`
- `src-tauri/src/mcp.rs`
- relevant sections of `src-tauri/src/commands.rs`, `models.rs`,
  `security.rs`, `readiness.rs`, `transaction.rs`, and `codex.rs`
- `scripts/generate_manifest_evidence.py`
- `scripts/validate_repository_templates.py`

Inline tests inspected:

- `source.rs:1469-1720`
- `mcp.rs:391-456`
- `security.rs:544-601`
- relevant wiki/dependency readiness tests at
  `readiness.rs:1304-1448`
- relevant plan/example tests at `commands.rs:4523-5356`
- relevant plan/lock and fault tests in `transaction.rs`

Validation executed:

- `python scripts/validate_repository_templates.py` — **pass**:
  “Validated 12 integrity groups for full planning package.”
- All named schema/example hashes recorded in `CHECKSUMS.sha256` matched.
  The bundled source-manifest path itself has no checksum entry; its
  byte-identical example does.
- No Cargo test command was run because this auditor was restricted to writing
  only the requested report. Rust tests were inspected statically; the parent
  should rerun them.

## Verified behavior

### Exact default-branch and pinned resolution

- Latest reads GitHub repository metadata, extracts `default_branch`, resolves
  that branch to `/commit/sha`, validates the 40-character SHA, and verifies
  the Git commit object before returning it
  (`HttpSourceClient::resolve_commit`, `source.rs:188-214`).
- Pinned commit requires a supplied full SHA and verifies it as a Git commit
  object (`source.rs:215-222`).
- Pinned release resolves the GitHub release tag, resolves the typed Git ref,
  dereferences up to four annotated tags, requires a commit object, and returns
  both canonical tag text and exact commit (`source.rs:223-292`).
- `resolve_source` fetches the manifest only after commit resolution and stores
  repository, mode, requested ref, canonical release, resolved revision,
  manifest hash, and origin in `SourceIdentity`
  (`source.rs:673-697`; `models.rs:341-353`).
- Maintenance rejects a pinned release whose tag later resolves to a different
  commit (`commands.rs:3511-3527`).

### One revision and selective files

- Manifest, recursive tree metadata, and selected blobs are all addressed by
  `resolution.identity.resolved_revision`
  (`commands.rs:2424-2459`, `2470-2487`).
- The runtime fetches the full Git tree as metadata, not a repository clone,
  then expands only selected supported components and downloads only the
  resulting selected blobs (`source.rs:307-349`, `1238-1328`).
- Non-generated selected files require lowercase SHA-256 and byte-size
  evidence (`source.rs:829-860`, `1359-1371`).
- Every downloaded file is verified against both SHA-256 and declared size
  before becoming a plan operation (`source.rs:1389-1428`;
  `commands.rs:2470-2564`).
- No application path performs `git clone` or searches for an Agentic HOI4
  Modding checkout. The evidence generator requires an explicit
  `--source-root`; it does not discover one.

### Redirect, host, size, and cache controls

- Only HTTPS `api.github.com` and `raw.githubusercontent.com` URLs without
  userinfo or custom ports are approved (`approved_source_url`,
  `source.rs:611-620`).
- Redirects are host-checked on each hop and capped
  (`source.rs:88-100`); final response URLs are checked again
  (`source.rs:142-147`, `524-529`).
- General responses are bounded by declared/content/read limits
  (`source.rs:112-159`). Tree responses fail on GitHub truncation and over
  100,000 entries (`source.rs:307-327`). Selected payloads are capped at
  20,000 files, 1 GiB aggregate, and 512 MiB per source file
  (`source.rs:27-29`, `1313-1327`).
- Verified blob cache entries are addressed by
  `blobs/<revision>/<sha256>` and are accepted only after size and SHA-256
  revalidation (`source.rs:383-446`).
- Interrupted reads retain bounded `.part` bytes, resume only after a matching
  `Content-Range`, and promote only after complete size/hash verification
  (`source.rs:449-592`).

### Current checked-in manifest

The manifest declares 10 components and 917 expected files totaling 13,271,111
bytes, below current aggregate limits. Static expansion found no exact or
NFC-plus-lowercase duplicate expected-file paths.

The current wiki component contains 586 declared files: 50 Markdown files and
536 media files. All 11 `wiki.required_pages` entries have exact expected-file
evidence, and every declared wiki expected path is under `paradox_wiki/`.

The current MCP declaration is honest about its limits:

- optional and Windows-only
  (`hoi4-mod-setup.manifest.json:1846-1853`);
- command target `hoi4-agent-tools.cmd`
  (`hoi4-mod-setup.manifest.json:1891-1897`);
- no executable, `cmd.exe`, or Node hash/size identity
  (`hoi4-mod-setup.manifest.json:1891-1915`);
- `manifest_target` fails with `UnsupportedPlatform` before any PATH
  resolution when immutable identities are missing
  (`mcp.rs:70-133`);
- actual process start requires wrapper, interpreter, and runtime hashes and
  sizes and rechecks all three before spawn (`mcp.rs:217-283`);
- macOS receives an explicit unsupported state rather than a fabricated
  command (`source.rs:1209-1233`, test at `1557-1579`).

## Findings by severity

### High H-01 — Runtime remote-manifest validation does not enforce the authoritative JSON Schema

`parse_manifest` deserializes to Rust and calls a handwritten
`validate_manifest`; it never evaluates
`docs/schemas/remote-manifest.schema.json`
(`source.rs:713-717`, `742-1043`). No JSON Schema validator is wired into the
Rust source path.

The handwritten validator covers many important invariants, but not all schema
contracts:

- schema-controlled wiki provenance values are enums
  (`remote-manifest.schema.json:515-535`), while runtime
  `WikiProvenance` stores unrestricted strings (`models.rs:264-270`) and
  `validate_manifest` never validates `source_status` or `license_status`;
- schema requires `latest.resolve_default_branch` and `record_commit` to be
  `true` (`remote-manifest.schema.json:555-568`), but runtime validates only
  positive rollback retention (`source.rs:1038-1041`);
- `pinned.allow_commit` and `allow_release` are not consulted by the resolver;
- `signing.required`, algorithm, and key ID are deserialized but never enforced
  (`remote-manifest.schema.json:595-614`;
  `models.rs:314-338`).

The checked-in manifest is schema-valid, so this is not a claim that its current
values are malformed. The failure is at the remote trust boundary: a future or
hostile schema-invalid manifest can be accepted and can place unapproved
provenance/license strings into the plan and lock, declare unsupported policy,
or request signing without causing fail-closed behavior.

Tests present: checked-in and example manifests parse, unsupported major is
implemented, and malformed evidence is partly covered. Tests missing:
schema-invalid/runtime-accepted provenance, update-policy constants, signing
required, and differential JSON-Schema-versus-Rust validation.

Recommended parent action: wire Draft 2020-12 validation into remote manifest
parsing or make the Rust validator provably equivalent, then explicitly reject
unsupported signing and policy combinations.

### High H-02 — Readiness substitutes bundled provenance for locked provenance and omits the locked wiki license state

The plan and lock correctly copy exact-manifest wiki metadata
(`commands.rs:2880-2881`; `transaction.rs:2551-2552`). Readiness then does not
use those values:

- `manifest_provenance_statuses` deserializes the current bundled manifest
  (`readiness.rs:24-37`);
- `evaluate` uses those bundled values for `source.license_metadata` and
  `wiki.provenance` (`readiness.rs:345-365`);
- `project_input` only checks that `wiki_metadata` exists, not its snapshot,
  media, source, or license values (`readiness.rs:1126-1129`);
- no readiness check displays `lock.wiki_metadata.license_status`.

This directly violates the exact-revision requirement for a pinned install.
An older pinned lock can display a newer bundled source/provenance state, and
the wiki license state is not surfaced distinctly at all. Current bundled
values happen to be `repository_only` / `not_found`, but that coincidence does
not satisfy pinned reproducibility.

Tests missing: a pinned lock whose wiki metadata differs from the bundle, a
legacy lock with missing metadata, and explicit wiki-license evidence in the
readiness report.

Recommended parent action: carry the lock’s full `WikiInstallMetadata` and
repository license state into `ReadinessInput`, render those exact values, and
reserve bundled metadata for an explicitly identified bootstrap lock only.

### Medium M-01 — Platform support is not propagated through dependency edges

`resolve_platform_support` evaluates only each component’s own platform list
(`source.rs:1179-1235`). It does not mark a nominally supported dependent as
blocked when one of its expanded dependencies is unsupported. Therefore a
future required `all` component depending on an optional Windows-only component
can be marked supported on macOS while its dependency is omitted from
`download_selected`.

The current graph does not trigger this condition: the optional Windows MCP and
3D components depend on platform-neutral components, not the reverse. The
current macOS MCP test therefore passes but does not cover dependency-state
propagation (`source.rs:1557-1579`).

Recommended parent action: resolve platform states in topological order and
propagate dependency failures, while keeping a truly leaf optional component
non-blocking.

### Medium M-02 — Reverse-dependency evidence is computed but not exposed for update/removal review

`reverse_dependencies` is deterministic and tested
(`source.rs:1155-1177`, `1589-1601`), but its data is only attached to the
internal `ComponentSupport`. `SourceManifestPreview` returns source identity,
repository, and components, but no support or reverse-dependency map
(`commands.rs:83-90`). `InstallationPlan` and `InstallationLock` also have no
reverse-impact evidence (`models.rs:674-723`, `790-827`).

The implementation can expand forward dependencies, but the required
user-visible “what depends on this?” evidence is not available to update or
removal review.

Recommended parent action: add a deterministic, source-revision-bound impact
record to preview/plan review, and test direct plus transitive removal impact.

### Medium M-03 — Size/platform evidence is produced by new plans but is optional in schemas and readiness

New planning code carries source size and platform
(`commands.rs:2562-2564`, `2812-2820`) and lock construction carries source
and installed sizes (`transaction.rs:2351-2372`). The persisted contracts do
not require them:

- plan operation `source_size` and `platform` are optional
  (`installation-plan.schema.json:459-491`);
- lock file `source_size` and `installed_size` are optional
  (`installation-lock.schema.json:415-445`);
- readiness treats a missing installed size as valid and never validates
  `source_size` (`readiness.rs:1015-1028`).

Consequently a schema-valid imported, migrated, or manually altered lock can
lose size evidence and still pass source readiness if hashes match. SHA-256
still protects content, so this is not a checksum bypass, but it violates the
plan/lock evidence contract and weakens size-limit reproducibility.

Recommended parent action: require sizes for new remote/generated records,
retain an explicit legacy-incomplete migration state, and make readiness warn
or block when selected source evidence lacks required sizes.

### Medium M-04 — Plan and lock examples are schema-valid but not runtime-representative

The package validator checks examples only against JSON Schema
(`scripts/validate_repository_templates.py:91-103`). Runtime coverage exists
for the repository-manifest example (`source.rs:1543-1554`), but not for the
installation-plan example and only incidental deserialization coverage exists
for the lock example.

Concrete drift:

- the example MCP action ID is `ext-001`
  (`installation-plan.example.json:153-170`), while
  `reviewed_plan_target` requires
  `external.mcp.hoi4_agent_tools.mcp.hoi4.health`
  (`mcp.rs:138-156`) and runtime generation uses that exact form
  (`commands.rs:1766-1769`);
- because the ID mismatch yields a source error, transaction readiness maps the
  example route to `block`, not the intended `planned_unavailable`
  (`transaction.rs:2009-2015`);
- the plan example omits source sizes/platforms from several operations even
  though runtime generation supplies them;
- generated files in the lock example use `source_revision: "generator-v1"`
  (`installation-lock.example.json:74-103`), but installed readiness requires
  every locked file’s source revision to equal the lock source commit
  (`readiness.rs:1023-1027`), and current lock construction writes that commit
  (`transaction.rs:2293-2303`, `2351-2358`);
- the lock example claims installed components whose full selected file
  coverage is not represented.

Recommended parent action: regenerate examples from runtime-owned fixtures,
add Rust parse-plus-`validate_plan` tests, and add an installed-readiness
fixture that proves a produced lock is accepted.

### Medium M-05 — Wiki marker evidence is lost, and non-`all_declared` media policies are accepted without implementation

Repository evidence says the wiki tree has the marker
`_last_updated_on_27_Nov_2025.txt`
(`docs/05_wiki_installation.md:5`;
`docs/source-audit/live_repository_inventory.json:111`). The current manifest
sets `snapshot_marker` to null (`hoi4-mod-setup.manifest.json:5200-5203`) and
its 586 expected wiki files contain no marker entry. The runtime therefore
cannot install, verify, lock, or display that observed marker.

The runtime validates only that the media-policy string is one of
`all_declared`, `referenced_only`, or `none`
(`source.rs:1004-1008`). Selection is controlled solely by component
include/exclude globs (`source.rs:1261-1299`); no code implements
referenced-media expansion or media exclusion. Current
`all_declared` plus `include: ["**"]` is coherent and all 536 media files are
hash-declared, but future `referenced_only` or `none` manifests would be
silently treated as full-tree installs unless their globs happened to compensate.

Recommended parent action: either implement each policy and marker coverage or
reject unsupported policies; reconcile whether the observed marker is tracked
at the generation revision without inventing provenance.

### Medium M-06 — Required hostile-source and resolution tests are largely absent

`HttpSourceClient` is a concrete real-network client with private endpoints and
no fake source adapter (`source.rs:77-109`). Inline tests cover helpers and
manifest logic, not HTTP sequences.

Missing meaningful tests include:

- repository default branch to exact commit, including a default-branch change
  between requests;
- pinned commit object rejection, lightweight release tags, annotated release
  tags, non-commit tag targets, and moved release tags;
- publication-commit manifest binding through a fake HTTP adapter (the current
  `published_manifest_is_consumed_at_the_resolved_revision` test supplies local
  bytes only at `source.rs:1525-1540`);
- allowed/denied redirect hosts, redirect count, final host, response body
  limits, content-length lies, and truncated trees;
- selected-file-only fetches, duplicate mapped destinations, file/aggregate
  limits, checksum mismatch, and size mismatch;
- corrupted immutable cache, interrupted body, valid/invalid range resume, and
  partial-file promotion;
- required wiki page failure, marker coverage, media-policy behavior, case
  mismatch, and unsafe links;
- dependency/platform propagation from M-01;
- runtime plan/lock example validity.

The source-manifest skill explicitly requires these hostile cases
(`SKILL.md:87-98`).

Recommended parent action: introduce a fake GitHub/source transport and make
the full latest/pinned/cache/redirect/wiki matrix a source release gate.

### Low L-01 — Collision keys use Unicode lowercase, not full Unicode case folding

`canonical_relative_key` performs NFC normalization followed by
`to_lowercase()` (`security.rs:101-105`). The accepted contract says Unicode
NFC plus case folding. Lowercasing is not a full Unicode case-fold operation,
so some case-equivalent names can retain different collision keys on
case-insensitive filesystems.

The existing test checks composed versus decomposed accents only
(`security.rs:582-590`); it does not cover case-fold-specific pairs or native
Windows/macOS collision behavior.

Recommended parent action: use a defined Unicode case-fold implementation and
add native collision fixtures on both supported platforms.

### Low L-02 — MCP health discards tool identities, so declared capabilities cannot be reconciled

The manifest declares six capabilities
(`hoi4-mod-setup.manifest.json:1904-1910`). MCP health validates each
`tools/list` name but returns only `tool_count`
(`mcp.rs:25-31`, `331-353`). It cannot show which live tools support the
declared focus/event/GUI/map/technology/probability capabilities or detect a
missing named viewer.

The current route remains `planned_unavailable`, so this does not create a
false healthy state today. It is a completion gap for any future executable
identity evidence.

Recommended parent action: retain a bounded sorted tool-name/capability record
and compare it to manifest declarations without calling tools.

### Low L-03 — Pinned release support is tag-to-commit only; release assets and stronger release identity are unsupported

The resolver correctly records canonical tag text plus exact commit. It does
not record a GitHub release object ID, asset identity, asset digest, or release
manifest. Plan/lock schemas also do not conditionally require `release` when
`mode == pinned_release` (`installation-plan.schema.json:86-133`;
`installation-lock.schema.json:78-125`), and `validate_plan` does not enforce
mode/ref/release field consistency.

Commit-pinned bytes remain reproducible, and maintenance detects a moved tag.
The unsupported portion is immutable release-asset installation and a
schema-enforced release identity.

Recommended parent action: keep the current tag-to-commit route explicit; add
release-object/asset fields only when repository evidence and tests exist.

### Low L-04 — Cache/checksum metadata contains dead or incomplete evidence

- `manifest_cache_ttl_seconds: 900` is declared
  (`hoi4-mod-setup.manifest.json:5234-5235`) but not read by runtime;
  only immutable blob caching is implemented.
- `cache_key` (`source.rs:1460-1467`) is unused; the actual cache layout is
  constructed directly in `fetch_verified_file`.
- `CHECKSUMS.sha256` records the byte-identical repository-manifest example
  (`CHECKSUMS.sha256:88`) but has no entry for
  `docs/source-manifest/hoi4-mod-setup.manifest.json`.

These do not invalidate the actual revision/hash-addressed blob cache, but they
create avoidable ambiguity in maintenance and repository integrity evidence.

## Reproducibility and integrity gaps

- Current installs bind remote manifest bytes and selected blobs to one exact
  resolved commit and record per-file SHA-256/size in runtime-generated
  plan/lock records.
- Runtime provenance is weaker than byte integrity: the
  `generated_for_revision` value is checked only as a commit-shaped string
  (`source.rs:784-795`), not verified as an existing source commit or ancestor.
  Selected-byte verification still fails closed on stale hashes.
- New lock creation records size, but persisted schema/readiness allows size
  evidence to disappear (M-03).
- Pinned readiness can display the wrong provenance/license evidence (H-02).
- The bundled manifest is byte-identical to the example and internally valid,
  but its direct path is omitted from repository checksums (L-04).
- No current runtime archive extraction exists, so archive integrity is neither
  implemented nor falsely claimed. Archive traversal and extraction-limit
  tests are correspondingly missing.

## Unsupported or uncertain routes

- **MCP Windows:** configuration declaration supported; executable health route
  is `planned_unavailable` because wrapper, `cmd.exe`, and Node immutable
  identity evidence is absent.
- **MCP macOS:** explicitly unsupported; no substitute command exists.
- **MCP package/install source/version:** not declared with sufficient immutable
  evidence and must not be invented.
- **Pinned release assets:** unsupported; current release mode resolves a tag
  to a commit.
- **Archives/bundles:** unsupported by the schema (`source.kind` permits only
  `file`, `tree`, and `generated` at
  `remote-manifest.schema.json:101-114`) and no extractor exists.
- **Wiki formal provenance/license:** source is `repository_only`; license is
  `not_found` (`hoi4-mod-setup.manifest.json:5217-5222`). These are evidence
  states, not legal conclusions.
- **Wiki marker:** repository evidence says one was observed, but it is absent
  from manifest metadata and file evidence; its distributable state is
  uncertain.
- **Wiki `referenced_only` / `none`:** accepted values but not implemented as
  distinct selection behavior.
- **Live upstream latest/release state:** not revalidated in this repository-only
  audit.

## Meaningful tests present

- checked-in manifest parsing and generation revision;
- remote bytes selected without bundled substitution;
- repository-manifest example runtime parsing;
- topological dependency expansion;
- deterministic direct reverse dependencies;
- dependency cycle rejection;
- missing expected-file/hash evidence rejection;
- immutable generation revision requirement;
- macOS non-blocking state for the current optional Windows MCP;
- glob include/exclude helper;
- release tag path-segment encoding;
- range start validation helper;
- basic path traversal/absolute-path rejection;
- NFC normalization collision;
- MCP initialize structure and bounded tools list;
- missing MCP immutable identity and arbitrary plan-path rejection;
- wiki broken-link readiness and local/external link behavior;
- schema/example package validation.

## Meaningful tests missing

The missing test matrix in M-06 is release-blocking for source completion. In
particular, the named acceptance surfaces have no end-to-end fake-source tests
for default-branch resolution, pinned release dereferencing, redirect/host
policy, response truncation, selective download, checksum/size mismatch,
corrupt cache, interrupted resume, or wiki media/marker coverage. There is also
no runtime test proving that plan and lock examples are artifacts the current
core can produce and accept.

## Recommended parent actions in fix order

1. **Enforce the remote-manifest schema at runtime** and fail closed on
   unsupported signing and update-policy declarations (H-01).
2. **Make readiness use exact locked provenance/license/marker/media metadata**
   and display the wiki license state (H-02).
3. **Propagate platform failures through dependencies** and expose
   reverse-dependency impact in review artifacts (M-01, M-02).
4. **Tighten plan/lock evidence contracts** for source/installed size and
   platform, with explicit legacy-incomplete handling (M-03).
5. **Replace illustrative plan/lock examples with runtime-generated fixtures**
   and add Rust runtime validation tests (M-04).
6. **Reconcile and enforce wiki marker/media policy** without inventing missing
   provenance (M-05).
7. **Add a fake source transport and hostile-source release gate** covering the
   complete latest/pinned/redirect/cache/wiki matrix (M-06).
8. **Implement contract-accurate Unicode case folding** and native collision
   tests (L-01).
9. **Retain bounded MCP tool identities** before any future capability is
   reported healthy (L-02).
10. Keep release assets and archives explicitly unsupported until repository
    evidence, identity fields, containment rules, and adversarial tests are
    designed (L-03); remove or implement dead cache metadata and checksum the
    direct bundled manifest path (L-04).
