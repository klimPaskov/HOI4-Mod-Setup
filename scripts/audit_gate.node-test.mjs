import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { evaluateAudit, parseExceptions } from "./audit_gate.mjs";

const exceptions = {
  exceptions: [
    {
      id: "GHSA-fc5q-39xc-vjpf",
      package: "picomatch",
      reason: "Development tools only; no fixed version is published.",
      approved: "2026-11-21",
      reviewBy: "2026-12-05",
    },
  ],
};

function advisory(name, id, severity = "high", patched = "<0.0.0") {
  return {
    module_name: name,
    github_advisory_id: id,
    severity,
    vulnerable_versions: "<=4.0.7",
    patched_versions: patched,
  };
}
const report = (...items) => ({
  advisories: Object.fromEntries(items.map((a, i) => [String(i), a])),
});
const picomatch = advisory("picomatch", "GHSA-fc5q-39xc-vjpf");
const inputs = (over = {}) => ({
  full: report(picomatch),
  production: report(),
  today: "2026-11-21",
  ...over,
});

test("allows a listed dev-only advisory with no fix before its review date", () => {
  const r = evaluateAudit(inputs(), exceptions);
  assert.deepEqual(r.failures, []);
  assert.equal(r.allowed.length, 1);
});

test("fails an unlisted high or critical advisory", () => {
  const full = report(picomatch, advisory("other", "GHSA-2222-3333-4444", "critical"));
  const r = evaluateAudit(inputs({ full }), exceptions);
  assert.deepEqual(r.failures, ["critical GHSA-2222-3333-4444 in other has no reviewed exception"]);
});

test("an exception does not cover the same advisory in another package", () => {
  const full = report(picomatch, advisory("anymatch", "GHSA-fc5q-39xc-vjpf"));
  const r = evaluateAudit(inputs({ full }), exceptions);
  assert.deepEqual(r.failures, ["high GHSA-fc5q-39xc-vjpf in anymatch has no reviewed exception"]);
});

test("never passes a listed advisory that reaches production", () => {
  const r = evaluateAudit(inputs({ production: report(picomatch) }), exceptions);
  assert.deepEqual(r.allowed, []);
  assert.match(r.failures[0], /reaches the production dependencies/u);
});

test("stops counting a listed entry once a fixed version exists", () => {
  const fixed = advisory("picomatch", "GHSA-fc5q-39xc-vjpf", "high", ">=4.0.8");
  const r = evaluateAudit(inputs({ full: report(fixed) }), exceptions);
  assert.deepEqual(r.allowed, []);
  assert.match(r.failures[0], /a fixed version exists \(>=4\.0\.8\)/u);
});

test("fails after the review date and for a stale entry", () => {
  const late = evaluateAudit(inputs({ today: "2026-12-06" }), exceptions);
  assert.match(late.failures[0], /review date 2026-12-05 has passed/u);
  const stale = evaluateAudit(inputs({ full: report() }), exceptions);
  assert.match(stale.failures[0], /matches no advisory any more/u);
});

test("ignores moderate advisories and rejects malformed input", () => {
  const full = report(picomatch, advisory("x", "GHSA-2222-3333-4444", "moderate"));
  assert.deepEqual(evaluateAudit(inputs({ full }), exceptions).failures, []);
  assert.throws(() => evaluateAudit(inputs({ full: { nope: 1 } }), exceptions));
  assert.throws(() =>
    parseExceptions({ exceptions: [{ ...exceptions.exceptions[0], reason: "" }] }),
  );
});

test("the checked-in list is valid", () => {
  parseExceptions(JSON.parse(readFileSync(new URL("./audit-exceptions.json", import.meta.url), "utf8")));
});
