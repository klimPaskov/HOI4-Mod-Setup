# Parent review: platform release wiring

The platform-release auditor report was read before this review. The
repository-side findings were addressed as follows:

- platform build jobs are tag-only and carry the protected `release`
  environment; Windows and macOS signing values are scoped to separate build
  steps;
- release identity and a clean worktree are checked before the native build;
  package, Tauri, Cargo, and lock versions must agree;
- `scripts/prepare_release_assets.mjs` validates every downloaded platform
  manifest, source/tag/architecture binding, signed-evidence marker, package
  hash, and exact platform set before creating uniquely named draft assets;
- Windows cleanup removes imported certificate entries and its temporary root;
  macOS cleanup restores the previous default keychain and removes the
  temporary keychain/root even when earlier steps fail;
- native Windows PE architecture is checked from the built application binary
  rather than the NSIS bootstrap stub; macOS package architecture is checked
  from the mounted application bundle;
- automatic updater metadata is explicitly deferred for 0.1.0 rather than
  represented by a partial configuration.

The following remain external or intentionally incomplete release gates:

- real protected certificates, Apple notarization credentials, and native
  Windows/macOS signed builds have not run on this host;
- clean-machine installer lifecycle, screen-reader, contrast, 200% scaling,
  Codex login, launcher, and credential-store evidence still require native
  machines and controlled credentials;
- SBOM/provenance service configuration, upstream workflow-manifest
  publication, GitHub ruleset activation, and maintainer publication approval
  remain required before a public binary release.
