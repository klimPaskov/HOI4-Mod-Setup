# Risks and open decisions

## High risks

### No live manifest

Production cannot safely infer all files from names. Add the manifest before release.

### Wiki provenance and licensing

The snapshot exists, while formal source and license evidence were not verified. Add a source manifest with origin, snapshot method, date, and license evidence when available.

### Current Windows-only commands

The application is cross-platform, while current MCP and 3D routes are Windows-oriented. Keep platform-neutral core usable on macOS and mark those components unsupported until the repository maintains macOS routes.

### Executable bootstrap

The 3D script can download and install dependencies. Add preflight-only output and a machine-readable action report.

### Version-policy drift

README pinned wording differs from latest-at-bootstrap resolution. Choose and encode one policy per dependency.

## Medium risks

- large mods can have several intentional namespaces
- TOML comment preservation may be imperfect
- OneDrive and iCloud can interfere with atomic rename
- global packages may be shared by several projects
- deleted lock files remove three-way merge evidence
- nested Git repositories and worktrees can change ownership boundary
- launcher descriptor is outside the project
- large wiki and 3D dependencies can create substantial backups

## Open decisions

1. Is HOI4 Agent Tools required on macOS before a source-supported command exists?
2. Should wiki updates use subtree files or release bundles?
3. Should the lock be committed by default?
4. Should modified wiki pages default to merge or renamed incoming?
5. Should external packages be global, project-local, or repository-script owned?
6. What exact Open in Codex invocation is supported per platform?
7. How will manifest signing and key rotation work?
8. Will version 1 support only the named source repository?
9. What backup quota and retention policy applies to large components?
10. Should UI concept sources ship with the product repository?

## Decisions made here

- no complete source clone
- exact commit recorded in every mode
- no project secret
- missing 3D key is optional and non-blocking
- LoRA and ComfyUI is preference-only in version 1
- no silent overwrite
- transaction and rollback are mandatory
- Git push and online creation require separate approval
- no invented platform commands
- all semantic setup fields use ChatGPT-authenticated Codex
- App Server managed browser login is primary with device-code fallback
- the external launcher descriptor and generated thumbnail are lock-managed

## Open-source governance decisions

### License

A public repository is not enough to grant open-source permissions. Select Apache-2.0, MIT, MPL-2.0, or another reviewed license before the first public source release. Review direct dependencies and bundled assets before the decision is finalized.

### Maintainer ownership

The initial CODEOWNERS template uses `@klimPaskov`. Replace or expand this with organization teams when maintainership grows. Code owner review is useful only when listed owners have the required repository access.

### GitHub Actions pinning

The supplied workflows use readable major tags as planning templates. Before production release, review and pin third-party actions to immutable revisions according to the repository security policy.

### Support capacity

Decide when to enable GitHub Discussions, which released versions receive fixes, and what response expectations can be published without promising unavailable support.

### Signing ownership

Decide who controls Windows signing and Apple signing or notarization credentials, how recovery works, and how release access is revoked when maintainers change.

## Codex subscription risks

- App Server protocol changes can break a tightly coupled client.
- A missing or outdated Codex installation blocks setup planning.
- Browser callbacks can fail under locked-down networks, which requires device-code fallback.
- ChatGPT workspace policy can restrict Codex access.
- Usage limits can interrupt semantic analysis.
- Persisting account metadata can create unnecessary privacy risk.
- Overbroad input manifests can send unrelated project text.
- Allowing Codex to write during analysis would break the transaction and approval boundary.

Mitigation uses a typed protocol adapter, capability checks, schema validation, strict redaction, read-only turns, resumable draft state, and locally available recovery.
