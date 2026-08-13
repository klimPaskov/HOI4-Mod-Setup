import { execFile } from "node:child_process";
import { existsSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, win32 } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export const installLocationKey = "HKCU\\Software\\klimpaskov\\HOI4 Mod Setup";
export const uninstallKey = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\HOI4 Mod Setup";
const installLocationSubKey = "Software\\klimpaskov\\HOI4 Mod Setup";
const uninstallSubKey = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\HOI4 Mod Setup";
const machineUninstallSubKey = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
const productName = "HOI4 Mod Setup";
const manufacturer = "klimpaskov";
const missingKeyExitCode = 2;
const missingValueExitCode = 3;

const registryReadScript = String.raw`
$ErrorActionPreference = "Stop"
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($env:HOI4_MOD_SETUP_REGISTRY_SUBKEY, $false)
if ($null -eq $key) { exit 2 }
try {
  if ($env:HOI4_MOD_SETUP_REGISTRY_VALUE -eq "__KEY_ONLY__") { exit 0 }
  $valueName = if ($env:HOI4_MOD_SETUP_REGISTRY_VALUE -eq "__DEFAULT__") {
    ""
  } else {
    $env:HOI4_MOD_SETUP_REGISTRY_VALUE
  }
  $value = $key.GetValue(
    $valueName,
    $null,
    [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
  )
  if ($null -eq $value) { exit 3 }
  $bytes = [Text.Encoding]::UTF8.GetBytes([string]$value)
  [Console]::Out.Write([Convert]::ToBase64String($bytes))
} finally {
  $key.Dispose()
}
`;

const registryDeleteScript = String.raw`
$ErrorActionPreference = "Stop"
$root = [Microsoft.Win32.Registry]::CurrentUser
$key = $root.OpenSubKey($env:HOI4_MOD_SETUP_REGISTRY_SUBKEY, $false)
if ($null -eq $key) { exit 2 }
$key.Dispose()
try {
  $root.DeleteSubKeyTree($env:HOI4_MOD_SETUP_REGISTRY_SUBKEY, $false)
} catch [System.ArgumentException] {
  exit 2
}
`;

const legacyMachineInstallScript = String.raw`
$ErrorActionPreference = "Stop"
$views = @(
  [Microsoft.Win32.RegistryView]::Registry64,
  [Microsoft.Win32.RegistryView]::Registry32
)
foreach ($view in $views) {
  $root = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
    [Microsoft.Win32.RegistryHive]::LocalMachine,
    $view
  )
  try {
    $uninstall = $root.OpenSubKey($env:HOI4_MOD_SETUP_REGISTRY_SUBKEY, $false)
    if ($null -eq $uninstall) { continue }
    try {
      foreach ($childName in $uninstall.GetSubKeyNames()) {
        try {
          $child = $uninstall.OpenSubKey($childName, $false)
        } catch [System.Security.SecurityException] {
          throw
        } catch [System.UnauthorizedAccessException] {
          throw
        }
        if ($null -eq $child) { continue }
        try {
          $displayName = $child.GetValue(
            "DisplayName",
            $null,
            [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
          )
          $publisher = $child.GetValue(
            "Publisher",
            $null,
            [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
          )
          if (
            (([string]$displayName + [string]$publisher) -ieq
              ($env:HOI4_MOD_SETUP_PRODUCT_NAME + $env:HOI4_MOD_SETUP_MANUFACTURER))
          ) { exit 0 }
        } finally {
          $child.Dispose()
        }
      }
    } finally {
      $uninstall.Dispose()
    }
  } finally {
    $root.Dispose()
  }
}
exit 2
`;

function normalizeWindowsPath(value) {
  const trimmed = String(value ?? "").trim().replace(/^"|"$/g, "");
  return trimmed ? win32.normalize(trimmed).replace(/[\\/]+$/, "").toLowerCase() : "";
}

function executableFromCommand(value) {
  const command = String(value ?? "").trim();
  if (!command) return "";
  if (command.startsWith('"')) {
    const closingQuote = command.indexOf('"', 1);
    return closingQuote === -1 ? "" : command.slice(1, closingQuote);
  }
  const executableEnd = command.toLowerCase().indexOf(".exe");
  return executableEnd === -1 ? command.split(/\s/u, 1)[0] : command.slice(0, executableEnd + 4);
}

export function isSameWindowsPath(left, right) {
  const normalizedLeft = normalizeWindowsPath(left);
  return normalizedLeft !== "" && normalizedLeft === normalizeWindowsPath(right);
}

export function isWindowsPathInside(candidate, root) {
  const normalizedCandidate = normalizeWindowsPath(candidate);
  const normalizedRoot = normalizeWindowsPath(root);
  if (!normalizedCandidate || !normalizedRoot) return false;
  const relative = win32.relative(normalizedRoot, normalizedCandidate);
  return relative === "" || (!relative.startsWith("..") && !win32.isAbsolute(relative));
}

export function resolveNativeSystemExecutable(
  executableName,
  fileExists = existsSync,
  resolveNative = realpathSync.native,
) {
  const relativeExecutables = new Map([
    ["powershell.exe", "WindowsPowerShell\\v1.0\\powershell.exe"],
    ["taskkill.exe", "taskkill.exe"],
  ]);
  const normalizedName = String(executableName).toLowerCase();
  const relative = relativeExecutables.get(normalizedName);
  if (!relative) {
    throw new Error("installer E2E refused an unapproved native Windows executable");
  }
  const nativeAlias = `\\\\?\\GLOBALROOT\\SystemRoot\\System32\\${relative}`;
  let executable;
  try {
    executable = resolveNative(nativeAlias);
  } catch {
    throw new Error("installer E2E could not resolve the native Windows system directory");
  }
  if (
    !win32.isAbsolute(executable) ||
    win32.basename(executable).toLowerCase() !== normalizedName
  ) {
    throw new Error(`installer E2E resolved an invalid native ${normalizedName} path`);
  }
  if (!fileExists(executable)) {
    throw new Error(`installer E2E could not find the native ${normalizedName} executable`);
  }
  return executable;
}

export function resolveSystemPowerShellPath(
  fileExists = existsSync,
  resolveNative = realpathSync.native,
) {
  return resolveNativeSystemExecutable("powershell.exe", fileExists, resolveNative);
}

async function runRegistryScript(script, environment = {}) {
  return execFileAsync(
    resolveSystemPowerShellPath(),
    [
      "-NoLogo",
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-EncodedCommand",
      Buffer.from(script, "utf16le").toString("base64"),
    ],
    {
      encoding: "utf8",
      env: { ...process.env, ...environment },
      timeout: 10_000,
      windowsHide: true,
    },
  );
}

function exitedWith(error, exitCode) {
  return error && Number(error.code) === exitCode;
}

function subKeyFor(key) {
  if (key === installLocationKey) return installLocationSubKey;
  if (key === uninstallKey) return uninstallSubKey;
  throw new Error("installer E2E refused an unexpected Windows registry key");
}

export const windowsRegistry = {
  async keyExists(key) {
    try {
      await runRegistryScript(registryReadScript, {
        HOI4_MOD_SETUP_REGISTRY_SUBKEY: subKeyFor(key),
        HOI4_MOD_SETUP_REGISTRY_VALUE: "__KEY_ONLY__",
      });
      return true;
    } catch (error) {
      if (exitedWith(error, missingKeyExitCode)) return false;
      throw error;
    }
  },

  async readString(key, name = null) {
    try {
      const { stdout } = await runRegistryScript(registryReadScript, {
        HOI4_MOD_SETUP_REGISTRY_SUBKEY: subKeyFor(key),
        HOI4_MOD_SETUP_REGISTRY_VALUE: name === null ? "__DEFAULT__" : name,
      });
      return Buffer.from(stdout.trim(), "base64").toString("utf8");
    } catch (error) {
      if (exitedWith(error, missingKeyExitCode) || exitedWith(error, missingValueExitCode)) return null;
      throw error;
    }
  },

  async deleteKey(key) {
    try {
      await runRegistryScript(registryDeleteScript, {
        HOI4_MOD_SETUP_REGISTRY_SUBKEY: subKeyFor(key),
      });
    } catch (error) {
      if (!exitedWith(error, missingKeyExitCode)) throw error;
    }
  },

  async legacyMachineInstallExists() {
    try {
      await runRegistryScript(legacyMachineInstallScript, {
        HOI4_MOD_SETUP_REGISTRY_SUBKEY: machineUninstallSubKey,
        HOI4_MOD_SETUP_PRODUCT_NAME: productName,
        HOI4_MOD_SETUP_MANUFACTURER: manufacturer,
      });
      return true;
    } catch (error) {
      if (exitedWith(error, missingKeyExitCode)) return false;
      throw error;
    }
  },
};

export async function assertWindowsInstallerRegistryIsClean(registry = windowsRegistry) {
  if (typeof registry.legacyMachineInstallExists !== "function") {
    throw new Error("installer E2E registry adapter does not implement the legacy-install safety check");
  }
  if (await registry.legacyMachineInstallExists()) {
    throw new Error("installer E2E requires a machine without a legacy HOI4 Mod Setup installation");
  }
  const existingKeys = [];
  for (const key of [installLocationKey, uninstallKey]) {
    if (await registry.keyExists(key)) existingKeys.push(key);
  }
  if (existingKeys.length > 0) {
    throw new Error(
      `installer E2E requires a clean Windows user profile; existing product registry state was found at ${existingKeys.join(
        ", ",
      )}`,
    );
  }
}

function assertOwnedExecutable(command, installRoot, valueName) {
  const executable = executableFromCommand(command);
  if (!executable || !isWindowsPathInside(executable, installRoot)) {
    throw new Error(`refusing to remove Windows installer registry state: ${valueName} is not owned by this E2E run`);
  }
}

function assertOwnedTemporaryRoot(workRoot, tempRoot) {
  const name = win32.basename(workRoot);
  if (
    !/^hoi4-mod-setup-installer-e2e-[a-z0-9]{6}$/iu.test(name) ||
    !isSameWindowsPath(win32.dirname(workRoot), tempRoot)
  ) {
    throw new Error("refusing Windows installer cleanup outside a freshly minted E2E temporary root");
  }
}

export async function cleanupWindowsInstallerRegistry(
  { workRoot, installRoot, tempRoot = tmpdir() },
  registry = windowsRegistry,
) {
  assertOwnedTemporaryRoot(workRoot, tempRoot);
  if (!isWindowsPathInside(installRoot, workRoot) || isSameWindowsPath(installRoot, workRoot)) {
    throw new Error("refusing Windows installer registry cleanup because the install root is not contained by the E2E root");
  }

  const installLocationExists = await registry.keyExists(installLocationKey);
  const uninstallExists = await registry.keyExists(uninstallKey);

  if (installLocationExists) {
    const savedInstallRoot = await registry.readString(installLocationKey);
    if (!isSameWindowsPath(savedInstallRoot, installRoot)) {
      throw new Error("refusing to remove a Windows install-location key that is not owned by this E2E run");
    }
  }

  if (uninstallExists) {
    const savedInstallRoot = await registry.readString(uninstallKey, "InstallLocation");
    if (!isSameWindowsPath(savedInstallRoot, installRoot)) {
      throw new Error("refusing to remove Windows uninstall metadata that is not owned by this E2E run");
    }
    for (const name of ["DisplayIcon", "UninstallString", "QuietUninstallString"]) {
      const command = await registry.readString(uninstallKey, name);
      if (command) assertOwnedExecutable(command, installRoot, name);
    }
  }

  if (uninstallExists) await registry.deleteKey(uninstallKey);
  if (installLocationExists) await registry.deleteKey(installLocationKey);

  for (const key of [installLocationKey, uninstallKey]) {
    if (await registry.keyExists(key)) {
      throw new Error(`Windows installer E2E cleanup left product registry state behind at ${key}`);
    }
  }
}
