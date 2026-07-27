# Parent review of Codex integration audit

The read-only handoff in `report.md` was reviewed after the implementation pass. The parent reran the repository-owned checks; no real login, token inspection, or external authentication was performed.

Addressed in the current workspace:

- usage-limited ChatGPT accounts fail closed before semantic planning and are presented as a resumable UI state;
- absolute project roots and scan IDs remain core-only bindings, are omitted from the model prompt, and are skipped from serialized Codex records, plans, and locks;
- all ten semantic proposal keys are required by the output schema, deterministic validation, confirmation, and persisted record gate;
- login cancellation is exposed as a non-blocking command, enabling device-code fallback and logout to proceed;
- remote logout errors are returned through a typed bridge result while local process, analysis, and evidence cleanup still occurs;
- signed-out recovery and managed removal have a reachable local Welcome route, and removal validation does not require Codex semantics;
- output redaction/account-key rejection, HTTPS login URL filtering, fixed-path system-browser opening, turn/thread correlation, bounded JSONL framing, strict legacy binding migration, and explicit window-close process teardown are covered by core code and tests.

Remaining evidence or release work:

- browser/device login completion, packaged fake-App-Server route tests, macOS native execution, and screen-reader/contrast/200% desktop review still require platform or user-owned runtime evidence;
- cross-process App Server replacement and power-loss recovery need native fault-injection coverage;
- source-manifest publication, application commit/tag, signing/notarization, license selection, and clean-machine release gates remain external blockers.

A separate bounded security-auditor handoff for the fixed-path system-browser opener was started after the parent implementation and shut down after repeated timeouts without a report or edits. The parent therefore relies on the local URL-rejection test, typed bridge test, Rust process-policy checks, package validation, and Windows build evidence; an independent audit remains outstanding.

Validation rerun after the fixes: Rust all-feature tests (113), clippy with `-D warnings`, fuzz target compilation, frontend typecheck/lint/unit (14), full package validator, and secret scan passed.
