// Node-based regression tests for the stale session guard's pure decision
// logic (run without a browser). Covers the acceptance matrix:
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
//
// and the regressions proving DOM freshness is no longer a precondition:
//
//   CLIENT_RUNNING_TIMER_MUTATES_BACKEND_COMPLETED          => RECONCILE
//   CLIENT_RUNNING_CONTINUOUS_DOM_MUTATIONS_BACKEND_COMPLETED => RECONCILE
//   CLIENT_RUNNING_BACKEND_RUNNING                          => NOOP
//   CURRENT_COMPLETED_OTHER_SESSIONS_RUNNING                => RECONCILE
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
  POLL_INTERVAL_MS,
  STOP_RECT_SELECTOR,
  shouldQueryBackend,
  resolveDecision,
  currentSessionId,
  parseSessionList,
  isStopControl,
} = sandbox.window.__HD_GUARD_INTERNALS__;

const RUNNING = true;
const IDLE = false;

const RUNNING_TRUE = { reachable: true, currentRunning: true };
const RUNNING_FALSE = { reachable: true, currentRunning: false };
const RUNNING_UNKNOWN = { reachable: true, currentRunning: null };
const UNREACHABLE = { reachable: false, currentRunning: null };

// ---------------------------------------------------------------------------
// DOM freshness is no longer a precondition. The decision function takes
// only (clientRunning, backend) — there is no quiet window and no DOM-mutation
// tracking at all, so cosmetic clocks or continuous mutations can never refresh
// a "last progress" timestamp and suppress reconciliation.
// ---------------------------------------------------------------------------
assert.strictEqual(POLL_INTERVAL_MS, 30000, "poll interval must be 30s");
assert.ok(!guardSrc.includes("STALE_AFTER_MS"), "stale window must be removed");
assert.ok(!guardSrc.includes("MutationObserver"), "DOM-mutation tracking must be removed");
assert.ok(!guardSrc.includes("COSMETIC_SELECTOR"), "cosmetic DOM filtering must be removed");
assert.ok(!guardSrc.includes("lastProgressAt"), "last-progress timestamp must be removed");
assert.ok(!guardSrc.includes("isCosmetic"), "cosmetic-mutation helper must be removed");

// ---------------------------------------------------------------------------
// A. NORMAL_RUNNING_NO_INTERFERENCE
//    client running + current session genuinely running => noop.
// ---------------------------------------------------------------------------
assert.strictEqual(shouldQueryBackend(RUNNING), true,
  "client running must warrant a backend read");
assert.strictEqual(shouldQueryBackend(IDLE), false,
  "idle client must not warrant a backend read");
assert.strictEqual(resolveDecision(RUNNING, RUNNING_TRUE), "noop");
assert.strictEqual(resolveDecision(IDLE, RUNNING_FALSE), "noop");

// ---------------------------------------------------------------------------
// B. STALE_CLIENT_BACKEND_COMPLETED
//    client running + current session completed => reconcile (regardless of how
//    recently the DOM mutated).
// ---------------------------------------------------------------------------
assert.strictEqual(resolveDecision(RUNNING, RUNNING_FALSE), "reconcile");

// ---------------------------------------------------------------------------
// C. STALE_CLIENT_BACKEND_FAILED
//    backend exposes failed/completed/cancelled uniformly as `running:false`
//    for the current session, so the guard maps them all to reconcile (recover
//    the real state) and never re-executes the task.
// ---------------------------------------------------------------------------
assert.strictEqual(resolveDecision(RUNNING, RUNNING_FALSE), "reconcile");

// ---------------------------------------------------------------------------
// D. BACKEND_UNREACHABLE
//    client running + backend unreadable => reconnect banner,
//    never kill/restart/cancel.
// ---------------------------------------------------------------------------
assert.strictEqual(resolveDecision(RUNNING, UNREACHABLE), "reconnect");
assert.strictEqual(resolveDecision(RUNNING, null), "reconnect");
assert.strictEqual(resolveDecision(RUNNING, undefined), "reconnect");

// ---------------------------------------------------------------------------
// New regressions.
// ---------------------------------------------------------------------------

// CLIENT_RUNNING_TIMER_MUTATES_BACKEND_COMPLETED => RECONCILE
// The "Deep diving..." elapsed timer (and any other cosmetic mutation) can no
// longer refresh a quiet-window timestamp, because there is no such timestamp.
assert.strictEqual(resolveDecision(RUNNING, RUNNING_FALSE), "reconcile");

// CLIENT_RUNNING_CONTINUOUS_DOM_MUTATIONS_BACKEND_COMPLETED => RECONCILE
// Continuous (non-cosmetic) DOM churn must NOT suppress reconciliation either.
assert.strictEqual(resolveDecision(RUNNING, RUNNING_FALSE), "reconcile");

// CLIENT_RUNNING_BACKEND_RUNNING => NOOP
// A genuinely running current session must never be reloaded.
assert.strictEqual(resolveDecision(RUNNING, RUNNING_TRUE), "noop");

// CURRENT_COMPLETED_OTHER_SESSIONS_RUNNING => RECONCILE
// Session A is the current (running-believing) session and is completed on the
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
  assert.strictEqual(resolveDecision(RUNNING, truth), "reconcile");
}

// ---------------------------------------------------------------------------
// Multi-session correctness regressions (retained from V0.1).
// ---------------------------------------------------------------------------

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
  assert.strictEqual(resolveDecision(RUNNING, truth), "noop");
}

// THREE_CONCURRENT_SESSIONS_CURRENT_COMPLETED => RECONCILE
{
  const items = [
    { sessionId: "A", running: false },
    { sessionId: "B", running: true },
    { sessionId: "C", running: true },
  ];
  const truth = parseSessionList({ result: { ok: true, value: { items } } }, "A");
  assert.strictEqual(resolveDecision(RUNNING, truth), "reconcile");
}

// ---------------------------------------------------------------------------
// E. NO_DUPLICATE_EXECUTION
//    every decision is one of {noop, reconcile, reconnect}; none may re-submit,
//    cancel, stop, restart, or create. Also assert the raw script never touches
//    any mutating official API.
// ---------------------------------------------------------------------------
const ALLOWED_ACTIONS = new Set(["noop", "reconcile", "reconnect"]);
for (const clientRunning of [true, false]) {
  for (const backend of [RUNNING_TRUE, RUNNING_FALSE, RUNNING_UNKNOWN, UNREACHABLE, null]) {
    const d = resolveDecision(clientRunning, backend);
    assert.ok(ALLOWED_ACTIONS.has(d), `unexpected decision: ${d}`);
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
// currentSessionId / parseSessionList / isStopControl unit checks
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

console.log("session_guard.test.mjs: all assertions passed");
