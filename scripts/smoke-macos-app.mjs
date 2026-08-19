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
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";

const APP = process.argv[2];
if (!APP || !existsSync(join(APP, "Contents"))) {
  console.error("usage: node scripts/smoke-macos-app.mjs <path-to.app>");
  process.exit(2);
}

const MACOS_DIR = join(APP, "Contents", "MacOS");
const RESOURCES = join(APP, "Contents", "Resources");
const RUNTIME = join(RESOURCES, "runtime");
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
  console.log(`SMOKE_${step}=FAIL`);
  if (detail) console.log(`SMOKE_${step}_DETAIL=${JSON.stringify(detail)}`);
  if (log) console.log("--- app stdout/stderr tail ---\n" + log.slice(-4000));
  console.log("PACKAGED_BACKEND_E2E=FAIL");
  console.log("GUI_E2E=NOT_AVAILABLE");
  process.exit(1);
}

const results = {};

// ── Architecture + node version of the PACKAGED runtime ─────────────────────
const nodeBin = join(RUNTIME, "darwin-x64", "node", "bin", "node");
if (!existsSync(nodeBin)) fail("BUNDLED_NODE", "missing " + nodeBin);
const fileNode = run("file", [nodeBin]);
if (!fileNode.out.includes("x86_64")) fail("BUNDLED_NODE_ARCH", fileNode.out);
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
if (!fileApp.out.includes("x86_64")) fail("APP_BINARY_ARCH", fileApp.out);
console.log(`SMOKE_APP_LAUNCH_PREP=OK (binary ${appBin} is x86_64)`);

// ── Runtime integrity (sha256 vs packaged manifest.json) ────────────────────
const manifest = JSON.parse(readFileSync(join(RUNTIME, "manifest.json"), "utf8"));
if (manifest.harness.version !== "0.1.0-rc.7") fail("INTEGRITY", "manifest harness version " + manifest.harness.version);
const t = manifest.targets["darwin-x64"];
if (!t) fail("INTEGRITY", "no darwin-x64 target in manifest");
if (sha256(nodeBin) !== t.nodeSha256) fail("INTEGRITY", "node sha256 mismatch vs manifest");
const entry = join(RUNTIME, "darwin-x64", "harness", manifest.harness.entry);
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
function findHarnessPort() {
  // Find TCP listeners owned by the app's process tree (the bundled node runs
  // dsh web from inside the .app, so its cmdline contains the app path).
  const lsof = run("lsof", ["-nP", "-iTCP", "-sTCP:LISTEN"]);
  if (lsof.code !== 0) return null;
  const appFrag = APP + "/Contents/Resources/runtime";
  for (const line of lsof.out.split("\n").slice(1)) {
    const f = line.split(/\s+/);
    if (f.length < 9) continue;
    const pid = f[1];
    const name = f[f.length - 1];
    const m = name.match(/(?:127\.0\.0\.1|\*|localhost):(\d+)$/);
    if (!m) continue;
    const ps = run("ps", ["-p", pid, "-o", "command="]);
    if (ps.out.includes(appFrag)) return { port: Number(m[1]), pid };
  }
  return null;
}

const deadline = Date.now() + TIMEOUT_MS;
let portInfo = null;
let sawReadyPhase = false;
while (Date.now() < deadline) {
  if (child.exitCode !== null) {
    fail("APP_LAUNCH", `app exited early (code=${child.exitCode})`, outBuf + "\n" + readStartupLog());
  }
  const log = readStartupLog();
  if (log.includes("[status] ready")) sawReadyPhase = true;
  if (log.includes("[error]")) fail("APP_ERROR", log.split("\n").filter((l) => l.includes("[error]")).join(" | "), outBuf + "\n" + log);
  if (!portInfo) portInfo = findHarnessPort();
  if (portInfo) {
    const http = run("curl", ["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "5", `http://127.0.0.1:${portInfo.port}/`]);
    if (http.out === "200") {
      results.port = portInfo.port;
      results.httpCode = "200";
      results.readyPhase = sawReadyPhase;
      console.log(`SMOKE_HARNESS_START=OK (pid=${portInfo.pid}, port=${portInfo.port})`);
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
      const stillListening = findHarnessPort();
      console.log(`SMOKE_SHUTDOWN=${child.exitCode === null ? "forced" : "graceful"} harness_port_closed=${stillListening ? "no" : "yes"}`);
      rmSync(home, { recursive: true, force: true });
      process.exit(0);
    }
  }
  spawnSync("sleep", [String(POLL_MS / 1000)]);
}
fail("READY_TIMEOUT", `no HTTP 200 on harness port within ${TIMEOUT_MS}ms` + (portInfo ? ` (port ${portInfo.port} found but not 200)` : " (no harness port found)"), outBuf + "\n" + readStartupLog());
