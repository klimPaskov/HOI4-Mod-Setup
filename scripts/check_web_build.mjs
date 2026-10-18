// Check the built web edition in dist-web/ before it is published: a strict
// Content Security Policy, no inline scripts, hash-pinned bundles, the
// WebAssembly core, published build hashes, and size budgets.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const dist = fileURLToPath(new URL("../dist-web/", import.meta.url));
const problems = [];
const html = readFileSync(join(dist, "index.html"), "utf8");

const csp = /<meta http-equiv="Content-Security-Policy" content="([^"]+)"/.exec(html)?.[1];
if (!csp) problems.push("index.html has no Content-Security-Policy");
else {
  const directives = Object.fromEntries(csp.split(";").map((part) => part.trim().split(/\s+/)).map(([name, ...values]) => [name, values]));
  if (directives["default-src"]?.join(" ") !== "'none'") problems.push("default-src must be 'none'");
  if (directives["script-src"]?.some((value) => value === "'unsafe-inline'" || value === "'unsafe-eval'" || value.startsWith("http"))) problems.push("script-src allows inline, eval, or remote code");
  const allowedHosts = new Set(["'self'", "https://api.github.com", "https://raw.githubusercontent.com", "https://api.anthropic.com", "https://api.openai.com", "https://generativelanguage.googleapis.com", "https://api.x.ai", "https://api.deepseek.com", "https://open.bigmodel.cn"]);
  for (const host of directives["connect-src"] ?? []) {
    if (!allowedHosts.has(host)) problems.push(`connect-src allows an unreviewed host: ${host}`);
  }
}

for (const match of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)) {
  if (!/\ssrc=/.test(match[1])) problems.push("index.html contains an inline script");
  else if (!/\sintegrity="sha384-/.test(match[1])) problems.push(`script without integrity: ${match[1].trim()}`);
}
for (const match of html.matchAll(/<link\b[^>]*rel="(?:stylesheet|modulepreload)"[^>]*>/g)) {
  if (!/\sintegrity="sha384-/.test(match[0])) problems.push(`stylesheet or preload without integrity: ${match[0]}`);
}

const wasm = join(dist, "core", "hoi4_setup_web_bg.wasm");
let wasmBytes = 0;
try {
  wasmBytes = statSync(wasm).size;
} catch {
  problems.push("the WebAssembly core is missing");
}
const WASM_BUDGET = 8 * 1024 * 1024;
if (wasmBytes > WASM_BUDGET) problems.push(`the WebAssembly core is ${wasmBytes} bytes, over the ${WASM_BUDGET} byte budget`);

let scriptBytes = 0;
const walk = (directory) => {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (name.endsWith(".map")) problems.push(`source map published: ${name}`);
    else if (name.endsWith(".js")) scriptBytes += statSync(path).size;
  }
};
walk(dist);
const SCRIPT_BUDGET = 1024 * 1024;
if (scriptBytes > SCRIPT_BUDGET) problems.push(`scripts total ${scriptBytes} bytes, over the ${SCRIPT_BUDGET} byte budget`);

const info = JSON.parse(readFileSync(join(dist, "build-info.json"), "utf8"));
if (!/^[0-9a-f]{40}$/.test(info.commit ?? "")) problems.push("build-info.json has no commit");
if (!info.files?.["core/hoi4_setup_web_bg.wasm"]) problems.push("build-info.json does not hash the WebAssembly core");

if (problems.length > 0) {
  for (const problem of problems) console.error(`web build: ${problem}`);
  process.exit(1);
}
console.log(`web build: ok (core ${(wasmBytes / 1024 / 1024).toFixed(2)} MB, scripts ${(scriptBytes / 1024).toFixed(0)} KB)`);
