import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { stageVerifiedLocalTarball } from "./mcp_local_tarball.mjs";

const bytes = Buffer.from("synthetic local pack fixture");
const integrity = "sha512-" + createHash("sha512").update(bytes).digest("base64");

test("local pack stages the exact integrity-verified bytes", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "hoi4-mcp-pack-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const input = join(root, "input.tgz");
  const output = join(root, "verified.tgz");
  await writeFile(input, bytes);
  assert.equal(await stageVerifiedLocalTarball(input, integrity, output), output);
  await writeFile(input, "changed after verification");
  assert.deepEqual(await readFile(output), bytes);
});

test("local pack rejects mismatched SHA-512 before staging", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "hoi4-mcp-pack-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const input = join(root, "input.tgz");
  const output = join(root, "verified.tgz");
  await writeFile(input, "wrong pack");
  await assert.rejects(stageVerifiedLocalTarball(input, integrity, output), /SHA-512 integrity/);
  await assert.rejects(readFile(output), { code: "ENOENT" });
});

test("local pack requires an explicit absolute regular file", async (t) => {
  await assert.rejects(stageVerifiedLocalTarball("", integrity, "unused"), /absolute tarball/);
  await assert.rejects(stageVerifiedLocalTarball("relative.tgz", integrity, "unused"), /absolute tarball/);
  const root = await mkdtemp(join(tmpdir(), "hoi4-mcp-pack-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await assert.rejects(stageVerifiedLocalTarball(root, integrity, "unused"), /bounded regular file/);
});
