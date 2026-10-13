import assert from "node:assert/strict";
import test from "node:test";
import { parseNativeBundles, verifyAppImageBytes, verifyDebianIdentity, verifyElfBytes } from "./linux_packages.mjs";
import { launchAndStop } from "./native_smoke.mjs";
import { withLinuxDebInstallation } from "./linux_installer_registry.mjs";

function elf(machine = 62) {
  const bytes = Buffer.alloc(64);
  bytes.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]);
  bytes.writeUInt16LE(3, 16);
  bytes.writeUInt16LE(machine, 18);
  bytes.writeUInt16LE(64, 52);
  return bytes;
}

test("bundle selection accepts both Linux packages and rejects duplicate, foreign, or unsafe paths", () => {
  assert.deepEqual(parseNativeBundles("appimage,deb"), ["appimage", "deb"]);
  for (const value of ["appimage,appimage", "nsis,deb", "dmg,nsis", "deb,", "../deb", "deb,../../target", "appimage, deb", "rpm"]) {
    assert.throws(() => parseNativeBundles(value), /unsupported native bundle/);
  }
});

test("ELF validation rejects architecture mismatch, object files, truncation, class, and endianness", () => {
  verifyElfBytes(elf());
  verifyElfBytes(elf(183), "ARM64");
  assert.throws(() => verifyElfBytes(elf(183)), /architecture mismatch/);
  assert.throws(() => verifyElfBytes(elf().subarray(0, 32)), /ELF64/);
  for (const [offset, value] of [[4, 1], [5, 2], [6, 0], [16, 1], [52, 0]]) {
    const bytes = elf(); bytes[offset] = value;
    assert.throws(() => verifyElfBytes(bytes), /ELF64/);
  }
});

test("ordinary ELF bytes cannot masquerade as an AppImage", () => {
  assert.throws(() => verifyAppImageBytes(elf()), /type-2/);
  const bytes = elf(); bytes.set([0x41, 0x49, 2], 8);
  verifyAppImageBytes(bytes);
  bytes[10] = 1;
  assert.throws(() => verifyAppImageBytes(bytes), /type-2/);
});

test("Debian identity binds product, version, and native architecture", () => {
  assert.equal(verifyDebianIdentity("hoi4-mod-setup\n0.5.0\namd64\n", "0.5.0"), "hoi4-mod-setup");
  for (const fields of ["other\n0.5.0\namd64", "hoi4-mod-setup\n0.4.0\namd64", "hoi4-mod-setup\n0.5.0\narm64"]) {
    assert.throws(() => verifyDebianIdentity(fields, "0.5.0"), /does not match/);
  }
});

test("native smoke fails startup when the application exits early", async () => {
  await assert.rejects(launchAndStop(process.execPath, { args: ["-e", "process.exit(7)"], observationMs: 250 }), /exited during startup/);
});

test("native smoke terminates a supervised long-running process", async () => {
  await launchAndStop(process.execPath, { args: ["-e", "setInterval(() => {}, 1000)"], observationMs: 250 });
});

test("dedicated Debian smoke refuses existing product state and fails closed on unreadable state", async () => {
  for (const situation of ["registered", "binary", "query failure"]) {
    let mutated = false;
    const adapter = {
      exists: () => situation === "binary",
      exec: async (command) => {
        if (command !== "/usr/bin/dpkg-query") { mutated = true; throw new Error("unexpected mutation"); }
        if (situation === "query failure") throw new Error("unreadable package database");
        return { stdout: situation === "registered" ? "hoi4-mod-setup\tinstalled\n" : "unrelated\tinstalled\n" };
      },
    };
    await assert.rejects(withLinuxDebInstallation("/temporary/setup.deb", async () => {}, adapter), /existing|unreadable/);
    assert.equal(mutated, false);
  }
});

test("dedicated Debian smoke purges partial installation and verifies absence after launch failure", async () => {
  const commands = [];
  let registered = false;
  await assert.rejects(withLinuxDebInstallation("/temporary/setup.deb", async () => { throw new Error("launch failed"); }, {
    exists: () => registered,
    exec: async (command, args) => {
      commands.push([command, args]);
      if (command === "/usr/bin/dpkg-query") return { stdout: registered ? "hoi4-mod-setup\tinstalled\n" : "unrelated\tinstalled\n" };
      if (args.includes("--install")) registered = true;
      if (args.includes("--purge")) registered = false;
      return { stdout: "" };
    },
  }), /launch failed/);
  assert.equal(registered, false);
  assert.equal(commands.filter(([, args]) => args.includes("--purge")).length, 1);
  assert.equal(commands.at(-1)[0], "/usr/bin/dpkg-query");
});

test("dedicated Debian smoke rejects incomplete purge and preserves ordinary failure evidence", async () => {
  let attempted = false;
  await assert.rejects(withLinuxDebInstallation("/temporary/setup.deb", async () => {}, {
    exists: () => false,
    exec: async (command, args) => {
      if (command === "/usr/bin/dpkg-query") return { stdout: attempted ? "hoi4-mod-setup\tinstalled\n" : "" };
      if (args.includes("--install")) { attempted = true; throw new Error("partial installation"); }
      return { stdout: "" };
    },
  }), (error) => error instanceof AggregateError && error.errors.some((failure) => /partial installation/.test(failure.message)) && error.errors.some((failure) => /left.*behind/.test(failure.message)));
});
