# Public-release compatibility fixture

`v0.3.5-manifest.json` contains the unchanged Git blob from the app tag
`v0.3.5:docs/source-manifest/hoi4-mod-setup.manifest.json`.
Its SHA-256 is `4550334afaea4cb3c6ad6d83219b02cadf3ee60d46112d7e46a352ee889ceb10`.
The published source revision is `0bb4917dca228886aa9a52963bc09e7078666d64`;
file evidence was generated from `7e50711580a3fa377f1acb834bb19a51128eacd2`.

The ordinary Rust suite validates this exact major-2 manifest. The opt-in
offline lifecycle suite installs its committed source, updates to the selected
package checkout's current committed revision, preserves the recorded Codex
primary and user edits, then verifies byte-exact rollback of the old files and
lock. Do not regenerate this fixture from a worktree or current manifest.
