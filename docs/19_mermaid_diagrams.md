# Mermaid diagrams

Standalone Mermaid files are under `diagrams/`.

## System context

```mermaid
flowchart LR
  U[Mod author] --> UI[HOI4 Mod Setup]
  UI --> APP[codex app-server]
  APP --> CHATGPT[ChatGPT account and Codex usage]
  UI --> SCAN[Deterministic read-only scanner]
  SCAN --> REVIEW[Detected facts]
  APP --> REVIEW[Schema-valid semantic proposals]
  REVIEW --> PLAN[Confirmed installation plan]
  PLAN --> TX[Transactional installer]
  TX --> MOD[HOI4 mod project]
  TX --> LAUNCHER[External launcher descriptor]
  TX --> VAULT[OS credential vault]
  UI --> MAN[Manifest and dependency engine]
  MAN --> GH[GitHub API and raw files]
```

## ChatGPT authentication and analysis

```mermaid
sequenceDiagram
  participant U as User
  participant UI as Desktop app
  participant C as Codex App Server
  participant V as Deterministic validators
  U->>UI: Create or import
  UI->>C: initialize
  UI->>C: account/read
  alt Signed out
    UI->>C: account/login/start type chatgpt
    C-->>UI: authUrl and loginId
    UI->>U: Open ChatGPT sign-in
    C-->>UI: login completed and account updated
  end
  UI->>C: turn/start with approved input and outputSchema
  C-->>UI: streamed result
  UI->>V: validate schema, identifiers, paths, and collisions
  V-->>UI: accepted proposals or blocking errors
  UI->>U: Confirm or edit
  U->>UI: Confirmed values
```

## Component graph

```mermaid
flowchart TD
  AUTH[codex.app_server and ChatGPT sign-in] --> ANALYSIS[Confirmed Codex analysis]
  ANALYSIS --> AG[core.agents]
  ANALYSIS --> LAUNCH[project.launcher_scaffold]
  AG --> SK[core.skills]
  SK --> SA[core.subagents]
  AG --> CX[codex.config]
  CX --> MCP[mcp.hoi4_agent_tools]
  W[wiki.snapshot]
  SK --> D3[workflow.3d]
  SA --> D3
  KEY[MESHY_API_KEY in OS vault] --> D3
  AG --> READY[Core readiness]
  LAUNCH --> READY
  SK --> READY
  SA --> READY
  CX --> READY
  W --> READY
  D3 -. optional .-> REPORT[Readiness report]
  READY --> REPORT
```

## Transaction

```mermaid
stateDiagram-v2
  [*] --> Preflight
  Preflight --> ResolveSource
  ResolveSource --> Download
  Download --> Verify
  Verify --> DryRunReview
  DryRunReview --> Backup: approved
  Backup --> Staging
  Staging --> Validation
  Validation --> Apply
  Apply --> PostInstallChecks
  PostInstallChecks --> ReadinessReport
  ReadinessReport --> RollbackRecord
  RollbackRecord --> Completed
  Staging --> Interrupted
  Apply --> Interrupted
  Interrupted --> Staging: resume
  Interrupted --> Rollback
  Rollback --> RolledBack
```

Additional files cover new project, existing project, merge and update, credentials, readiness, and recovery.
