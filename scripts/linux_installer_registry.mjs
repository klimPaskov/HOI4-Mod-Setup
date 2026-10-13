import { existsSync } from "node:fs";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const queryArgs = ["--show", "--showformat=${Package}\t${db:Status-Status}\n"];
const binary = "/usr/bin/hoi4-mod-setup";
const product = "hoi4-mod-setup";
const defaults = { exec: promisify(execFile), exists: existsSync };

async function registered(adapter) {
  // Query the full database; unknown product and unreadable database must not
  // share the same benign interpretation of dpkg-query exit status 1.
  const inventory = await adapter.exec("/usr/bin/dpkg-query", queryArgs, { timeout: 30_000, maxBuffer: 8 * 1024 * 1024 });
  return inventory.stdout.split("\n").some((line) => line.split("\t")[0] === product);
}

export async function withLinuxDebInstallation(packagePath, launch, injected = {}) {
  const adapter = { ...defaults, ...injected };
  if (await registered(adapter)) throw new Error("dedicated Linux installer smoke refuses an existing product registration");
  if (adapter.exists(binary)) throw new Error("dedicated Linux installer smoke refuses an existing application executable");
  let failure;
  try {
    await adapter.exec("/usr/bin/sudo", ["-n", "/usr/bin/dpkg", "--install", packagePath], { timeout: 120_000 });
    await launch(binary);
  } catch (error) { failure = error; }
  try {
    // Install can fail after writing package state. The clean preflight proves
    // that this fixed product registration belongs exclusively to this test.
    await adapter.exec("/usr/bin/sudo", ["-n", "/usr/bin/dpkg", "--purge", product], { timeout: 120_000 });
    const remains = await registered(adapter);
    if (remains || adapter.exists(binary)) throw new Error("Debian uninstall left the executable or package registration behind");
  } catch (error) {
    failure = failure ? new AggregateError([failure, error], "Debian installation and cleanup failed") : error;
  }
  if (failure) throw failure;
}
