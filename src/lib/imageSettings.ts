// Pure rules for the settings a hoi4-imagegen backend reads from
// .hoi4-imagegen/config.toml, matching the core's check so a page never offers
// a value the core will refuse.

/** A short label for a backend setting, from the last part of its TOML key. */
export function imageSettingLabel(key: string): string {
  const name = key.split(".").pop() ?? key;
  return name.charAt(0).toUpperCase() + name.slice(1).replace(/_/g, " ");
}

/** An address on this computer, without credentials. */
export function isLoopbackUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname) && !url.username && !url.password;
  } catch {
    return false;
  }
}

/** An address is https anywhere or http on this computer; a path stays inside the project. */
export function imageSettingProblem(kind: string, value: string): string | null {
  if (!value) return null;
  if (value.length > 300 || /[\s\u0000-\u001f]/.test(value)) return "Enter a value without spaces.";
  if (kind === "url") {
    try {
      const url = new URL(value);
      if (url.username || url.password || !["http:", "https:"].includes(url.protocol)) return "Enter an http or https address without a user name.";
      if (url.protocol === "http:" && !isLoopbackUrl(value)) return "An address outside this computer must use https.";
      return null;
    } catch {
      return "Enter an http or https address without a user name.";
    }
  }
  if (kind === "text") return /^[A-Za-z0-9._:/-]+$/.test(value) ? null : "Use letters, digits and . _ : / - only.";
  if (kind === "project_path") {
    const ok = !value.startsWith("/") && !value.includes("\\") && !value.includes(":") && !value.split("/").some((part) => part === ".." || part === "") && value.toLowerCase().endsWith(".json");
    return ok ? null : "Enter a .json path inside the project, with forward slashes.";
  }
  return "This setting kind is not supported.";
}

interface BackendLike {
  id: string;
  default?: boolean;
  settings: Array<{ key: string; kind: string; required: boolean; default?: string | null }>;
}

/** The first problem in the chosen backend's settings: a refused value, or a required one left empty. */
export function backendSettingsProblem(backends: BackendLike[], values: Record<string, string> | undefined): string | null {
  const backend = backends.find((option) => option.id === values?.backend) ?? backends.find((option) => option.default) ?? backends[0];
  for (const setting of backend?.settings ?? []) {
    const value = values?.[setting.key] ?? "";
    if (!value && setting.required && !setting.default) return `${imageSettingLabel(setting.key)} is required.`;
    const problem = imageSettingProblem(setting.kind, value);
    if (problem) return `${imageSettingLabel(setting.key)}: ${problem}`;
  }
  return null;
}
