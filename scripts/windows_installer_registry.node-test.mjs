import assert from "node:assert/strict";
import test from "node:test";

import {
  assertWindowsInstallerRegistryIsClean,
  cleanupWindowsInstallerRegistry,
  installLocationKey,
  isSameWindowsPath,
  isWindowsPathInside,
  resolveNativeSystemExecutable,
  resolveSystemPowerShellPath,
  uninstallKey,
} from "./windows_installer_registry.mjs";

class FakeRegistry {
  constructor(entries = {}, { legacyMachineInstall = false } = {}) {
    this.entries = new Map(Object.entries(entries).map(([key, value]) => [key, { ...value }]));
    this.deleted = [];
    this.legacyMachineInstall = legacyMachineInstall;
  }

  async keyExists(key) {
    return this.entries.has(key);
  }

  async readString(key, name = null) {
    return this.entries.get(key)?.[name ?? "(Default)"] ?? null;
  }

  async deleteKey(key) {
    this.deleted.push(key);
    this.entries.delete(key);
  }

  async legacyMachineInstallExists() {
    return this.legacyMachineInstall;
  }
}

const tempRoot = "C:\\Users\\tester\\AppData\\Local\\Temp";
const workRoot = `${tempRoot}\\hoi4-mod-setup-installer-e2e-Ab12Cd`;
const installRoot = `${workRoot}\\installed`;
const cleanupOptions = { workRoot, installRoot, tempRoot };

test("Windows path ownership is exact and case insensitive", () => {
  assert.equal(isSameWindowsPath("C:\\TEMP\\APP\\", "c:\\temp\\app"), true);
  assert.equal(isWindowsPathInside("C:\\temp\\app2", "C:\\temp\\app"), false);
  assert.equal(isWindowsPathInside("C:\\temp\\app\\installed", "C:\\temp\\app"), true);
});

test("system PowerShell resolution uses the native Windows directory alias", () => {
  const expected = "D:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
  assert.equal(resolveSystemPowerShellPath(() => true, () => expected), expected);
});

test("system PowerShell resolution fails closed when the native alias is unavailable", () => {
  assert.throws(
    () => resolveSystemPowerShellPath(() => true, () => {
      throw new Error("unavailable");
    }),
    /could not resolve the native Windows system directory/u,
  );
});

test("native taskkill resolution ignores PATH and validates the system executable", () => {
  const expected = "C:\\Windows\\System32\\taskkill.exe";
  assert.equal(
    resolveNativeSystemExecutable("taskkill.exe", () => true, () => expected),
    expected,
  );
  assert.throws(
    () => resolveNativeSystemExecutable("cmd.exe", () => true, () => expected),
    /unapproved native Windows executable/u,
  );
});

test("installer E2E refuses to overwrite an existing product installation", async () => {
  const registry = new FakeRegistry({
    [installLocationKey]: { "(Default)": "C:\\Users\\tester\\AppData\\Local\\HOI4 Mod Setup" },
  });
  await assert.rejects(assertWindowsInstallerRegistryIsClean(registry), /requires a clean Windows user profile/u);
  assert.deepEqual(registry.deleted, []);
});

test("installer E2E refuses a matching legacy machine installation", async () => {
  const registry = new FakeRegistry({}, { legacyMachineInstall: true });
  await assert.rejects(assertWindowsInstallerRegistryIsClean(registry), /legacy HOI4 Mod Setup installation/u);
  assert.deepEqual(registry.deleted, []);
});

test("installer E2E fails closed when the legacy registry query fails", async () => {
  const registry = new FakeRegistry();
  registry.legacyMachineInstallExists = async () => {
    throw new Error("access denied");
  };
  await assert.rejects(assertWindowsInstallerRegistryIsClean(registry), /access denied/u);
  assert.deepEqual(registry.deleted, []);
});

test("installer E2E fails closed when a current-user registry query fails", async () => {
  const registry = new FakeRegistry();
  registry.keyExists = async () => {
    throw new Error("query failed");
  };
  await assert.rejects(assertWindowsInstallerRegistryIsClean(registry), /query failed/u);
  assert.deepEqual(registry.deleted, []);
});

test("cleanup removes only registry state owned by the current E2E root", async () => {
  const registry = new FakeRegistry({
    [installLocationKey]: { "(Default)": installRoot },
    [uninstallKey]: {
      InstallLocation: installRoot,
      DisplayIcon: `"${installRoot}\\hoi4-mod-setup.exe"`,
      UninstallString: `"${installRoot}\\uninstall.exe"`,
      QuietUninstallString: `"${installRoot}\\uninstall.exe" /S`,
    },
  });
  await cleanupWindowsInstallerRegistry(cleanupOptions, registry);
  assert.deepEqual(registry.deleted.sort(), [installLocationKey, uninstallKey].sort());
});

test("cleanup preserves registry state that points outside the E2E root", async () => {
  const registry = new FakeRegistry({
    [installLocationKey]: { "(Default)": "C:\\Users\\tester\\AppData\\Local\\HOI4 Mod Setup" },
  });
  await assert.rejects(
    cleanupWindowsInstallerRegistry(cleanupOptions, registry),
    /not owned by this E2E run/u,
  );
  assert.deepEqual(registry.deleted, []);
});

test("cleanup validates every product key before deleting either one", async () => {
  const registry = new FakeRegistry({
    [installLocationKey]: { "(Default)": installRoot },
    [uninstallKey]: {
      InstallLocation: "C:\\Users\\tester\\AppData\\Local\\HOI4 Mod Setup",
      UninstallString: '"C:\\Users\\tester\\AppData\\Local\\HOI4 Mod Setup\\uninstall.exe"',
    },
  });
  await assert.rejects(
    cleanupWindowsInstallerRegistry(cleanupOptions, registry),
    /uninstall metadata that is not owned by this E2E run/u,
  );
  assert.deepEqual(registry.deleted, []);
});

test("cleanup rejects an external uninstall command before deleting registry state", async () => {
  const registry = new FakeRegistry({
    [installLocationKey]: { "(Default)": installRoot },
    [uninstallKey]: {
      InstallLocation: installRoot,
      DisplayIcon: '"C:\\Windows\\System32\\notepad.exe"',
      UninstallString: `"${installRoot}\\uninstall.exe"`,
    },
  });
  await assert.rejects(
    cleanupWindowsInstallerRegistry(cleanupOptions, registry),
    /DisplayIcon is not owned by this E2E run/u,
  );
  assert.deepEqual(registry.deleted, []);
});

test("cleanup succeeds after NSIS has already removed the uninstall key", async () => {
  const registry = new FakeRegistry({
    [installLocationKey]: { "(Default)": installRoot },
  });
  await cleanupWindowsInstallerRegistry(cleanupOptions, registry);
  assert.deepEqual(registry.deleted, [installLocationKey]);
});

test("cleanup refuses a caller-supplied root outside the minted E2E namespace", async () => {
  const registry = new FakeRegistry({
    [installLocationKey]: { "(Default)": "C:\\Users\\tester\\AppData\\Local\\HOI4 Mod Setup" },
  });
  await assert.rejects(
    cleanupWindowsInstallerRegistry(
      {
        workRoot: "C:\\Users\\tester\\AppData\\Local",
        installRoot: "C:\\Users\\tester\\AppData\\Local\\HOI4 Mod Setup",
        tempRoot,
      },
      registry,
    ),
    /freshly minted E2E temporary root/u,
  );
  assert.deepEqual(registry.deleted, []);
});
