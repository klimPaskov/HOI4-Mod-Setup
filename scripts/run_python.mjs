import { spawnSync } from "node:child_process";

// Distribution Python is normally named python3 on Linux and macOS.
const executable = process.platform === "win32" ? "python" : "python3";
const result = spawnSync(executable, process.argv.slice(2), {
  cwd: new URL("..", import.meta.url), stdio: "inherit", windowsHide: true,
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
