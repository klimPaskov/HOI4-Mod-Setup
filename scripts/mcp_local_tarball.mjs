import { createHash } from "node:crypto";
import { lstat, readFile, writeFile } from "node:fs/promises";
import { isAbsolute } from "node:path";

// Only the opt-in smoke test uses local packs. The application and bootstrap
// retain their published-registry checks.
export async function stageVerifiedLocalTarball(input, expectedIntegrity, destination) {
  if (!input || !isAbsolute(input)) {
    throw new Error("HOI4_MCP_LOCAL_TARBALL must name an absolute tarball path.");
  }
  const metadata = await lstat(input);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 512 * 1024 * 1024) {
    throw new Error("The local MCP tarball must be a bounded regular file.");
  }
  const bytes = await readFile(input);
  if (bytes.length > 512 * 1024 * 1024) {
    throw new Error("The local MCP tarball exceeds its size bound.");
  }
  const integrity = "sha512-" + createHash("sha512").update(bytes).digest("base64");
  if (integrity !== expectedIntegrity) {
    throw new Error("Local MCP tarball SHA-512 integrity does not match the bundled manifest.");
  }
  // npm reads this exact verified buffer, not a later reopening of the input.
  await writeFile(destination, bytes, { flag: "wx" });
  return destination;
}
