export const DOCUMENTATION_SCENARIOS = [
  "welcome",
  "provider",
  "existing",
  "description",
  "identity",
  "components",
  "environments",
  "image-generation",
  "live-testing",
  "workflows",
  "mcp",
  "git",
  "dry-run",
  "ready",
  "maintenance",
  "chat-sources",
  "recovery",
] as const;

export type DocumentationScenario = typeof DOCUMENTATION_SCENARIOS[number];
