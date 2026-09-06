import assert from "node:assert/strict";
import test from "node:test";
import { Script } from "node:vm";
import { hostileFixture } from "./native-browser-smoke-server.mjs";

const deny = () => Promise.reject(new Error("Command not allowed"));

// Execute the JavaScript actually served to the hostile native webview. These
// tests check the fixture's verdicts, not the operating system security boundary.
async function runFixture({ invoke = deny, bridgeAvailable = true, notification } = {}) {
  const script = hostileFixture().match(/<script>([\s\S]*?)<\/script>/)?.[1];
  assert.ok(script, "the fixture must serve its executable security checks");
  const summary = { textContent: "Running…" };
  const output = { textContent: "" };
  const reports = [];
  const context = {
    document: {
      querySelector: (selector) => selector === "#summary" ? summary : output,
    },
    window: {
      __TAURI_INTERNALS__: bridgeAvailable ? { invoke } : undefined,
      open: () => null,
      dispatchEvent: () => undefined,
    },
    location: { href: "http://127.0.0.1:43210/" },
    navigator: {},
    localStorage: {
      getItem: () => null,
      setItem: () => undefined,
    },
    Notification: { requestPermission: notification ?? (() => Promise.resolve("denied")) },
    KeyboardEvent: class KeyboardEvent {},
    fetch: async (_url, options) => {
      reports.push(JSON.parse(options.body));
      return { ok: true };
    },
    // Let unresolved operations expire through the real timeout path quickly.
    setTimeout: (callback) => setTimeout(callback, 5),
    clearTimeout,
  };

  await new Script(script).runInNewContext(context);
  return { summary: summary.textContent, reports };
}

test("native smoke reports passed automated checks only after actual command rejections", async () => {
  const result = await runFixture();
  const commands = result.reports.filter(({ kind }) => kind === "command-result");

  assert.equal(commands.length, 14);
  assert.ok(commands.every(({ outcome }) => outcome === "rejected"));
  assert.equal(result.reports.at(-1).outcome, "passed");
  assert.match(result.summary, /^Automated checks passed/);
  assert.match(result.summary, /native manual checks still required/);
});

test("an unresolved command times out and cannot produce a passed smoke summary", async () => {
  const result = await runFixture({
    invoke: (command) => command === "daemon_request" ? new Promise(() => {}) : deny(),
  });
  const timedOut = result.reports.find(({ kind, command }) =>
    kind === "command-result" && command === "daemon_request",
  );

  assert.equal(timedOut.outcome, "timeout");
  assert.equal(result.reports.at(-1).outcome, "failed");
  assert.deepEqual(result.reports.at(-1).failedChecks, ["command-result"]);
  assert.match(result.summary, /^Smoke failed/);
  assert.doesNotMatch(result.summary, /checks passed/);
});

test("an absent Tauri bridge is a failed check, not a set of rejected commands", async () => {
  const result = await runFixture({ bridgeAvailable: false });

  assert.equal(result.reports.find(({ kind }) => kind === "tauri-internals").available, false);
  assert.equal(result.reports.filter(({ kind }) => kind === "command-result").length, 0);
  assert.equal(result.reports.at(-1).outcome, "failed");
  assert.deepEqual(result.reports.at(-1).failedChecks, ["tauri-internals"]);
  assert.match(result.summary, /^Smoke failed/);
});

test("an unexpectedly resolved command fails the smoke summary", async () => {
  const result = await runFixture({
    invoke: (command) => command === "browser_surface_close" ? Promise.resolve() : deny(),
  });

  assert.equal(result.reports.at(-1).outcome, "failed");
  assert.deepEqual(result.reports.at(-1).failedChecks, ["command-result"]);
  assert.match(result.summary, /^Smoke failed/);
});

test("an unresolved notification permission times out and fails instead of hanging", async () => {
  const result = await runFixture({ notification: () => new Promise(() => {}) });

  assert.equal(result.reports.find(({ kind }) => kind === "permissions").notification, "timeout");
  assert.equal(result.reports.at(-1).outcome, "failed");
  assert.deepEqual(result.reports.at(-1).failedChecks, ["permissions"]);
  assert.match(result.summary, /^Smoke failed/);
});
