// The full dependency audit with reviewed exceptions (maintainer decision D-97).
//
// `pnpm audit --prod --audit-level high` stays strict for what users install. This gate runs the
// full audit (development tools included) and fails on every high or critical advisory, except one
// listed in scripts/audit-exceptions.json for that exact package with a reason, until its review
// date. An exception never covers an advisory that also reaches the production dependencies, and
// it stops counting once the advisory names a patched version. The gate also fails when an
// exception is past its review date or no longer matches any advisory, so none outlives its reason.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ID = /^GHSA(-[23456789cfghjmpqrvwx]{4}){3}$/u;
const DATE = /^\d{4}-\d{2}-\d{2}$/u;

export function parseExceptions(value) {
  if (value === null || typeof value !== "object" || !Array.isArray(value.exceptions)) {
    throw new Error("audit-exceptions.json needs an exceptions array");
  }
  for (const entry of value.exceptions) {
    const ok =
      ID.test(entry?.id ?? "") &&
      typeof entry.package === "string" &&
      entry.package.length > 0 &&
      typeof entry.reason === "string" &&
      entry.reason.length >= 20 &&
      DATE.test(entry.approved ?? "") &&
      DATE.test(entry.reviewBy ?? "") &&
      entry.reviewBy > entry.approved;
    if (!ok) throw new Error(`Invalid audit exception: ${JSON.stringify(entry)}`);
  }
  return value;
}

const key = (id, name) => `${id}\0${name}`;

/** High and critical advisories (or all with `all`) from a `pnpm audit --json` report. */
function advisories(report, all) {
  if (report === null || typeof report !== "object" || typeof report.advisories !== "object") {
    throw new Error("Unreadable pnpm audit report");
  }
  const found = new Map();
  for (const item of Object.values(report.advisories ?? {})) {
    if (!all && item.severity !== "high" && item.severity !== "critical") continue;
    const id =
      item.github_advisory_id ?? /GHSA(-[0-9a-z]{4}){3}/u.exec(item.url ?? "")?.[0] ?? item.url;
    found.set(key(id, item.module_name), {
      id,
      name: item.module_name,
      severity: item.severity,
      range: item.vulnerable_versions ?? "",
      patched: item.patched_versions ?? "<0.0.0",
    });
  }
  return found;
}

/** Decides the gate from the two reports. Pure: the caller runs pnpm. */
export function evaluateAudit({ full, production, today }, exceptions) {
  const found = advisories(full, false);
  const prod = advisories(production, true);
  const failures = [];
  const allowed = [];
  const used = new Set();
  const sorted = [...found.values()].sort((a, b) =>
    `${a.id} ${a.name}`.localeCompare(`${b.id} ${b.name}`),
  );
  for (const advisory of sorted) {
    const k = key(advisory.id, advisory.name);
    const exception = exceptions.exceptions.find((e) => key(e.id, e.package) === k);
    if (exception === undefined) {
      failures.push(
        `${advisory.severity} ${advisory.id} in ${advisory.name} has no reviewed exception`,
      );
      continue;
    }
    used.add(k);
    if (prod.has(k)) {
      failures.push(
        `${advisory.id} in ${advisory.name} reaches the production dependencies; an exception never covers those`,
      );
    } else if (today > exception.reviewBy) {
      failures.push(
        `${advisory.id} in ${advisory.name}: the exception's review date ${exception.reviewBy} has passed; review it again`,
      );
    } else if (advisory.patched !== "<0.0.0") {
      failures.push(
        `${advisory.id} in ${advisory.name}: a fixed version exists (${advisory.patched}); update the dependency and remove the exception`,
      );
    } else {
      allowed.push(
        `${advisory.id} in ${advisory.name} (${advisory.range}, no fixed version; reviewed exception until ${exception.reviewBy})`,
      );
    }
  }
  for (const exception of exceptions.exceptions) {
    if (!used.has(key(exception.id, exception.package))) {
      failures.push(
        `The exception for ${exception.id} in ${exception.package} matches no advisory any more; remove it`,
      );
    }
  }
  return { failures, allowed };
}

function auditJson(args) {
  // pnpm audit exits non-zero whenever it finds anything; the report decides, not the exit code.
  const cli = process.env.npm_execpath;
  if (cli === undefined) throw new Error("Run this through `pnpm audit:gate`");
  const run = spawnSync(process.execPath, [cli, "audit", "--json", ...args], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (run.error !== undefined) throw run.error;
  return JSON.parse(run.stdout);
}

function main() {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const exceptions = parseExceptions(
    JSON.parse(readFileSync(path.join(here, "audit-exceptions.json"), "utf8")),
  );
  const result = evaluateAudit(
    {
      full: auditJson([]),
      production: auditJson(["--prod"]),
      today: new Date().toISOString().slice(0, 10),
    },
    exceptions,
  );
  for (const line of result.allowed) console.log(`allowed: ${line}`);
  for (const line of result.failures) console.error(`audit: ${line}`);
  if (result.failures.length > 0) process.exit(1);
  console.log("audit gate: no high or critical advisory without a reviewed exception");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
