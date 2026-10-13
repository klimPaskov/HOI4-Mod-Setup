import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, readlink, readdir, realpath } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";

export function isContained(root, path) {
  const value = relative(root, path);
  return value !== "" && value !== ".." && !value.startsWith("../") && !value.startsWith("..\\") && !isAbsolute(value);
}

async function regularBytes(path, maximum) {
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > maximum) {
    throw new Error("The MCP package contains an invalid regular file.");
  }
  const handle = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const opened = await handle.stat();
    if (!opened.isFile() || opened.size > maximum) throw new Error("An MCP package file exceeds its size bound.");
    const bytes = await handle.readFile();
    if (bytes.length > maximum) throw new Error("An MCP package file exceeds its size bound.");
    return bytes;
  } finally {
    await handle.close();
  }
}

export async function packageFiles(packageRoot, symlinks = {}) {
  if ((await lstat(packageRoot)).isSymbolicLink()) throw new Error("The MCP package root contains a link.");
  const root = await realpath(packageRoot);
  const files = [];
  const seen = new Set();
  let totalBytes = 0;
  const pending = [root];
  while (pending.length) {
    const directory = pending.pop();
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const absolute = join(directory, entry.name);
      const path = relative(root, absolute).replaceAll("\\", "/");
      const metadata = await lstat(absolute);
      let bytes;
      if (metadata.isSymbolicLink()) {
        const target = await readlink(absolute);
        if (symlinks[path] !== target) throw new Error("The MCP package contains an unreviewed link.");
        const destination = resolve(dirname(absolute), target);
        if (!isContained(root, destination)) throw new Error("The MCP package link escapes its tree.");
        let component = root;
        for (const segment of relative(root, destination).split(/[\\/]/)) {
          component = join(component, segment);
          if ((await lstat(component)).isSymbolicLink()) throw new Error("The MCP package link target contains a link.");
        }
        await regularBytes(destination, 32 * 1024 * 1024);
        seen.add(path);
        bytes = Buffer.concat([Buffer.from("symlink\0", "utf8"), Buffer.from(target, "utf8")]);
      } else if (metadata.isDirectory()) {
        pending.push(absolute);
        continue;
      } else if (metadata.isFile()) {
        bytes = await regularBytes(absolute, 32 * 1024 * 1024);
      } else {
        throw new Error("The MCP package contains a special file.");
      }
      totalBytes += bytes.length;
      if (totalBytes > 512 * 1024 * 1024) throw new Error("The MCP package exceeds its total size bound.");
      files.push({ path, bytes });
      if (files.length > 10_000) throw new Error("The MCP package exceeds its file-count bound.");
    }
  }
  if (Object.keys(symlinks).some((path) => !seen.has(path))) throw new Error("The MCP package is missing a reviewed link.");
  files.sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
  const digest = createHash("sha256");
  for (const file of files) {
    digest.update(file.path, "utf8");
    digest.update(Buffer.from([0]));
    digest.update(String(file.bytes.length), "ascii");
    digest.update(Buffer.from([0]));
    digest.update(file.bytes);
  }
  return { files, sha256: digest.digest("hex") };
}
