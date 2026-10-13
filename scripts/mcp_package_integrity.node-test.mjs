import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, symlink, unlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { isContained, packageFiles } from "./mcp_package_integrity.mjs";

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "hoi4-mcp-integrity-fixture-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "node_modules", "semver", "bin"), { recursive: true });
  await mkdir(join(root, "node_modules", ".bin"), { recursive: true });
  const target = join(root, "node_modules", "semver", "bin", "semver.js");
  const link = join(root, "node_modules", ".bin", "semver");
  await writeFile(target, "reviewed source");
  return { root, target, link, allowed: { "node_modules/.bin/semver": "../semver/bin/semver.js" } };
}

test("temporary containment rejects the root itself and a sibling prefix", () => {
  const root = resolve(tmpdir(), "hoi4-owned");
  assert.equal(isContained(root, join(root, "child")), true);
  assert.equal(isContained(root, root), false);
  assert.equal(isContained(root, root + "-peer/child"), false);
});

test("regular package bytes are measured and changed contents change identity", async (t) => {
  const f = await fixture(t);
  const original = await packageFiles(f.root);
  assert.equal(original.files.length, 1);
  await writeFile(f.target, "changed source");
  assert.notEqual((await packageFiles(f.root)).sha256, original.sha256);
});

const nativeLinks = { skip: process.platform === "win32" ? "Native Linux link fixtures" : false };
test("only exact reviewed link targets enter the symlink sentinel tree", nativeLinks, async (t) => {
  const f = await fixture(t);
  await symlink("../semver/bin/semver.js", f.link);
  const tree = await packageFiles(f.root, f.allowed);
  assert.equal(tree.files.length, 2);
  assert.deepEqual(tree.files.find((file) => file.path === "node_modules/.bin/semver").bytes,
    Buffer.from("symlink\0../semver/bin/semver.js"));
  await assert.rejects(packageFiles(f.root), /unreviewed link/);
  await unlink(f.link);
  await assert.rejects(packageFiles(f.root, f.allowed), /missing a reviewed link/);
});

test("a different literal target is rejected even when it resolves to the same file", nativeLinks, async (t) => {
  const f = await fixture(t);
  await symlink("../semver/bin/../bin/semver.js", f.link);
  await assert.rejects(packageFiles(f.root, f.allowed), /unreviewed link/);
});

test("review declarations cannot authorize an escaping or directory target", nativeLinks, async (t) => {
  const f = await fixture(t);
  await symlink("../../../outside", f.link);
  await assert.rejects(packageFiles(f.root, { "node_modules/.bin/semver": "../../../outside" }), /escapes its tree/);
  await unlink(f.link);
  await symlink("../semver/bin", f.link);
  await assert.rejects(packageFiles(f.root, { "node_modules/.bin/semver": "../semver/bin" }), /invalid regular file/);
});

test("a reviewed link cannot lead through a second link", nativeLinks, async (t) => {
  const f = await fixture(t);
  await unlink(f.target);
  await writeFile(f.target + ".actual", "reviewed source");
  await symlink("semver.js.actual", f.target);
  await symlink("../semver/bin/semver.js", f.link);
  await assert.rejects(packageFiles(f.root, { ...f.allowed, "node_modules/semver/bin/semver.js": "semver.js.actual" }), /target contains a link/);
});
