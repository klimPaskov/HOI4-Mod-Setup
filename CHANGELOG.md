# Changelog

All notable changes to HOI4 Mod Setup should be recorded here. The project follows semantic versioning once public releases begin.

## Unreleased

### Added

- Initial product and architecture planning package
- User-facing README and contributor documentation
- Open-source GitHub workflow templates
- Repo-local AGENTS, ten living skills, and nine bounded subagent templates
- ChatGPT-managed Codex App Server authentication and analysis contract
- Launcher-ready generation for both descriptors and `thumbnail.png`

### Changed

- All semantic project identity and convention proposals use the user’s ChatGPT Codex access
- Desktop UI references use a restrained seven-phase wizard with progressive disclosure
- Scan, plan, lock, project-state, and readiness schemas record the Codex and launcher boundaries

### Security

- Codex owns ChatGPT tokens and refresh
- No OpenAI API key or external-token fallback exists in the core product
- Account identity and usage data stay out of projects and locks
- Downloads resolve to exact source revisions and require SHA-256 verification
