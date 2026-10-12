import { build, preview } from "vite";
import { resolve } from "node:path";

// Compile synthetic fixture routes only into an ignored test artifact. The
// normal production build keeps those routes disabled.
const root = resolve(import.meta.dirname, "..");
const outDir = resolve(root, "artifacts", "browser-fixture");
await build({
  root,
  define: { "import.meta.env.DEV": "true" },
  build: { outDir, emptyOutDir: true },
});
const server = await preview({
  root,
  build: { outDir },
  preview: { host: "127.0.0.1", port: 1421, strictPort: true },
});
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.once(signal, () => server.httpServer.close(() => process.exit(0)));
}
