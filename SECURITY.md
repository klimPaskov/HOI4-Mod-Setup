# Security policy

HOI4 Mod Setup reads and writes development repositories, downloads workflow files, manages optional credentials, and can run verified external tools. Security reports should be handled privately.

## Reporting a vulnerability

Use GitHub private vulnerability reporting when it is enabled for the repository. If it is unavailable, use the private security contact listed in the repository profile or release notes.

Do not open a public issue for:

- credential exposure
- path traversal or link escape
- arbitrary command execution
- unsafe archive extraction
- signature, checksum, or manifest bypass
- rollback data loss
- unintended overwrite of user files
- update-channel compromise
- release signing or notarization compromise

Include a concise reproduction, affected version, platform, impact, and any safe proof-of-concept files. Remove API keys, access tokens, private repository contents, usernames, and unnecessary absolute paths.

## Response expectations

Maintainers should acknowledge a report, reproduce it in a private branch, classify affected releases, prepare a fix and regression test, and coordinate disclosure. Exact response timelines should be published once the maintainer team and support capacity are known.

## Supported versions

Before the first stable release, only the latest published preview may receive security fixes. After stable release, the project should publish a support table here.

## Security requirements for contributions

- No secrets in source, tests, logs, fixtures, screenshots, examples, lock files, or crash reports
- No shell-string command construction for untrusted values
- Exact source revision and SHA-256 verification
- Root containment and symlink or junction defense
- Safe archive extraction limits
- Explicit transaction rollback evidence
- Credential store use instead of project configuration
- GitHub Actions permissions minimized per job
- Third-party actions pinned to reviewed immutable revisions before production use
- Release credentials limited to protected environments

## Public repository configuration

Private vulnerability reporting is the security intake. Keep public issue forms
free of vulnerability reports. Default GitHub Actions permissions are
read-only; only the final gated publication job receives release write access.

Production third-party actions are pinned to reviewed immutable revisions.

## ChatGPT authentication

HOI4 Mod Setup delegates authentication to Codex App Server. Reports involving copied tokens, account leakage, protocol logs with login URLs or device codes, API-key fallback, or semantic-analysis write access are security issues and should be reported privately.

## Codex token ownership

Codex owns ChatGPT token persistence and refresh. Reports should treat any application read, copy, log, support-bundle inclusion, project serialization, or lock serialization of Codex tokens as a security defect. The normal product has no OpenAI API-key fallback.
