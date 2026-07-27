import { readFile } from "node:fs/promises";

const html = await readFile(new URL("../index.html", import.meta.url), "utf8");
if (!html.includes('id="root"') || !html.includes("/src/main.tsx")) {
  throw new Error("application entry point is incomplete");
}
console.log("Browser smoke fixture is structurally ready; run the desktop E2E matrix on Windows and macOS CI runners.");
