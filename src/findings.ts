// Readable presentation of deterministic scan findings. The raw finding value
// stays the exact core-scanned string because approved evidence is bound to
// its hash; these helpers only shape what the review screen shows.

const FINDING_LABELS: Record<string, string> = {
  project_instructions: "Project instructions",
  config_present: "Codex project configuration",
  installed_environments: "Coding environments",
  mod_name: "Mod name",
  project_descriptor: "Project descriptor",
  launcher_descriptor: "Launcher file",
  documentation_files: "Documentation files",
  repository: "Git repository",
  managed_setup: "Existing setup",
  portrait_routes: "Portrait settings",
  server_ids: "MCP servers",
  absolute_path_files: "Files with absolute paths",
  skill_count: "Skills",
  subagent_count: "Subagents",
  thumbnail: "Thumbnail",
};

const ENVIRONMENT_LABELS: Record<string, string> = {
  codex: "Codex",
  claude_code: "Claude Code",
  cursor: "Cursor",
  qoder: "Qoder",
  opencode: "OpenCode",
};

export function findingLabel(key: string): string {
  const known = FINDING_LABELS[key];
  if (known) return known;
  const words = key.replace(/[_.]+/g, " ").trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : "Finding";
}

function plural(count: number, singular: string, pluralForm = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : pluralForm}`;
}

function environmentList(ids: unknown): string {
  if (!Array.isArray(ids) || ids.length === 0) return "";
  return ids.map((id) => ENVIRONMENT_LABELS[String(id)] ?? String(id)).join(", ");
}

function parse(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

function genericSummary(value: unknown): string {
  if (value === null || value === undefined || value === "") return "None";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "number" || typeof value === "string") return String(value);
  if (Array.isArray(value)) return value.length ? value.slice(0, 4).map(genericSummary).join(", ") + (value.length > 4 ? ` and ${value.length - 4} more` : "") : "None";
  const scalars = Object.entries(value as Record<string, unknown>)
    .filter(([, item]) => ["string", "number", "boolean"].includes(typeof item))
    .slice(0, 3)
    .map(([key, item]) => `${findingLabel(key)}: ${genericSummary(item)}`);
  return scalars.length ? scalars.join(" · ") : "Details available";
}

/** Short, readable summary of a finding value for the review list. */
export function findingDisplayValue(key: string, raw: string): string {
  const value = parse(raw);
  const record = value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
  switch (key) {
    case "project_instructions":
      return value === true || raw === "true" ? "Found (AGENTS.md)" : "Not found";
    case "config_present":
      return value === true || raw === "true" ? "Found" : "Not found";
    case "installed_environments": {
      if (!record) break;
      const detected = environmentList(record.detected);
      const primary = ENVIRONMENT_LABELS[String(record.primary)] ?? String(record.primary ?? "Codex");
      return detected ? `Detected: ${detected}` : `None detected · ${primary} will be the default`;
    }
    case "repository": {
      if (!record) break;
      if (record.present !== true) return "No Git repository";
      const parts = [record.branch ? `Branch ${String(record.branch)}` : record.detached ? "Detached HEAD" : "Git repository"];
      const changed = Number(record.staged_files ?? 0) + Number(record.unstaged_files ?? 0) + Number(record.untracked_files ?? 0);
      if (changed > 0) parts.push(`${plural(changed, "changed setup file")}`);
      if (record.status_probe === "unsafe_configuration") parts.push("some details unavailable");
      return parts.join(" · ");
    }
    case "managed_setup":
      if (!record) break;
      return record.present === true ? record.valid === false ? "Found, needs repair" : "Installed by HOI4 Mod Setup" : "Not installed yet";
    case "server_ids":
      return Array.isArray(value) && value.length ? value.join(", ") : "None";
    case "absolute_path_files":
      return Array.isArray(value) && value.length ? plural(value.length, "file") : "None";
    case "skill_count":
    case "subagent_count": {
      if (!record) break;
      const count = Number(record.count ?? 0);
      const malformed = Array.isArray(record.malformed) ? record.malformed.length : 0;
      const noun = key === "skill_count" ? "skill" : "subagent";
      return malformed ? `${plural(count, noun)}, ${malformed} need review` : count ? plural(count, noun) : "None";
    }
    case "thumbnail":
      if (!record) break;
      return record.width && record.height ? `${String(record.width)} × ${String(record.height)}${record.managed_placeholder ? " placeholder" : ""}` : "Found";
    case "documentation_files":
      return typeof value === "number" ? value ? plural(value, "file") : "None" : genericSummary(value);
    case "portrait_routes":
      return "Kept on this computer";
    default:
      break;
  }
  return genericSummary(value);
}
