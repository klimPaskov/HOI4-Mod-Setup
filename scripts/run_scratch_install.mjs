import { spawnSync } from "node:child_process";
import { isAbsolute, resolve } from "node:path";

const source = process.env.HOI4_MOD_SETUP_SOURCE_REPOSITORY;
if (!source || !isAbsolute(source)) {
  throw new Error("Set HOI4_MOD_SETUP_SOURCE_REPOSITORY to an explicit committed package checkout.");
}
const environment = Object.fromEntries(Object.entries(process.env).filter(([name]) =>
  !/(?:TOKEN|SECRET|PASSWORD|API.?KEY|COOKIE|AUTHORIZATION|CREDENTIAL|^ANTHROPIC_|^OPENAI_|^MESHY_|^CODEX_HOME$)/i.test(name)));
const result = spawnSync(process.execPath, [resolve(import.meta.dirname, "run_rust_tests.mjs"),
  "live_scratch_install_starts_every_registered_server_for_claude_and_codex", "--", "--ignored", "--nocapture"], {
  cwd: resolve(import.meta.dirname, ".."),
  env: { ...environment, HOI4_TEST_RUST_THREADS: "1" },
  stdio: "inherit", windowsHide: true,
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
