export interface GeneratedIdentityDefaults {
  projectId: string;
  scriptPrefix: string;
  primaryNamespace: string;
  descriptorTags: string[];
  folderProfile: string[];
}

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
  if (/total conversion|overhaul|new countries|new nations|map|province|provinces/.test(text)) tags.push("Total Conversion");
  if (/alternate|diverg|cold war|history|historical/.test(text)) tags.push("Alternative History");
  if (/\bevents?\b|\bdecisions?\b|\bfocus tree\b|\bpolitic\w*|\bdiplomac\w*/.test(text)) tags.push("Events");
  if (/portrait|character|leader/.test(text)) tags.push("Portraits");
  if (/3d|model|unit|building/.test(text)) tags.push("3D");
  return (tags.length ? tags : ["Gameplay"]).slice(0, 4);
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
