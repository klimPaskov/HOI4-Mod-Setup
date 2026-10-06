import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

const root = new URL("../src/", import.meta.url);
const forbidden = ["fs.writeFile", "writeFileSync", "child_process", "process.env", "localStorage.setItem"];

async function walk(url) {
  const entries = await readdir(url, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const child = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, url);
    if (entry.isDirectory()) files.push(...await walk(child));
    else if (/\.(ts|tsx|js|jsx)$/.test(entry.name)) files.push(child);
  }
  return files;
}

const findings = [];
for (const file of await walk(root)) {
  const text = await readFile(file, "utf8");
  for (const pattern of forbidden) {
    if (text.includes(pattern)) findings.push(`${file.pathname}: forbidden UI authority ${pattern}`);
  }
}
// Tauri matches renderer arguments to Rust parameters by camelCase name and
// silently drops unknown keys, so a renamed parameter would arrive as None.
// Check every typed command against its Rust signature.
function topLevelKeys(objectBody) {
  const keys = [];
  let depth = 0;
  let token = "";
  for (const character of objectBody) {
    if ("{<([".includes(character)) depth += 1;
    else if ("}>)]".includes(character)) depth -= 1;
    if (depth === 0 && /[A-Za-z0-9_?]/.test(character)) token += character;
    else if (depth === 0 && character === ":" && token) {
      keys.push({ name: token.replace(/\?$/, ""), optional: token.endsWith("?") });
      token = "";
    } else if (depth === 0) token = "";
  }
  return keys;
}

function balanced(text, start, open, close) {
  let depth = 0;
  for (let index = start; index < text.length; index += 1) {
    if (text[index] === open) depth += 1;
    else if (text[index] === close) {
      depth -= 1;
      if (depth === 0) return text.slice(start + 1, index);
    }
  }
  return "";
}

const camel = (name) => name.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
const typescript = await readFile(new URL("../src/lib/tauri.ts", import.meta.url), "utf8");
const rust = await readFile(new URL("../src-tauri/src/commands.rs", import.meta.url), "utf8");
const rustCommands = new Map();
for (const match of rust.matchAll(/#\[tauri::command[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*(?:<[^>]*>)?\s*\(/g)) {
  const parameters = balanced(rust, match.index + match[0].length - 1, "(", ")")
    .split(/,(?![^<]*>)/)
    .map((parameter) => parameter.trim())
    .filter(Boolean)
    .map((parameter) => {
      const [name, ...type] = parameter.split(":");
      return { name: camel(name.trim().replace(/^mut\s+/, "")), type: type.join(":").trim() };
    })
    .filter((parameter) => !/AppHandle|Window|State</.test(parameter.type));
  rustCommands.set(match[1], parameters);
}
const mapStart = typescript.indexOf("TauriCommandMap");
const mapBody = balanced(typescript, typescript.indexOf("{", mapStart), "{", "}");
for (const match of mapBody.matchAll(/^\s{2}(\w+): \{ args: /gm)) {
  const command = match[1];
  const parameters = rustCommands.get(command);
  if (!parameters) continue;
  const argsStart = match.index + match[0].length;
  const argsBody = mapBody.startsWith("Record<string, never>", argsStart)
    ? ""
    : balanced(mapBody, argsStart, "{", "}");
  const keys = topLevelKeys(argsBody);
  for (const key of keys) {
    if (!parameters.some((parameter) => parameter.name === key.name)) {
      findings.push(`src/lib/tauri.ts: ${command} sends ${key.name}, which the Rust command does not accept`);
    }
  }
  for (const parameter of parameters) {
    if (!parameter.type.startsWith("Option<") && !keys.some((key) => key.name === parameter.name)) {
      findings.push(`src/lib/tauri.ts: ${command} never sends required Rust parameter ${parameter.name}`);
    }
  }
}

if (findings.length) {
  console.error(findings.join("\n"));
  process.exit(1);
}
console.log("Frontend authority lint passed.");
