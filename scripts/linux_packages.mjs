import { chmodSync, copyFileSync, existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";

export function parseNativeBundles(value) {
  if (!value) return [];
  const bundles = value.split(",");
  if (bundles.some((bundle) => !["nsis", "dmg", "appimage", "deb"].includes(bundle)) || new Set(bundles).size !== bundles.length
    || (bundles.length > 1 && bundles.some((bundle) => !["appimage", "deb"].includes(bundle)))) {
    throw new Error(`unsupported native bundle target: ${value}`);
  }
  return bundles;
}

export function verifyElfBytes(bytes, architecture = "X64", label = "Linux executable") {
  const machine = architecture === "X64" ? 62 : architecture === "ARM64" ? 183 : null;
  if (!machine) throw new Error(`unsupported Linux architecture: ${architecture}`);
  if (bytes.length < 64 || !bytes.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))
    || bytes[4] !== 2 || bytes[5] !== 1 || bytes[6] !== 1 || bytes.readUInt16LE(52) !== 64
    || ![2, 3].includes(bytes.readUInt16LE(16))) {
    throw new Error(`${label} is not a complete little-endian ELF64 executable`);
  }
  if (bytes.readUInt16LE(18) !== machine) throw new Error(`${label} ELF architecture mismatch`);
}

export function verifyAppImageBytes(bytes, architecture = "X64") {
  verifyElfBytes(bytes, architecture, "AppImage runtime");
  if (bytes[8] !== 0x41 || bytes[9] !== 0x49 || bytes[10] !== 2) {
    throw new Error("Linux AppImage is missing its type-2 format marker");
  }
}

export function verifyDebianIdentity(fields, version, architecture = "X64") {
  const expectedArch = architecture === "X64" ? "amd64" : architecture === "ARM64" ? "arm64" : null;
  const [name, actualVersion, actualArch] = fields.trim().split(/\r?\n/);
  if (name !== "hoi4-mod-setup" || actualVersion !== version || !expectedArch || actualArch !== expectedArch) {
    throw new Error("Debian package name, version, or architecture does not match release metadata");
  }
  return name;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 120_000, maxBuffer: 16 * 1024 * 1024, ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed while inspecting Linux package: ${(result.stderr ?? "").slice(0, 4096)}`);
  return result.stdout ?? "";
}

export function verifyLinuxPackage(packagePath, version, architecture = "X64") {
  if (process.platform !== "linux") throw new Error("Linux package inspection requires a Linux runner");
  const temporary = mkdtempSync(join(tmpdir(), "hoi4-mod-setup-package-check-"));
  try {
    let payload;
    if (/\.AppImage$/i.test(packagePath)) {
      verifyAppImageBytes(readFileSync(packagePath), architecture);
      const image = join(temporary, "package.AppImage");
      copyFileSync(packagePath, image);
      chmodSync(image, 0o700);
      run(image, ["--appimage-extract"], { cwd: temporary });
      payload = join(temporary, "squashfs-root");
      const appRun = join(payload, "AppRun");
      if (!existsSync(appRun) || !realpathSync(appRun).startsWith(realpathSync(payload) + sep)) throw new Error("AppImage has no root-contained AppRun entry point");
    } else if (/\.deb$/i.test(packagePath)) {
      // Individual fields avoid dpkg-deb's labelled multi-field output.
      const fields = ["Package", "Version", "Architecture"].map((field) => run("/usr/bin/dpkg-deb", ["--field", packagePath, field]).trim()).join("\n");
      verifyDebianIdentity(fields, version, architecture);
      payload = join(temporary, "payload");
      run("/usr/bin/dpkg-deb", ["--extract", packagePath, payload]);
    } else {
      throw new Error("unsupported Linux package extension");
    }
    const executable = resolve(payload, "usr", "bin", "hoi4-mod-setup");
    if (!existsSync(executable) || !realpathSync(executable).startsWith(realpathSync(payload) + sep)) {
      throw new Error("Linux package does not contain its root-contained application executable");
    }
    verifyElfBytes(readFileSync(executable), architecture, "Packaged Linux application");
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}
