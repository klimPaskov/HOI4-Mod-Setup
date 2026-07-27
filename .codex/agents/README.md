# HOI4 Mod Setup Codex subagents

These are narrow helpers for the application repository. Spawn every project subagent with `fork_context=false` and provide the exact task, files, constraints, allowed writes, tests, and handoff path. The parent owns final implementation and completion.

| Agent | Scope |
| --- | --- |
| `hoi4setup_codex_integration_auditor` | ChatGPT auth, App Server, structured analysis, and token boundary |
| `hoi4setup_source_manifest_auditor` | Remote manifest, selective source, wiki, and hashes |
| `hoi4setup_scanner_auditor` | Deterministic scan, evidence, confidence, and semantic separation |
| `hoi4setup_transaction_recovery_auditor` | Journal, apply, recovery, and rollback |
| `hoi4setup_security_auditor` | Credentials, containment, process execution, and supply chain |
| `hoi4setup_ui_accessibility_auditor` | Minimal UI and accessibility |
| `hoi4setup_platform_release_auditor` | Windows, macOS, packaging, signing, and release |
| `hoi4setup_documentation_curator` | Documentation-only consistency patches |
| `hoi4setup_skill_maintainer` | Living skill updates and overlap control |
