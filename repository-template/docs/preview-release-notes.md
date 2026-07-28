# HOI4 Mod Setup development preview

This prerelease is built from one exact public Git commit and includes native
Windows `.exe` and macOS `.dmg` packages for testing.

The GitHub release assets are named clearly:

- `HOI4-Mod-Setup-windows-x64-setup.exe` — Windows x64 installer
- `HOI4-Mod-Setup-macos-arm64.dmg` — macOS Apple silicon installer
- `HOI4-Mod-Setup-macos-x64.dmg` — macOS Intel installer

The release page also includes the generated provenance and SHA-256 files.

It is a development preview, not a stable release. Windows and macOS may show
a platform security warning because stable publisher signing and notarization
are separate release gates. Verify `PREVIEW_PROVENANCE.json`,
`PREVIEW_ARTIFACTS.sha256`, and the source commit before installing.

The source is public under the Apache License 2.0:
<https://github.com/klimPaskov/HOI4-Mod-Setup>.
