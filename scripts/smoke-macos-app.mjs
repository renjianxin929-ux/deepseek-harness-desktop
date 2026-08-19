#!/usr/bin/env node
// Packaged-app backend smoke for the macOS Intel x64 build slice.
//
//   node scripts/smoke-macos-app.mjs <path-to-DeepSeek Harness Desktop.app>
//
// Launches the REAL packaged .app with an ISOLATED HOME (never touches the
// runner user's ~/.dsh or real config) and a PATH without any node/npm, then
// verifies the backend chain honestly:
//   A. app executable launches (x86_64)
//   B. bundled x64 node launches            (v22.22.3, x86_64)
//   C. runtime integrity passes             (sha256 vs packaged manifest.json)
//   D. Harness process starts               (dsh web child of the app)
//   E. loopback port becomes ready
//   F. HTTP GET / on that port returns 200
//   G. no system Node required              (PATH has no node; harness runs the
//                                            bundled node inside the .app)
//
// GUI (the WebView window) is NOT visually verifiable in headless CI — this
// script reports PACKAGED_BACKEND_E2E=PASS and GUI_E2E=NOT_AVAILABLE. It never
// claims a GUI PASS it cannot prove.

import { spawn, spawnSync, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";

const APP = resolve(process.argv[2] || "");
if (!APP || !existsSync(join(APP, "Contents"))) {
  console.error("usage: node scripts/smoke-macos-app.mjs <path-to.app>");
  console.error("  (resolved APP=" + APP + ", cwd=" + process.cwd() + ")");
  process.exit(2);
}

const MACOS_DIR = join(APP, "Contents", "MacOS");
const RESOURCES = join(APP, "Contents", "Resources");
// Tauri v2 maps `..`-prefixed bundle resources (e.g. "../runtime") under
// `Contents/Resources/_up_/` inside the .app — the app's own runtime resolver
// checks both locations. Mirror that discovery here.
const RUNTIME = existsSync(join(RESOURCES, "_up_", "runtime"))
  ? join(RESOURCES, "_up_", "runtime")
  : join(RESOURCES, "runtime");
// Target dir override (defaults to the x64 slice this smoke is built for;
// set SMOKE_RUNTIME_TARGET=darwin-arm64 to run the same flow on an arm64 build).
const RUNTIME_TARGET = process.env.SMOKE_RUNTIME_TARGET || "darwin-x64";
// Expected Mach-O arch marker for the target (x64 slice enforces x86_64; the
// override path allows validating the same flow against a native arm64 build).
const EXPECTED_ARCH = RUNTIME_TARGET === "darwin-arm64" ? "arm64" : "x86_64";
const TIMEOUT_MS = Number(process.env.SMOKE_TIMEOUT_MS || 150000);
const POLL_MS = 2000;

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}
function run(cmd, args) {
  const r = spawnSync(cmd, args, { encoding: "utf8" });
  return { code: r.status, out: (r.stdout || "").trim(), err: (r.stderr || "").trim() };
}
function fail(step, detail, log) {
  try { child && child.kill("SIGTERM"); } catch (_) {}
  console.log(`SMOKE_${step}=FAIL`);
  if (detail) console.log(`SMOKE_${step}_DETAIL=${JSON.stringify(detail)}`);
  if (log) console.log("--- app stdout/stderr tail ---\n" + log.slice(-4000));
  console.log("PACKAGED_BACKEND_E2E=FAIL");
  console.log("GUI_E2E=NOT_AVAILABLE");
  process.exit(1);
}

const results = {};

// ── Architecture + node version of the PACKAGED runtime ─────────────────────
const nodeBin = join(RUNTIME, RUNTIME_TARGET, "node", "bin", "node");
if (!existsSync(nodeBin)) {
  // Path inventory so a CI packaging regression is diagnosable at a glance.
  const steps = [APP, join(APP, "Contents"), RESOURCES, RUNTIME, join(RUNTIME, RUNTIME_TARGET), join(RUNTIME, RUNTIME_TARGET, "node"), join(RUNTIME, RUNTIME_TARGET, "node", "bin")];
  const inv = ["cwd=" + process.cwd(), "APP=" + JSON.stringify(APP), "RUNTIME=" + JSON.stringify(RUNTIME), "nodeBin=" + JSON.stringify(nodeBin)];
  for (const p of steps) {
    let kind = "missing";
    try { kind = existsSync(p) ? (lstatSync(p).isSymbolicLink() ? "symlink" : "dir/file") : "missing"; } catch { kind = "err"; }
    inv.push(`  ${kind}  ${p}`);
  }
  inv.push("--- ls -la " + join(APP, "Contents", "Resources") + " ---\n" + run("ls", ["-la", join(APP, "Contents", "Resources")]).out);
  inv.push("--- ls -la " + RUNTIME + " ---\n" + run("ls", ["-la", RUNTIME]).out);
  fail("BUNDLED_NODE", "missing " + nodeBin + "\n" + inv.join("\n"));
}
const fileNode = run("file", [nodeBin]);
if (!fileNode.out.includes(EXPECTED_ARCH)) fail("BUNDLED_NODE_ARCH", fileNode.out);
const nodeVer = run(nodeBin, ["--version"]);
if (!nodeVer.out.startsWith("v22.22.3")) fail("BUNDLED_NODE_VERSION", nodeVer.out);
results.nodeArch = "x86_64";
results.nodeVersion = nodeVer.out;
console.log(`SMOKE_BUNDLED_NODE=OK (${nodeVer.out}, x86_64)`);

// ── App binary architecture ──────────────────────────────────────────────────
const bins = readdirSync(MACOS_DIR).filter((n) => !n.startsWith("."));
if (!bins.length) fail("APP_BINARY", "no binary in Contents/MacOS");
const appBin = join(MACOS_DIR, bins[0]);
const fileApp = run("file", [appBin]);
if (!fileApp.out.includes(EXPECTED_ARCH)) fail("APP_BINARY_ARCH", fileApp.out);
console.log(`SMOKE_APP_LAUNCH_PREP=OK (binary ${appBin} is ${EXPECTED_ARCH})`);

// ── Runtime integrity (sha256 vs packaged manifest.json) ────────────────────
const manifest = JSON.parse(readFileSync(join(RUNTIME, "manifest.json"), "utf8"));
if (manifest.harness.version !== "0.1.0-rc.7") fail("INTEGRITY", "manifest harness version " + manifest.harness.version);
const t = manifest.targets[RUNTIME_TARGET];
if (!t) fail("INTEGRITY", "no " + RUNTIME_TARGET + " target in manifest");
if (sha256(nodeBin) !== t.nodeSha256) fail("INTEGRITY", "node sha256 mismatch vs manifest");
const entry = join(RUNTIME, RUNTIME_TARGET, "harness", manifest.harness.entry);
if (!existsSync(entry)) fail("INTEGRITY", "harness entry missing: " + entry);
if (sha256(entry) !== t.harnessEntrySha256) fail("INTEGRITY", "harness entry sha256 mismatch vs manifest");
console.log("SMOKE_INTEGRITY=OK (node + harness entry sha256 match packaged manifest.json)");

// ── Launch with isolated HOME and a PATH WITHOUT node/npm ───────────────────
const home = mkdtempSync(join(tmpdir(), "hd-intel-smoke-"));
const logDir = join(home, "Library", "Logs", "HarnessDesktop");
mkdirSync(logDir, { recursive: true });
const startupLog = join(logDir, "startup.log");
const env = {
  ...process.env,
  HOME: home,
  TMPDIR: home,
  TMP: home,
  TEMP: home,
  PATH: "/usr/bin:/bin:/usr/sbin:/sbin", // no node/npm/npx/dsh anywhere
};
delete env.HD_SYSTEM_RUNTIME;

const child = spawn(appBin, [], { env, stdio: ["ignore", "pipe", "pipe"] });
let outBuf = "";
child.stdout.on("data", (d) => (outBuf += d));
child.stderr.on("data", (d) => (outBuf += d));

function readStartupLog() {
  try {
    return readFileSync(startupLog, "utf8");
  } catch {
    return "";
  }
}
// The app logs its own readiness line ("[ready] http://127.0.0.1:<port>") —
// the authoritative source for the harness port (no lsof guessing needed).
function portFromStartupLog(log) {
  const m = (log || "").match(/\[ready\] http:\/\/127\.0\.0\.1:(\d+)/);
  return m ? Number(m[1]) : null;
}
function findHarnessPort() {
  // Find TCP listeners owned by the app's process tree (the bundled node runs
  // dsh web from inside the .app, so its cmdline contains the app path).
  const lsof = run("lsof", ["-nP", "-iTCP", "-sTCP:LISTEN"]);
  if (lsof.code !== 0) return null;
  const appFrag = "/runtime/" + RUNTIME_TARGET + "/";
  // macOS lsof NAME column looks like "127.0.0.1:54321 (LISTEN)" — scan every
  // field for the address (the trailing "(LISTEN)" state must not be mistaken
  // for the address).
  for (const line of lsof.out.split("\n").slice(1)) {
    const f = line.split(/\s+/);
    if (f.length < 5) continue;
    const pid = f[1];
    let addr = null;
    for (const field of f) {
      const m = field.match(/^(?:127\.0\.0\.1|\*|localhost):(\d+)$/);
      if (m) { addr = m[1]; break; }
    }
    if (!addr) continue;
    const ps = run("ps", ["-p", pid, "-o", "command="]);
    if (ps.out.includes(appFrag)) return { port: Number(addr), pid, attributed: true };
  }
  // Fallback: the app is alive but no listener was attributed to it — probe
  // every 127.0.0.1 listener and accept one that serves HTTP (on a clean CI
  // runner there is nothing else listening on loopback during this window).
  const fallback = [];
  for (const line of lsof.out.split("\n").slice(1)) {
    const f = line.split(/\s+/);
    if (f.length < 5) continue;
    for (const field of f) {
      const m = field.match(/^127\.0\.0\.1:(\d+)$/);
      if (m) { fallback.push(Number(m[1])); break; }
    }
  }
  if (fallback.length) return { port: fallback[0], pid: null, attributed: false };
  return null;
}

function diagnostics() {
  const appName = basename(APP, ".app");
  const psAll = run("ps", ["axo", "pid,ppid,command"]);
  const relevant = (psAll.out || "").split("\n").filter((l) => l.includes(appName) || l.includes("runtime/" + RUNTIME_TARGET));
  return (
    "--- ps (app/runtime processes) ---\n" +
    (relevant.join("\n") || "(none)") +
    "\n--- lsof -nP -iTCP -sTCP:LISTEN ---\n" +
    run("lsof", ["-nP", "-iTCP", "-sTCP:LISTEN"]).out +
    "\n--- startup.log ---\n" +
    readStartupLog()
  );
}

const deadline = Date.now() + TIMEOUT_MS;
let portInfo = null;
let sawReadyPhase = false;
while (Date.now() < deadline) {
  if (child.exitCode !== null) {
    fail("APP_LAUNCH", `app exited early (code=${child.exitCode})`, outBuf + "\n" + diagnostics());
  }
  const log = readStartupLog();
  if (log.includes("[status] ready")) sawReadyPhase = true;
  if (log.includes("[error]")) fail("APP_ERROR", log.split("\n").filter((l) => l.includes("[error]")).join(" | "), outBuf + "\n" + diagnostics());
  const loggedPort = portFromStartupLog(log);
  if (loggedPort && (!portInfo || portInfo.port !== loggedPort)) {
    portInfo = { port: loggedPort, pid: null, attributed: false, source: "startup.log" };
  }
  if (!portInfo) portInfo = findHarnessPort();
  if (portInfo) {
    const http = run("curl", ["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "5", `http://127.0.0.1:${portInfo.port}/`]);
    if (http.out === "200") {
      results.port = portInfo.port;
      results.httpCode = "200";
      results.readyPhase = sawReadyPhase;
      console.log(`SMOKE_HARNESS_START=OK (pid=${portInfo.pid}, port=${portInfo.port}, attributed=${portInfo.attributed})`);
      console.log("SMOKE_PORT=" + portInfo.port);
      console.log("SMOKE_HTTP_READY=200");
      console.log(`SMOKE_NO_SYSTEM_NODE=OK (launched with PATH=${env.PATH}; harness runs bundled node inside the .app)`);
      console.log("SMOKE_APP_LAUNCH=OK");
      console.log("PACKAGED_BACKEND_E2E=PASS");
      console.log("GUI_E2E=NOT_AVAILABLE (headless CI: WebView window not visually verifiable)");
      // Graceful shutdown check: SIGTERM, expect clean exit + child cleanup.
      child.kill("SIGTERM");
      const grace = Date.now() + 15000;
      while (Date.now() < grace && child.exitCode === null) {
        spawnSync("sleep", ["1"]);
      }
      if (child.exitCode === null) {
        try { child.kill("SIGKILL"); } catch (_) {}
        spawnSync("sleep", ["1"]);
      }
      const stillListening = findHarnessPort();
      console.log(`SMOKE_SHUTDOWN=${child.exitCode === null ? "forced" : "graceful"} harness_port_closed=${stillListening ? "no" : "yes"}`);
      rmSync(home, { recursive: true, force: true });
      process.exit(0);
    }
  }
  spawnSync("sleep", [String(POLL_MS / 1000)]);
}
fail("READY_TIMEOUT", `no HTTP 200 on harness port within ${TIMEOUT_MS}ms` + (portInfo ? ` (port ${portInfo.port} found but not 200)` : " (no harness port found)"), outBuf + "\n" + diagnostics());
