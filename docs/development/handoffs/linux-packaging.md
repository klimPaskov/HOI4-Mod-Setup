# Linux packaging and release handoff

## Implemented

- Four native targets: Windows x64, macOS arm64, macOS x64, and Ubuntu 22.04 Linux x64.
- Linux builds request `appimage,deb`; bundle parsing rejects unsafe, duplicate, and incompatible selections and copies only the requested fresh outputs.
- Native verification binds source/version/runner identity and hashes, validates native and packaged ELF64, the AppImage type-2 marker and contained payload, and Debian product/version/amd64 identity.
- Linux desktop and package launch smokes use Xvfb/D-Bus, isolated XDG profiles, bounded process-group cleanup, AppImage extraction, and safe local Debian extraction.
- Dedicated disposable CI additionally runs actual Debian installation/purge, refuses existing product state, fails closed on unreadable package state, and verifies absence even after launch failure. Ordinary local runs do not alter the host package database.
- Protected `sign-linux` checks out no source and has no OIDC. Exactly one bounded step receives the Tauri updater private key and signs both final Linux packages. It records package-bound updater evidence without claiming an OS signature.
- Curation cryptographically verifies both Linux updater artifacts against the committed public key through the existing Rust verifier before constructing the publication bundle.
- `latest.json` provides `linux-x86_64-appimage`, `linux-x86_64-deb`, and the AppImage `linux-x86_64` fallback. Debian and AppImage signatures and download URLs remain distinct.
- Stable publication has five installers plus `latest.json` and two macOS archives: eight public files. Exact uploaded names and downloaded bytes are verified before undrafting. Preview publication has five installers.
- The checked-in main ruleset retains every existing required check and adds `Desktop E2E (ubuntu-22.04)`. `scripts/apply_main_ruleset.py` binds current main, requires the latest completed successful GitHub Actions check evidence, rejects policy drift, and applies only the reviewed check change.
- Updated `RELEASING.md`, `DEVELOPMENT.md`, `docs/20_testing_strategy.md`, and the open-source-release/testing skills.

## Local checks completed

- `pnpm test:release-assets`: 32 passing tests, including Linux architecture/format and Debian lifecycle failure regressions, complete updater routes, dependency-inventory unions, missing/foreign/changed release artifacts, and existing Windows registry protections.
- `python scripts/check_workflow_security.py`: passed, including mandatory Linux native jobs and protected final updater verification.
- `python -m unittest scripts/test_linux_release_gates.py`: three passing tests covering absent native gates, failed/skipped/cancelled/foreign or stale successful CI evidence, and preservation of server-added review protection.
- `python scripts/validate_repository.py`: ten repository integrity groups passed.
- Node syntax and Git whitespace checks passed.

## Parent acceptance remaining

Run the actual Linux build and native smokes in WSL's Ubuntu 24.04 development
profile. The CI Ubuntu 22.04 build establishes compatibility with the release
baseline and executes the actual dedicated Debian lifecycle. Review Windows
and both macOS native evidence before publishing. Real account login, desktop
Secret Service round trips, real launcher/game loading, and signed
replacement/restart/retry are separate acceptance gates.

After all required checks succeed on current main, review
`python scripts/apply_main_ruleset.py --sha <main-commit>` and apply with
`--apply`. This handoff does not claim the live protection change or release
publication has occurred. No packages, keys, or certificates are committed.
