import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const result = spawnSync(process.execPath, [require.resolve("@playwright/test/cli"), "test", ...process.argv.slice(2)], {
  cwd: new URL("..", import.meta.url),
  stdio: "inherit",
  windowsHide: true,
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
