import { spawnSync } from "node:child_process";

// Rust defaults to one test thread per logical CPU. Each native lifecycle
// test also starts Git, PowerShell, or fault-worker children; on large Windows
// runners this overwhelms process startup and turns bounded checks into flakes.
// Keep all tests enabled, with an explicit override for dedicated runners.
const threads = process.env.HOI4_TEST_RUST_THREADS ?? "4";
if (!/^[1-9]\d?$/.test(threads) || Number(threads) > 32) {
  throw new Error("HOI4_TEST_RUST_THREADS must be an integer from 1 to 32.");
}
const args = process.argv.slice(2);
// pnpm removes its own `--` separator. Restore Cargo's separator when the
// forwarded options belong to libtest instead of Cargo.
if (!args.includes("--")) {
  const testOption = args.findIndex((arg) => /^--(?:ignored|include-ignored|nocapture|exact|list|show-output|test-threads|skip)(?:=|$)/.test(arg));
  if (testOption !== -1) args.splice(testOption, 0, "--");
}
const result = spawnSync("cargo", ["test", "--workspace", "--all-features", ...args], {
  cwd: new URL("..", import.meta.url),
  env: { ...process.env, RUST_TEST_THREADS: threads },
  stdio: "inherit",
  windowsHide: true,
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
