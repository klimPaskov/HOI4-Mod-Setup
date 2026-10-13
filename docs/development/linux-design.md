# Native Linux support

Linux is a native platform in the existing Rust adapters and React wizard. The first release targets x86-64 Linux, with Ubuntu 22.04 as the build baseline and Ubuntu 24.04 as local acceptance. It adds AppImage and Debian packages beside the existing Windows and macOS downloads.

Project transaction, source checksum, merge and rollback rules are unchanged. Platform enums gain Linux; existing Windows/macOS project state remains valid without migration. The app reads manifest majors 2 through 5. Source major 5 introduces Linux platform declarations and exact per-platform npm identity; older apps reject that incompatible source contract rather than silently skip its runtime evidence.

Linux uses XDG config/data/cache and Downloads directories, bounded native/Flatpak Steam discovery, and Secret Service without plaintext fallback. Trusted system tools are root-owned, non-writable, and verified against root-owned dpkg records. Official coding-client artifacts are matched to publisher download digests before execution. Publisher digest checks are not described as full Sigstore transparency verification.

HOI4 Agent Tools has one release/version and registry integrity, with separate Windows and Linux installed-tree evidence. Linux's two npm internal symlinks are an exact closed map: verify their literal targets, require contained regular destinations, recreate only those links in the private runtime, and rehash before spawn. Arbitrary package symlinks remain rejected. Coding-client registration uses declared platform targets, not command-name guessing.

The live-testing wizard defaults to Clausewitz on all supported platforms. Native Linux registers `~/tools/Clausewitz-MCP/.venv/bin/python` with the absolute pinned server path directly. It has no WSL distribution or SSH alias. Compatible native Linux HOI4 1.19.2 and server readiness still require confirmation; registration alone does not verify gameplay. Windows-only 3D and Live Tools routes remain unavailable on Linux.

Release authority stays behind the existing exact annotated-tag, CI, platform, signing and curation gates. Linux packages have updater signatures, not an invented Linux OS signing claim. Dedicated CI performs native desktop and package lifecycle smokes with isolated profiles; local Ubuntu acceptance exercises the real MCP installers, scratch projects and credential-store roundtrip.

Requirements LNX-01 through LNX-09 are mapped to `paths.rs`, `credentials.rs`, `linux_trust.rs`, `process.rs`, `mcp.rs`, client configuration adapters, `live_testing.rs`, the Linux provider UI tests, real scratch lifecycle tests, package verifier tests, and CI/release workflows. Final evidence and remaining limitations are recorded with the release validation report.
