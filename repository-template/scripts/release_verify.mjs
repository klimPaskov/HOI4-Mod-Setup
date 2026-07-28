import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageMetadata = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const tauriMetadata = JSON.parse(readFileSync(resolve(root, "src-tauri", "tauri.conf.json"), "utf8"));
const cargoMetadata = readFileSync(resolve(root, "src-tauri", "Cargo.toml"), "utf8");
const cargoLock = readFileSync(resolve(root, "Cargo.lock"), "utf8");
const cargoVersion = cargoMetadata.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const cargoPackage = cargoLock.split("[[package]]").find((entry) => /^\s*name\s*=\s*"hoi4-mod-setup"/m.test(entry));
const lockVersion = cargoPackage?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (cargoVersion !== packageMetadata.version || lockVersion !== packageMetadata.version) {
  throw new Error("package.json, src-tauri/Cargo.toml, and Cargo.lock versions must match");
}
const required = [
  resolve(root, "dist", "index.html"),
  resolve(root, "dist", "release", "BUILD_METADATA.json"),
  resolve(root, "dist", "release", "SBOM.cdx.json"),
];
for (const path of required) if (!existsSync(path)) throw new Error(`missing release output: ${path}`);
const metadata = JSON.parse(readFileSync(required[1], "utf8"));
const sbom = JSON.parse(readFileSync(required[2], "utf8"));
if (sbom.bomFormat !== "CycloneDX" || sbom.specVersion !== "1.5" || !Array.isArray(sbom.components) || sbom.components.length === 0) {
  throw new Error("release SBOM is not a populated CycloneDX 1.5 document");
}
const sbomRevision = sbom.metadata?.properties?.find((property) => property.name === "hoi4.mod.setup.source_revision")?.value;
if (metadata.sourceRevision !== "unresolved-local" && sbomRevision !== metadata.sourceRevision) {
  throw new Error("release SBOM source revision does not match release metadata");
}
if (metadata.product !== "HOI4 Mod Setup" || metadata.version !== packageMetadata.version || tauriMetadata.version !== packageMetadata.version) {
  throw new Error("release metadata and configured application versions do not match");
}
const tagVersion = process.env.GITHUB_REF?.startsWith("refs/tags/") && process.env.GITHUB_REF_NAME?.startsWith("v")
  ? process.env.GITHUB_REF_NAME.slice(1)
  : undefined;
if (tagVersion && metadata.version !== tagVersion) {
  throw new Error(`release tag ${process.env.GITHUB_REF_NAME} does not match release metadata ${metadata.version}`);
}
const requestedBundle = process.env.HOI4_MOD_SETUP_BUNDLE;
if (requestedBundle && metadata.bundle !== requestedBundle) {
  throw new Error(`release metadata bundle ${metadata.bundle} does not match requested ${requestedBundle}`);
}
const artifactManifestPath = resolve(root, "dist", "release", metadata.artifact_manifest ?? "ARTIFACTS.sha256");
if (!existsSync(artifactManifestPath)) throw new Error("release artifact manifest is missing");
const artifactManifest = JSON.parse(readFileSync(artifactManifestPath, "utf8"));
if (!Array.isArray(artifactManifest) || artifactManifest.length === 0) throw new Error("release artifact manifest is empty");
const seen = new Set();
for (const artifact of artifactManifest) {
  if (!artifact || typeof artifact.path !== "string" || !/^[^\\/][^:*?"<>|]*$/.test(artifact.path) || seen.has(artifact.path)) {
    throw new Error("release artifact manifest contains an invalid or duplicate path");
  }
  seen.add(artifact.path);
  const absolute = resolve(root, "dist", "release", artifact.path);
  if (!absolute.startsWith(resolve(root, "dist", "release") + (process.platform === "win32" ? "\\" : "/"))) throw new Error("release artifact escaped its root");
  if (!existsSync(absolute)) throw new Error(`missing release artifact: ${artifact.path}`);
  const digest = createHash("sha256").update(readFileSync(absolute)).digest("hex");
  if (digest !== artifact.sha256) throw new Error(`release artifact hash mismatch: ${artifact.path}`);
}
if (process.env.HOI4_MOD_SETUP_REQUIRE_TAURI === "1") {
  if (metadata.frontendOnly) throw new Error("release metadata is frontend-only");
  if (!/^[0-9a-f]{40}$/i.test(metadata.sourceRevision ?? "") || metadata.sourceRevision === "unresolved-local") {
    throw new Error("platform release is missing the exact source revision");
  }
  const gitRevision = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  const checkedOutRevision = gitRevision.status === 0 ? gitRevision.stdout.trim().toLowerCase() : "";
  const metadataRevision = metadata.sourceRevision.toLowerCase();
  if (checkedOutRevision !== metadataRevision) {
    throw new Error(`release metadata revision ${metadataRevision} does not match checked-out HEAD ${checkedOutRevision}`);
  }
  const requireReleaseIdentity = process.env.HOI4_MOD_SETUP_REQUIRE_RELEASE_IDENTITY === "1";
  if (requireReleaseIdentity && !/^[0-9a-f]{40}$/i.test(process.env.GITHUB_SHA?.trim() ?? "")) {
    throw new Error("release verification requires GITHUB_SHA");
  }
  if (process.env.GITHUB_SHA && process.env.GITHUB_SHA.trim().toLowerCase() !== metadataRevision) {
    throw new Error("release metadata revision does not match GITHUB_SHA");
  }
  if (requireReleaseIdentity && (!process.env.GITHUB_REF?.startsWith("refs/tags/") || !process.env.GITHUB_REF_NAME)) {
    throw new Error("release verification requires a tag ref");
  }
  const isTagRef = process.env.GITHUB_REF?.startsWith("refs/tags/") ?? false;
  if (isTagRef) {
    const tag = process.env.GITHUB_REF_NAME;
    if (!/^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
      throw new Error(`release tag ${tag} is not a supported semantic version tag`);
    }
    const tagRevision = spawnSync("git", ["rev-parse", `refs/tags/${tag}^{commit}`], {
      cwd: root,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    if (tagRevision.status !== 0 || tagRevision.stdout.trim().toLowerCase() !== metadataRevision) {
      throw new Error(`release tag ${tag} does not match the checked-out release revision`);
    }
  }
  if (metadata.platform === "unresolved-local" || metadata.architecture === "unresolved-local") {
    throw new Error("platform release is missing runner identity");
  }
  const packageRoot = resolve(root, "dist", "release", "packages");
  if (!existsSync(packageRoot)) throw new Error("Tauri package directory is missing");
  const packages = readdirSync(packageRoot, { recursive: true });
  const platform = String(metadata.platform).toLowerCase();
  const packagePattern = requestedBundle === "nsis"
    ? /\.exe$/i
    : requestedBundle === "dmg"
      ? /\.dmg$/i
      : platform.includes("windows")
        ? /\.(msi|exe)$/i
        : platform.includes("mac") || platform.includes("darwin")
          ? /\.(dmg|app)$/i
          : /\.(msi|dmg|app|exe)$/i;
  const packageFiles = packages.filter((path) => /\.(msi|exe|dmg|app)$/i.test(path));
  if (!packageFiles.some((path) => packagePattern.test(path))) {
    throw new Error(`no ${platform} package was found in the Tauri bundle output`);
  }
  if (requestedBundle && packageFiles.some((path) => !packagePattern.test(path))) {
    throw new Error(`release bundle contains a package outside the requested ${requestedBundle} target`);
  }
  verifyPlatformArchitecture(packageRoot, packageFiles, platform, String(metadata.architecture).toUpperCase());
  if (process.env.HOI4_MOD_SETUP_REQUIRE_SIGNING === "1") {
    if (metadata.signing !== "configured") {
      throw new Error("platform package is unsigned; configure signing before publication");
    }
    verifyPlatformSignatures(packageRoot, packageFiles, platform);
    const evidence = {
      schema_version: "1.0.0",
      source_revision: metadataRevision,
      platform: metadata.platform,
      architecture: metadata.architecture,
      package_sha256: Object.fromEntries(packageFiles.map((path) => [path, createHash("sha256").update(readFileSync(resolve(packageRoot, path))).digest("hex")])),
      method: platform.includes("windows") ? "authenticode" : "codesign-and-stapler",
    };
    const evidencePath = resolve(root, "dist", "release", "SIGNING_VERIFICATION.json");
    writeFileSync(evidencePath, JSON.stringify(evidence, null, 2) + "\n", "utf8");
    const evidenceHash = createHash("sha256").update(readFileSync(evidencePath)).digest("hex");
    const existingEvidence = artifactManifest.find((entry) => entry.path === "SIGNING_VERIFICATION.json");
    if (existingEvidence) existingEvidence.sha256 = evidenceHash;
    else artifactManifest.push({ path: "SIGNING_VERIFICATION.json", sha256: evidenceHash });
    writeFileSync(artifactManifestPath, JSON.stringify(artifactManifest.sort((left, right) => left.path.localeCompare(right.path)), null, 2) + "\n", "utf8");
  }
}
console.log(process.env.HOI4_MOD_SETUP_REQUIRE_TAURI === "1"
  ? `Tauri package verified; signing=${metadata.signing}. Publication remains protected.`
  : "Release frontend artifact verified; platform package verification is an explicit CI step.");

function verifyPlatformArchitecture(packageRoot, packageFiles, platform, architecture) {
  if (!["X64", "ARM64"].includes(architecture)) throw new Error(`unsupported release architecture: ${architecture}`);
  if (platform.includes("windows")) {
    const expectedMachine = architecture === "X64" ? 0x8664 : 0xaa64;
    const nativeExecutable = resolve(root, "target", "release", "hoi4-mod-setup.exe");
    const candidates = [
      ...(existsSync(nativeExecutable) ? [{ label: "target/release/hoi4-mod-setup.exe", path: nativeExecutable, native: true }] : []),
      ...packageFiles.filter((path) => /\.exe$/i.test(path)).map((path) => ({ label: path, path: resolve(packageRoot, path), native: false })),
    ];
    if (candidates.length === 0) throw new Error("Windows release contains no PE executable to inspect");
    for (const candidate of candidates) {
      const bytes = readFileSync(candidate.path);
      if (bytes.length < 0x40) throw new Error(`Windows package is too small to inspect: ${candidate.label}`);
      const peOffset = bytes.readUInt32LE(0x3c);
      if (peOffset + 6 > bytes.length || bytes.toString("ascii", peOffset, peOffset + 4) !== "PE\u0000\u0000") {
        throw new Error(`Windows package is not a PE image: ${candidate.label}`);
      }
      const machine = bytes.readUInt16LE(peOffset + 4);
      const acceptedMachine = candidate.native ? [expectedMachine] : [expectedMachine, 0x014c];
      if (!acceptedMachine.includes(machine)) throw new Error(`Windows package architecture mismatch for ${candidate.label}`);
    }
    return;
  }
  if (platform.includes("mac") || platform.includes("darwin")) {
    const expectedArch = architecture === "ARM64" ? "arm64" : "x86_64";
    for (const relative of packageFiles.filter((path) => /\.dmg$/i.test(path))) {
      const packagePath = resolve(packageRoot, relative);
      const mountPoint = mkdtempSync(resolve(tmpdir(), "hoi4-mod-setup-arch-"));
      let mounted = false;
      try {
        const attach = spawnSync("hdiutil", ["attach", "-nobrowse", "-readonly", "-mountpoint", mountPoint, packagePath], { encoding: "utf8" });
        if (attach.status !== 0) throw new Error(`DMG mount failed for architecture inspection: ${relative}`);
        mounted = true;
        const apps = findAppBundles(mountPoint);
        if (apps.length !== 1) throw new Error(`expected one signed app in ${relative}`);
        const executableRoot = resolve(apps[0], "Contents", "MacOS");
        const executables = readdirSync(executableRoot, { withFileTypes: true }).filter((entry) => entry.isFile());
        if (executables.length !== 1) throw new Error(`expected one macOS executable in ${relative}`);
        const executable = resolve(executableRoot, executables[0].name);
        const lipo = spawnSync("lipo", ["-archs", executable], { encoding: "utf8" });
        const architectures = lipo.stdout.trim().split(/\s+/).filter(Boolean);
        if (lipo.status !== 0 || architectures.length !== 1 || architectures[0] !== expectedArch) {
          throw new Error(`macOS package architecture mismatch for ${relative}`);
        }
      } finally {
        if (mounted) spawnSync("hdiutil", ["detach", mountPoint, "-force"], { stdio: "ignore" });
        rmSync(mountPoint, { recursive: true, force: true });
      }
    }
    return;
  }
  throw new Error(`architecture verification is unsupported on runner platform ${platform}`);
}

function verifyPlatformSignatures(packageRoot, packageFiles, platform) {
  if (platform.includes("windows")) {
    const expectedSubject = process.env.HOI4_MOD_SETUP_WINDOWS_SIGNER;
    if (!expectedSubject) throw new Error("Windows signing identity is not configured");
    for (const relative of packageFiles.filter((path) => /\.exe$/i.test(path))) {
      const packagePath = resolve(packageRoot, relative);
      const script = "$signature = Get-AuthenticodeSignature -LiteralPath $env:HOI4_PACKAGE; if ($signature.Status -ne 'Valid') { exit 1 }; if ($signature.SignerCertificate.Subject -notlike ('*' + $env:HOI4_SIGNER + '*')) { exit 2 }";
      const result = spawnSync("powershell.exe", ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script], {
        env: { ...process.env, HOI4_PACKAGE: packagePath, HOI4_SIGNER: expectedSubject },
        stdio: "ignore",
      });
      if (result.status !== 0) throw new Error(`Authenticode verification failed for ${relative}`);
    }
    return;
  }
  if (platform.includes("mac") || platform.includes("darwin")) {
    const expectedIdentity = process.env.HOI4_MOD_SETUP_MACOS_SIGNING_IDENTITY;
    if (!expectedIdentity) throw new Error("macOS signing identity is not configured");
    for (const relative of packageFiles.filter((path) => /\.dmg$/i.test(path))) {
      const packagePath = resolve(packageRoot, relative);
      const mountPoint = mkdtempSync(resolve(tmpdir(), "hoi4-mod-setup-dmg-"));
      let mounted = false;
      try {
        const attach = spawnSync("hdiutil", ["attach", "-nobrowse", "-readonly", "-mountpoint", mountPoint, packagePath], { encoding: "utf8" });
        if (attach.status !== 0) throw new Error(`DMG mount failed for ${relative}`);
        mounted = true;
        const apps = findAppBundles(mountPoint);
        if (apps.length !== 1) throw new Error(`expected one signed app in ${relative}`);
        const appPath = apps[0];
        const signature = spawnSync("codesign", ["--verify", "--deep", "--strict", "--verbose=2", appPath], { encoding: "utf8" });
        if (signature.status !== 0) throw new Error(`codesign verification failed for ${relative}`);
        const details = spawnSync("codesign", ["-dv", "--verbose=4", appPath], { encoding: "utf8" });
        const detailText = `${details.stdout ?? ""}\n${details.stderr ?? ""}`;
        if (details.status !== 0 || !detailText.includes(`Authority=${expectedIdentity}`)) {
          throw new Error(`macOS signing identity mismatch for ${relative}`);
        }
        const notarization = spawnSync("xcrun", ["stapler", "validate", packagePath], { stdio: "ignore" });
        if (notarization.status !== 0) throw new Error(`notarization validation failed for ${relative}`);
      } finally {
        if (mounted) spawnSync("hdiutil", ["detach", mountPoint, "-force"], { stdio: "ignore" });
        rmSync(mountPoint, { recursive: true, force: true });
      }
    }
    return;
  }
  throw new Error(`signing verification is unsupported on runner platform ${platform}`);
}

function findAppBundles(directory) {
  const matches = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory() && entry.name.endsWith(".app")) matches.push(path);
    else if (entry.isDirectory()) matches.push(...findAppBundles(path));
  }
  return matches;
}
