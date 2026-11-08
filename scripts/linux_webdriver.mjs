import { spawn } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { createServer } from "node:net";

// A dependency-free W3C WebDriver client for the Linux acceptance test.
// Playwright cannot attach to WebKitGTK, so the Linux app is driven through the
// distro's WebKitWebDriver, the driver the Tauri documentation names for Linux.
// Every call goes to the unmodified production application. No IPC is mocked.

const ELEMENT_KEY = "element-6066-11e4-a52e-4f735466cecf";
const pause = (ms) => new Promise((done) => setTimeout(done, ms));

// Injected into the page once. It finds elements by ARIA role and accessible name,
// the way the Windows driver's Playwright locators do. Names are an exact string,
// or {source, flags} for a regular expression.
const PRELUDE = String.raw`
if (!window.__e2e) {
  const ROLES = {
    button: 'button, [role="button"], input[type="button"], input[type="submit"]',
    radio: 'input[type="radio"], [role="radio"]',
    checkbox: 'input[type="checkbox"], [role="checkbox"]',
    switch: '[role="switch"]',
    combobox: 'select, [role="combobox"]',
    radiogroup: '[role="radiogroup"]',
    heading: 'h1, h2, h3, h4, h5, h6, [role="heading"]',
    alert: '[role="alert"]',
    textbox: 'input:not([type]), input[type="text"], textarea, [role="textbox"]',
  };
  const clean = (text) => (text ?? '').replace(/\s+/g, ' ').trim();
  // Text with a space at every element boundary, as an accessible name is computed (textContent would glue blocks together).
  const spaced = (node) => node.nodeType === 3 ? node.data : ' ' + [...node.childNodes].map(spaced).join('') + ' ';
  const nameOf = (el) => {
    const aria = el.getAttribute('aria-label');
    if (aria) return clean(aria);
    const by = el.getAttribute('aria-labelledby');
    if (by) return clean(by.split(/\s+/).map((id) => document.getElementById(id)?.textContent ?? '').join(' '));
    // A label that wraps its control also holds the control's own text (a select's options), which is not its name.
    if (el.labels && el.labels.length) return clean([...el.labels].map((label) => { const copy = label.cloneNode(true); copy.querySelectorAll('select, input, textarea, option').forEach((node) => node.remove()); return spaced(copy); }).join(' '));
    return clean(el.innerText ?? el.textContent);
  };
  const matches = (name, wanted) => {
    if (wanted === undefined || wanted === null) return true;
    if (typeof wanted === 'string') return name === wanted;
    return new RegExp(wanted.source, wanted.flags).test(name);
  };
  const shown = (el) => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length);
  const within = (scope) => (scope ? document.querySelector(scope) ?? document : document);
  window.__e2e = {
    nameOf,
    find(role, name, scope) {
      return [...within(scope).querySelectorAll(ROLES[role] ?? role)].filter((el) => matches(nameOf(el), name));
    },
    byLabel(label, scope) {
      return [...within(scope).querySelectorAll('input, textarea, select')].filter((el) => nameOf(el) === label);
    },
    byText(text) {
      const all = [...document.querySelectorAll('summary, button, label, p, span, div, a, h1, h2, h3, li')];
      return all.filter((el) => clean(el.innerText) === text && ![...el.children].some((child) => clean(child.innerText) === text));
    },
    shown,
    setValue(el, value) {
      const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    },
    choose(el, wanted) {
      const option = [...el.options].find((o) => o.value === wanted || clean(o.textContent) === wanted);
      if (!option) throw new Error('no option ' + wanted);
      el.value = option.value;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    },
  };
}
`;

export async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const port = server.address().port;
  await new Promise((done) => server.close(done));
  return port;
}

export class WebDriverSession {
  constructor(base, id) { this.base = base; this.id = id; this.defaultTimeout = 30_000; }

  static async start({ driverPath = "/usr/bin/WebKitWebDriver", binary, args = [], env, cwd }) {
    const port = await freePort();
    const driver = spawn(driverPath, [`--port=${port}`, "--host=127.0.0.1"], { env, cwd, stdio: ["ignore", "pipe", "pipe"], detached: true });
    // Keep the last few KB of the driver's and the application's output, for a failure report.
    const output = [];
    const capture = (chunk) => { output.push(String(chunk)); while (output.join("").length > 6000) output.shift(); };
    driver.stdout.on("data", capture); driver.stderr.on("data", capture);
    driver.exited = () => driver.exitCode !== null || driver.signalCode !== null;
    driver.tail = () => output.join("").slice(-4000);
    const base = `http://127.0.0.1:${port}`;
    const deadline = Date.now() + 30_000;
    for (;;) {
      try { if ((await fetch(`${base}/status`, { signal: AbortSignal.timeout(1000) })).ok) break; } catch { /* the driver is still starting */ }
      if (driver.exitCode !== null) throw new Error("WebKitWebDriver exited before it was ready.");
      if (Date.now() > deadline) throw new Error("WebKitWebDriver was not ready in time.");
      await pause(250);
    }
    const created = await request(base, "POST", "/session", {
      capabilities: { alwaysMatch: { "webkitgtk:browserOptions": { binary, args } } },
    });
    const session = new WebDriverSession(base, created.sessionId);
    session.driver = driver;
    return session;
  }

  call(method, path, body, timeoutMs) { return request(this.base, method, `/session/${this.id}${path}`, body, timeoutMs); }

  async run(script, ...args) {
    await this.call("POST", "/execute/sync", { script: PRELUDE, args: [] });
    return this.call("POST", "/execute/sync", { script, args });
  }

  async waitUntil(check, milliseconds, label) {
    const deadline = Date.now() + milliseconds;
    let last;
    while (Date.now() < deadline) {
      try { const value = await check(); if (value) return value; last = undefined; } catch (error) { last = error; }
      await pause(500);
    }
    throw new Error(`Timed out: ${label}${last ? ` (${String(last.message).slice(0, 300)})` : ""}`);
  }

  async heading() { return this.run("return document.querySelector('#screen-title')?.innerText ?? null;"); }
  async text() { return this.run("return document.body.innerText;"); }
  async alerts() { return this.run("return [...document.querySelectorAll('[role=alert]')].map((el) => el.innerText);"); }
  // WebKitGTK sometimes does not answer a snapshot request while it is busy rendering. Evidence screenshots are
  // best effort: a missing one is reported and never fails the scenario.
  async screenshot(path) {
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try { await writeFile(path, Buffer.from(await this.call("GET", "/screenshot", undefined, 30_000), "base64")); return true; } catch { /* retry once */ }
    }
    return false;
  }
  // Role and accessible name of every control on the screen, for a failure report.
  async controls() {
    return this.run("return ['button', 'radio', 'checkbox', 'switch', 'combobox', 'textbox'].flatMap((role) => window.__e2e.find(role).filter(window.__e2e.shown).map((el) => role + ': ' + window.__e2e.nameOf(el).slice(0, 80))).slice(0, 60);");
  }
  driverReport() { return { exited: this.driver?.exited?.(), exitCode: this.driver?.exitCode, signal: this.driver?.signalCode, tail: this.driver?.tail?.() }; }
  async url() { return this.call("GET", "/url"); }

  // The first element of a role and name; waits for it to exist.
  async element(role, name, { scope, timeout = this.defaultTimeout } = {}) {
    const found = await this.waitUntil(() => this.run("return window.__e2e.find(arguments[0], arguments[1], arguments[2])[0] ?? null;", role, serialise(name), scope ?? null), timeout, `${role} "${name?.source ?? name}"`);
    return found;
  }
  async has(role, name, scope) { return (await this.run("return window.__e2e.find(arguments[0], arguments[1], arguments[2]).length;", role, serialise(name), scope ?? null)) > 0; }
  async count(role, name, scope) { return this.run("return window.__e2e.find(arguments[0], arguments[1], arguments[2]).length;", role, serialise(name), scope ?? null); }
  async isEnabled(role, name) { return this.run("const el = window.__e2e.find(arguments[0], arguments[1])[0]; return !!el && !el.disabled && el.getAttribute('aria-disabled') !== 'true';", role, serialise(name)); }

  async click(role, name, options) {
    const element = await this.element(role, name, options);
    const text = await this.waitUntil(() => this.run("const el = arguments[0]; el.scrollIntoView({block: 'center'}); return !el.disabled;", element), options?.timeout ?? this.defaultTimeout, `${role} "${name?.source ?? name}" enabled`);
    void text;
    try { await this.call("POST", `/element/${element[ELEMENT_KEY]}/click`, {}); }
    catch { await this.run("arguments[0].click();", element); }
  }
  // Styled radio and checkbox inputs are visually hidden, so activate them as a label click would.
  async check(role, name, checked = true, options) {
    const element = await this.element(role, name, options);
    if ((await this.run("return arguments[0].checked === true || arguments[0].getAttribute('aria-checked') === 'true';", element)) !== checked) await this.run("arguments[0].click();", element);
    if ((await this.run("return arguments[0].checked === true || arguments[0].getAttribute('aria-checked') === 'true';", element)) !== checked) throw new Error(`${role} "${name?.source ?? name}" did not become ${checked ? "checked" : "unchecked"}`);
  }
  async isChecked(role, name, options) {
    const element = await this.element(role, name, options);
    return this.run("return arguments[0].checked === true || arguments[0].getAttribute('aria-checked') === 'true';", element);
  }
  async attribute(role, name, attribute, options) {
    const element = await this.element(role, name, options);
    return this.run("return arguments[0].getAttribute(arguments[1]);", element, attribute);
  }
  async fill(label, value) {
    const element = await this.waitUntil(() => this.run("return window.__e2e.byLabel(arguments[0])[0] ?? null;", label), this.defaultTimeout, `field "${label}"`);
    await this.run("arguments[0].scrollIntoView({block: 'center'}); window.__e2e.setValue(arguments[0], arguments[1]);", element, value);
    const actual = await this.run("return arguments[0].value;", element);
    if (actual !== value) throw new Error(`field "${label}" did not take its value`);
  }
  async selectOption(label, wanted) {
    const element = await this.waitUntil(() => this.run("return window.__e2e.find('combobox', arguments[0])[0] ?? null;", label), this.defaultTimeout, `combobox "${label}"`);
    await this.run("window.__e2e.choose(arguments[0], arguments[1]);", element, wanted);
  }
  async clickText(text) {
    const element = await this.waitUntil(() => this.run("return window.__e2e.byText(arguments[0])[0] ?? null;", text), this.defaultTimeout, `text "${text}"`);
    try { await this.call("POST", `/element/${element[ELEMENT_KEY]}/click`, {}); } catch { await this.run("arguments[0].click();", element); }
  }
  async textVisible(text, timeout = this.defaultTimeout) {
    await this.waitUntil(async () => (await this.text()).includes(text), timeout, `text "${text}"`);
  }

  async close() {
    try { await this.call("DELETE", ""); } catch { /* The application may already have exited. */ }
    if (this.driver?.pid && this.driver.exitCode === null) {
      try { process.kill(-this.driver.pid, "SIGTERM"); } catch { /* already gone */ }
    }
  }
}

function serialise(name) {
  return name instanceof RegExp ? { source: name.source, flags: name.flags } : name ?? null;
}

async function request(base, method, path, body, timeoutMs = 120_000) {
  // One connection per command: WebKitWebDriver closes idle keep-alive connections, which surfaces as "fetch failed".
  const send = () => fetch(`${base}${path}`, {
    method, headers: { "Content-Type": "application/json", Connection: "close" }, body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });
  let response;
  try { response = await send(); } catch (error) {
    if (error?.name === "TimeoutError") throw new Error(`${method} ${path.split("/").slice(3).join("/")}: no answer within ${timeoutMs / 1000} s`);
    await pause(250);
    response = await send();
  }
  const json = await response.json().catch(() => ({}));
  if (!response.ok || json.value?.error) throw new Error(`${method} ${path}: ${json.value?.error ?? response.status}: ${String(json.value?.message ?? "").slice(0, 400)}`);
  return json.value;
}
