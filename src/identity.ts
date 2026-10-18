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
