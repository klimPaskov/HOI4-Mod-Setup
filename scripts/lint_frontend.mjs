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
if (findings.length) {
  console.error(findings.join("\n"));
  process.exit(1);
}
console.log("Frontend authority lint passed.");
