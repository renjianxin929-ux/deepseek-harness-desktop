// Node-based regression tests for the stale session guard's pure decision
// logic (run without a browser). Covers the V0.1 acceptance matrix:
//
//   A. NORMAL_RUNNING_NO_INTERFERENCE
//   B. STALE_CLIENT_BACKEND_COMPLETED
//   C. STALE_CLIENT_BACKEND_FAILED
//   D. BACKEND_UNREACHABLE
//   E. NO_DUPLICATE_EXECUTION
//
// plus the multi-session correctness regressions:
//
//   CURRENT_COMPLETED_OTHER_SESSION_RUNNING   => RECONCILE
//   CURRENT_RUNNING_OTHER_SESSION_COMPLETED   => NOOP
//   THREE_CONCURRENT_SESSIONS_CURRENT_COMPLETED => RECONCILE
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import vm from "node:vm";
import assert from "node:assert";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const guardSrc = readFileSync(
  join(root, "src-tauri", "src", "session_guard.js"),
  "utf8"
);

// Mutable localStorage backing store so `currentSessionId` can be exercised.
const lsStore = {};

function loadGuard() {
  const sandbox = {
    console: { info() {}, error() {}, warn() {}, log() {} },
    setInterval() { return 0; },
    clearInterval() {},
    setTimeout() { return 0; },
    clearTimeout() {},
  };
  sandbox.window = sandbox;
  sandbox.location = { protocol: "about:", hostname: "" };
  sandbox.navigator = { language: "en-US" };
  sandbox.localStorage = {
    getItem(k) { return k in lsStore ? lsStore[k] : null; },
    setItem(k, v) { lsStore[k] = String(v); },
    removeItem(k) { delete lsStore[k]; },
  };
  sandbox.document = {
    readyState: "complete",
    addEventListener() {},
    getElementById() { return null; },
    querySelectorAll() { return []; },
    createElement() {
      return { setAttribute() {}, appendChild() {}, style: {} };
    },
    head: { appendChild() {} },
    documentElement: { lang: "", setAttribute() {}, appendChild() {}, style: {} },
    body: { nodeType: 1, appendChild() {}, removeChild() {} },
  };
  vm.createContext(sandbox);
  vm.runInContext(guardSrc, sandbox, { filename: "session_guard.js" });
  return sandbox;
}

const sandbox = loadGuard();
const {
  STALE_AFTER_MS,
  STOP_RECT_SELECTOR,
  shouldQueryBackend,
  resolveDecision,
  currentSessionId,
  parseSessionList,
  isStopControl,
  isCosmetic,
} = sandbox.window.__HD_GUARD_INTERNALS__;

const RUNNING = true;
const IDLE = false;
const stale = STALE_AFTER_MS; // >= stale window (quiet long enough)
const fresh = STALE_AFTER_MS - 1; // recent progress

const RUNNING_TRUE = { reachable: true, currentRunning: true };
const RUNNING_FALSE = { reachable: true, currentRunning: false };
const RUNNING_UNKNOWN = { reachable: true, currentRunning: null };
const UNREACHABLE = { reachable: false, currentRunning: null };

// ---------------------------------------------------------------------------
// A. NORMAL_RUNNING_NO_INTERFERENCE
//    client running + current session genuinely running => noop.
// ---------------------------------------------------------------------------
assert.strictEqual(shouldQueryBackend(RUNNING, fresh, STALE_AFTER_MS), false,
  "recent progress must not warrant a backend read");
assert.strictEqual(resolveDecision(RUNNING, fresh, STALE_AFTER_MS, RUNNING_TRUE), "noop");
// Even when quiet longer than the window, backend truth (still running) wins:
assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, RUNNING_TRUE), "noop");
// Idle client never triggers anything either:
assert.strictEqual(resolveDecision(IDLE, stale, STALE_AFTER_MS, RUNNING_FALSE), "noop");

// ---------------------------------------------------------------------------
// B. STALE_CLIENT_BACKEND_COMPLETED
//    client running + long quiet + current session completed => reconcile.
// ---------------------------------------------------------------------------
assert.strictEqual(shouldQueryBackend(RUNNING, stale, STALE_AFTER_MS), true);
assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, RUNNING_FALSE), "reconcile");

// ---------------------------------------------------------------------------
// C. STALE_CLIENT_BACKEND_FAILED
//    backend exposes failed/completed/cancelled uniformly as `running:false`
//    for the current session, so the guard maps them all to reconcile (recover
//    the real state) and never re-executes the task.
// ---------------------------------------------------------------------------
assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, RUNNING_FALSE), "reconcile");

// ---------------------------------------------------------------------------
// D. BACKEND_UNREACHABLE
//    client running + long quiet + backend unreadable => reconnect banner,
//    never kill/restart/cancel.
// ---------------------------------------------------------------------------
assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, UNREACHABLE), "reconnect");
assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, null), "reconnect");
assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, undefined), "reconnect");

// ---------------------------------------------------------------------------
// Multi-session correctness regressions.
// ---------------------------------------------------------------------------

// CURRENT_COMPLETED_OTHER_SESSION_RUNNING => RECONCILE
// Session A is the current (stale running) session and is completed on the
// backend, while unrelated sessions B and C are genuinely running. The guard
// must reconcile A instead of being suppressed by B/C.
{
  const items = [
    { sessionId: "A", running: false },
    { sessionId: "B", running: true },
    { sessionId: "C", running: true },
  ];
  const truth = parseSessionList({ result: { ok: true, value: { items } } }, "A");
  assert.deepStrictEqual(JSON.parse(JSON.stringify(truth)), { reachable: true, currentRunning: false });
  assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, truth), "reconcile");
}

// CURRENT_RUNNING_OTHER_SESSION_COMPLETED => NOOP
// Session A is the current session and is genuinely running; B/C are completed.
// The guard must NOT reconcile (never interfere with a genuinely running turn).
{
  const items = [
    { sessionId: "A", running: true },
    { sessionId: "B", running: false },
    { sessionId: "C", running: false },
  ];
  const truth = parseSessionList({ result: { ok: true, value: { items } } }, "A");
  assert.deepStrictEqual(JSON.parse(JSON.stringify(truth)), { reachable: true, currentRunning: true });
  assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, truth), "noop");
}

// THREE_CONCURRENT_SESSIONS_CURRENT_COMPLETED => RECONCILE
{
  const items = [
    { sessionId: "A", running: false },
    { sessionId: "B", running: true },
    { sessionId: "C", running: true },
  ];
  const truth = parseSessionList({ result: { ok: true, value: { items } } }, "A");
  assert.strictEqual(resolveDecision(RUNNING, stale, STALE_AFTER_MS, truth), "reconcile");
}

// ---------------------------------------------------------------------------
// E. NO_DUPLICATE_EXECUTION
//    every decision is one of {noop, reconcile, reconnect}; none may re-submit,
//    cancel, stop, restart, or create. Also assert the raw script never touches
//    any mutating official API.
// ---------------------------------------------------------------------------
const ALLOWED_ACTIONS = new Set(["noop", "reconcile", "reconnect"]);
for (const clientRunning of [true, false]) {
  for (const age of [0, STALE_AFTER_MS - 1, STALE_AFTER_MS, STALE_AFTER_MS * 3]) {
    for (const backend of [RUNNING_TRUE, RUNNING_FALSE, RUNNING_UNKNOWN, UNREACHABLE, null]) {
      const d = resolveDecision(clientRunning, age, STALE_AFTER_MS, backend);
      assert.ok(ALLOWED_ACTIONS.has(d), `unexpected decision: ${d}`);
    }
  }
}

const FORBIDDEN = [
  "session.prompt",
  "session.cancel",
  "session.create",
  "session.fork",
  "session.updateQueue",
  "session.attachment",
  "session.selectModel",
  "session.rename",
  "__TAURI__",
];
for (const token of FORBIDDEN) {
  assert.ok(
    !guardSrc.includes(token),
    `guard source must not contain forbidden operation: ${token}`
  );
}
// The only backend query is the read-only session list.
assert.ok(guardSrc.includes('"/api/session.list"'));
assert.ok(guardSrc.includes('method: "session.list"'));
// Per-current-session truth, never the global any-running heuristic.
assert.ok(guardSrc.includes('"dsh.sessions.current"'));
assert.ok(!guardSrc.includes("anyRunning"));

// ---------------------------------------------------------------------------
// currentSessionId / parseSessionList / isStopControl / isCosmetic unit checks
// ---------------------------------------------------------------------------
// Cross-realm note: vm-context return values carry that realm's prototype.
const plain = (x) => JSON.parse(JSON.stringify(x));

// currentSessionId reads the official persisted selection.
delete lsStore["dsh.sessions.current"];
assert.strictEqual(currentSessionId(), null, "no persisted selection => null");

lsStore["dsh.sessions.current"] = JSON.stringify({ sessionId: "A" });
assert.strictEqual(currentSessionId(), "A");

lsStore["dsh.sessions.current"] = JSON.stringify({
  sessionId: "P",
  subagentAddress: { parentSessionId: "P", childSessionId: "C", mode: "child" },
});
assert.strictEqual(currentSessionId(), "C", "subagent child id wins when addressed");

lsStore["dsh.sessions.current"] = "not-json";
assert.strictEqual(currentSessionId(), null, "unparseable selection => null");
delete lsStore["dsh.sessions.current"];

// parseSessionList reduces to the current session's running state.
assert.deepStrictEqual(
  plain(parseSessionList(
    { result: { ok: true, value: { items: [{ sessionId: "s1", running: false }] } } },
    "s1"
  )),
  { reachable: true, currentRunning: false }
);
assert.deepStrictEqual(
  plain(parseSessionList(
    { result: { ok: true, value: { items: [{ sessionId: "s1", running: true }] } } },
    "s1"
  )),
  { reachable: true, currentRunning: true }
);
// Current session absent from the catalog => not running (stale/removed).
assert.deepStrictEqual(
  plain(parseSessionList({ result: { ok: true, value: { items: [] } } }, "s1")),
  { reachable: true, currentRunning: false }
);
// Unknown current id => cannot determine truth (guard stays conservative).
assert.deepStrictEqual(
  plain(parseSessionList(
    { result: { ok: true, value: { items: [{ sessionId: "s1", running: true }] } } },
    null
  )),
  { reachable: true, currentRunning: null }
);
// Malformed / failed envelopes are "unreadable", never "not running":
assert.deepStrictEqual(
  plain(parseSessionList({ result: { ok: false } }, "s1")),
  { reachable: false, currentRunning: null }
);
assert.deepStrictEqual(plain(parseSessionList({}, "s1")), { reachable: false, currentRunning: null });
assert.deepStrictEqual(plain(parseSessionList(null, "s1")), { reachable: false, currentRunning: null });

const stopButton = {
  disabled: false,
  querySelector: (sel) => (sel === STOP_RECT_SELECTOR ? { nodeType: 1 } : null),
};
const sendButton = {
  disabled: false,
  querySelector: () => null,
};
const disabledButton = {
  disabled: true,
  querySelector: () => ({ nodeType: 1 }),
};
assert.strictEqual(isStopControl(stopButton), true);
assert.strictEqual(isStopControl(sendButton), false);
assert.strictEqual(isStopControl(disabledButton), false);
assert.strictEqual(isStopControl(null), false);

const body = sandbox.document.body;
assert.strictEqual(
  isCosmetic({ nodeType: 1, matches: () => true, parentNode: body }),
  true
);
assert.strictEqual(
  isCosmetic({ nodeType: 1, matches: () => false, parentNode: body }),
  false
);
// A text node under a cosmetic element is cosmetic (elapsed-clock tick):
assert.strictEqual(
  isCosmetic({ nodeType: 3, parentNode: { nodeType: 1, matches: () => true, parentNode: body } }),
  true
);

console.log("session_guard.test.mjs: all assertions passed");
