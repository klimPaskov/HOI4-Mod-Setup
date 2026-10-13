import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { resolveSystemPowerShellPath } from "./windows_installer_registry.mjs";
import { requireHostedWindows } from "./released_e2e_support.mjs";

const exec = promisify(execFile);

// WebView2 150+ ignores environment overrides in elevated hosted runners.
// Scope the documented HKLM override to this executable and remove exact owned values.
const policyScript = String.raw`
$ErrorActionPreference = 'Stop'
$entries = @{
  'AdditionalBrowserArguments' = $env:HOI4_E2E_WEBVIEW_ARGS
  'UserDataFolder' = $env:HOI4_E2E_WEBVIEW_PROFILE
}
$name = 'hoi4-mod-setup.exe'
foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64)) {
  $root = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $view)
  try {
    foreach ($entry in $entries.GetEnumerator()) {
      $path = 'SOFTWARE\Policies\Microsoft\Edge\WebView2\' + $entry.Key
      $key = $root.OpenSubKey($path, $true)
      try {
        $existing = if ($null -eq $key) { $null } else { $key.GetValue($name, $null) }
        if ($env:HOI4_E2E_WEBVIEW_MODE -eq 'preflight') {
          if ($null -ne $existing) { throw 'Existing application WebView2 policy' }
        } elseif ($env:HOI4_E2E_WEBVIEW_MODE -eq 'set') {
          if ($null -ne $existing) { throw 'Application WebView2 policy changed' }
          if ($null -eq $key) { $key = $root.CreateSubKey($path) }
          $key.SetValue($name, $entry.Value, [Microsoft.Win32.RegistryValueKind]::String)
        } elseif ($env:HOI4_E2E_WEBVIEW_MODE -eq 'clear') {
          if ($null -ne $existing -and $existing -cne $entry.Value) { throw 'Refusing unrelated WebView2 policy cleanup' }
          if ($null -ne $existing) { $key.DeleteValue($name, $false) }
        } else { throw 'Unknown WebView2 policy operation' }
      } finally { if ($null -ne $key) { $key.Dispose() } }
    }
  } finally { $root.Dispose() }
}
`;

export async function configureHostedWebView(env) {
  requireHostedWindows();
  const values = { ...env, HOI4_E2E_WEBVIEW_ARGS: env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, HOI4_E2E_WEBVIEW_PROFILE: env.WEBVIEW2_USER_DATA_FOLDER };
  const run = (mode) => exec(resolveSystemPowerShellPath(), ["-NoProfile", "-NonInteractive", "-Command", policyScript], {
    env: { ...values, HOI4_E2E_WEBVIEW_MODE: mode }, windowsHide: true, timeout: 30_000,
  });
  await run("preflight");
  try { await run("set"); } catch (error) { await run("clear"); throw error; }
  return () => run("clear");
}
