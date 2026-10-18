export interface GeneratedIdentityDefaults {
  projectId: string;
  scriptPrefix: string;
  primaryNamespace: string;
  descriptorTags: string[];
  folderProfile: string[];
}

export const HOI4_DESCRIPTOR_TAGS = [
  "Alternative History",
  "Balance",
  "Events",
  "Fixes",
  "Gameplay",
  "Graphics",
  "Historical",
  "Ideologies",
  "Map",
  "Military",
  "National Focuses",
  "Sound",
  "Technologies",
  "Translation",
  "Utilities",
] as const;

function asciiWords(value: string): string[] {
  return value
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .match(/[a-z0-9]+/g) ?? [];
}

function projectSlug(value: string): string {
  const words = asciiWords(value);
  let slug = words.join("_") || "new_mod";
  if (/^[0-9]/.test(slug)) slug = `mod_${slug}`;
  if (slug.length < 2) slug = `${slug}_mod`;
  return slug.slice(0, 64);
}

function identifierPrefix(name: string, projectId: string): string {
  const words = asciiWords(name);
  const initials = words.slice(0, 3).map((word) => word[0]).join("");
  const candidate = initials.length >= 2 ? initials : projectId.replace(/_/g, "").slice(0, 3);
  return (candidate || "mod").slice(0, 64);
}

function inferredTags(name: string, description: string): string[] {
  const text = `${name} ${description}`.toLowerCase();
  const tags: string[] = [];
  if (/alternate|diverg|cold war|alternate history/.test(text)) tags.push("Alternative History");
  if (/\bhistorical\b|world war|wwii|ww2/.test(text)) tags.push("Historical");
  if (/\bevents?\b|\bdecisions?\b|\bpolitic\w*|\bdiplomac\w*/.test(text)) tags.push("Events");
  if (/focus tree|national focus/.test(text)) tags.push("National Focuses");
  if (/map|province|provinces|island|continent/.test(text)) tags.push("Map");
  if (/unit|navy|army|military|division|ship/.test(text)) tags.push("Military");
  if (/portrait|character|leader|3d|model|graphic|art/.test(text)) tags.push("Graphics");
  if (/sound|music|audio|voice/.test(text)) tags.push("Sound");
  if (/technology|technologies|research/.test(text)) tags.push("Technologies");
  if (/localisation|localization|translation|language/.test(text)) tags.push("Translation");
  if (/ideology|ideologies/.test(text)) tags.push("Ideologies");
  if (/balance|rebalance/.test(text)) tags.push("Balance");
  if (/fix|patch|bug/.test(text)) tags.push("Fixes");
  if (/utility|tool/.test(text)) tags.push("Utilities");
  if (/total conversion|overhaul|new countries|new nations|mechanic|gameplay/.test(text)) tags.push("Gameplay");
  return Array.from(new Set(tags.length ? tags : ["Gameplay"])).slice(0, 4);
}

function inferredFolders(description: string): string[] {
  const folders = ["common", "events", "localisation/english", "gfx", "interface", "docs"];
  const text = description.toLowerCase();
  if (/map|province|total conversion|overhaul/.test(text)) folders.push("map");
  if (/history|historical|country|countries|nation|nations/.test(text)) folders.push("history");
  return folders;
}

/**
 * Produce editable defaults before a provider turn is available. These are
 * conventions, not confirmed project facts; the Rust core still validates
 * them and a selected provider may replace them with reviewed proposals.
 */
export function deriveGeneratedIdentity(name: string, description: string): GeneratedIdentityDefaults {
  const projectId = projectSlug(name || description);
  const scriptPrefix = identifierPrefix(name || description, projectId);
  return {
    projectId,
    scriptPrefix,
    primaryNamespace: scriptPrefix,
    descriptorTags: inferredTags(name, description),
    folderProfile: inferredFolders(description),
  };
}

/** One value the project files decide, from the scan's `profile.detected`. */
export interface DetectedValue {
  key: string;
  value: unknown;
  reason: string;
}

/** Read the detected values a scan returned; malformed input yields none. */
export function detectedValues(findings: ReadonlyArray<{ id: string; value: string }>): DetectedValue[] {
  const finding = findings.find((candidate) => candidate.id === "profile.detected");
  if (!finding) return [];
  try {
    const parsed: unknown = JSON.parse(finding.value);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is DetectedValue => typeof item === "object" && item !== null
      && typeof (item as DetectedValue).key === "string" && typeof (item as DetectedValue).reason === "string");
  } catch {
    return [];
  }
}

export function detectedString(values: DetectedValue[], key: string): string | undefined {
  const value = values.find((item) => item.key === key)?.value;
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

export function detectedList(values: DetectedValue[], key: string): string[] | undefined {
  const value = values.find((item) => item.key === key)?.value;
  return Array.isArray(value) && value.length > 0 && value.every((item) => typeof item === "string") ? value as string[] : undefined;
}

export interface DeterministicImportValues {
  displayName: string;
  projectId: string;
  scriptPrefix: string;
  primaryNamespace: string;
  descriptorTags: string[];
  folderProfile: string[];
  localisationConvention?: string;
  documentationConvention?: string;
}

/**
 * Values for an imported mod when no setup assistant runs: what the files
 * decide is used as is, and only the gaps get the same generated defaults a
 * new mod gets. The user reviews every value before planning.
 */
export function deterministicImportValues(values: DetectedValue[], folderName: string, currentName: string): DeterministicImportValues {
  const displayName = detectedString(values, "display_name") ?? (currentName.trim() || folderName);
  const defaults = deriveGeneratedIdentity(displayName, "");
  const primaryNamespace = detectedString(values, "primary_namespace") ?? defaults.primaryNamespace;
  return {
    displayName,
    projectId: /^[a-z][a-z0-9_]{1,63}$/.test(folderName) ? folderName : defaults.projectId,
    scriptPrefix: detectedString(values, "script_prefix") ?? primaryNamespace,
    primaryNamespace,
    descriptorTags: detectedList(values, "descriptor_tags") ?? defaults.descriptorTags,
    folderProfile: detectedList(values, "folder_profile") ?? [],
    localisationConvention: detectedString(values, "localisation_convention"),
    documentationConvention: detectedString(values, "documentation_convention"),
  };
}
