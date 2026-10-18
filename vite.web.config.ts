import { defineConfig } from "vite";
import type { Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

/** Hosts the web edition may contact. Mod files never leave the page. */
export const WEB_CONNECT_SOURCES = [
  "https://api.github.com",
  "https://raw.githubusercontent.com",
  // Setup-assistant routes; only the one the user chooses is contacted, with
  // the API key they entered.
  "https://api.anthropic.com",
  "https://generativelanguage.googleapis.com",
  "https://api.x.ai",
  "https://api.deepseek.com",
  "https://open.bigmodel.cn",
];

export const WEB_CONTENT_SECURITY_POLICY = [
  "default-src 'none'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "worker-src 'self'",
  "style-src 'self'",
  "img-src 'self' data: blob:",
  "font-src 'self'",
  `connect-src 'self' ${WEB_CONNECT_SOURCES.join(" ")}`,
  "base-uri 'none'",
  "form-action 'none'",
  "object-src 'none'",
].join("; ");

/** The policy is added only to the built page; the dev server injects
 * inline scripts for hot reload. */
function contentSecurityPolicy(): Plugin {
  return {
    name: "web-content-security-policy",
    apply: "build",
    transformIndexHtml(html) {
      return html.replace(
        "<meta charset=\"UTF-8\" />",
        `<meta charset="UTF-8" />\n    <meta http-equiv="Content-Security-Policy" content="${WEB_CONTENT_SECURITY_POLICY}" />`,
      );
    },
  };
}

const root = fileURLToPath(new URL("./web", import.meta.url));

// The dev server runs at the root; a build and its preview use the Pages path.
export default defineConfig(({ command, isPreview }) => ({
  root,
  base: command === "build" || isPreview ? process.env.WEB_BASE ?? "/HOI4-Mod-Setup/app/" : "/",
  plugins: [react(), contentSecurityPolicy()],
  publicDir: fileURLToPath(new URL("./web/public", import.meta.url)),
  server: {
    host: "localhost",
    port: 1430,
    strictPort: true,
    fs: { allow: [fileURLToPath(new URL(".", import.meta.url))] },
  },
  worker: { format: "es" },
  build: {
    outDir: fileURLToPath(new URL("./dist-web", import.meta.url)),
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
  },
}));
