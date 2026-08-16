#!/usr/bin/env node
// DeepSeek Harness Desktop — Windows runtime smoke check.
//
// Validates a materialized `windows-x64` bundled runtime. On Windows it runs the
// bundled `node.exe` + pinned dsh entry for real (functional check); on any
// other host it performs the structural checks only (the Windows `node.exe`
// cannot execute there), and reports that explicitly rather than claiming a
// Windows run.
//
//   node scripts/smoke-windows-runtime.mjs [--runtime <dir>]
//
// Default runtime root: <repo>/runtime (same default as the materializer).

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(fileURLToPath(new URL(".", import.meta.url)), "..");
const TARGET = "windows-x64";
const HARNESS_PACKAGE = "@deepseek-ai/dsh";
const HARNESS_VERSION = "0.1.0-rc.6";

function parseArgs(argv) {
  const args = { runtime: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--runtime") args.runtime = argv[++i];
  }
  return args;
}

function sha256(buf) {
  return createHash("sha256").update(buf).digest("hex");
}

function fail(msg) {
  console.error(`[smoke] FAIL: ${msg}`);
  process.exit(1);
}

function ok(msg) {
  console.log(`[smoke] ok: ${msg}`);
}

const { runtime } = parseArgs(process.argv.slice(2));
const runtimeDir = runtime ? resolve(REPO_ROOT, runtime) : join(REPO_ROOT, "runtime");

// 1. Manifest.
const manifestPath = join(runtimeDir, "manifest.json");
if (!existsSync(manifestPath)) fail(`missing ${manifestPath}`);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
if (manifest.harness?.package !== HARNESS_PACKAGE) fail("manifest harness package mismatch");
if (manifest.harness?.version !== HARNESS_VERSION) {
  fail(`manifest harness version ${manifest.harness?.version} != ${HARNESS_VERSION}`);
}
const target = manifest.targets?.[TARGET];
if (!target) fail(`no target "${TARGET}" in manifest`);
if (!target.nodeSha256 || !target.harnessEntrySha256) {
  fail("manifest is missing integrity checksums for windows-x64");
}
ok(`manifest (harness ${manifest.harness.version}, node ${manifest.node.version})`);

// 2. Layout: node.exe + harness entry.
const nodeExe = join(runtimeDir, target.node);
const harnessDir = join(runtimeDir, target.harness);
const binJs = join(harnessDir, manifest.harness.entry);
if (!existsSync(nodeExe)) fail(`node.exe missing at ${nodeExe}`);
if (!existsSync(binJs)) fail(`harness entry missing at ${binJs}`);
ok("node.exe + harness entry present");

// 3. Integrity checksums.
const nodeActual = sha256(readFileSync(nodeExe));
if (nodeActual !== target.nodeSha256) fail("node.exe sha256 mismatch");
const entryActual = sha256(readFileSync(binJs));
if (entryActual !== target.harnessEntrySha256) fail("harness entry sha256 mismatch");
ok("integrity checksums verified");

// 4. Pinned dsh version from the materialized tree.
const dshPkgPath = join(harnessDir, "node_modules", HARNESS_PACKAGE, "package.json");
if (!existsSync(dshPkgPath)) fail(`missing ${HARNESS_PACKAGE}/package.json`);
const dshPkg = JSON.parse(readFileSync(dshPkgPath, "utf8"));
if (dshPkg.version !== HARNESS_VERSION) fail(`dsh resolved to ${dshPkg.version}`);
ok(`${HARNESS_PACKAGE}@${dshPkg.version}`);

// 5. License / notice inventory + no user data.
for (const f of ["NOTICE.md", "THIRD_PARTY_LICENSES.json"]) {
  if (!existsSync(join(runtimeDir, f))) fail(`missing ${f}`);
}
if (!existsSync(join(runtimeDir, TARGET, "node", "LICENSE"))) fail("missing bundled node LICENSE");
ok("license/notice inventory present");
if (existsSync(join(runtimeDir, TARGET, ".dsh"))) fail("unexpected user data (~/.dsh) in runtime");

// 6. Functional check — Windows only (real node.exe + dsh entry).
if (process.platform === "win32") {
  const nodeVerRaw = execFileSync(nodeExe, ["--version"], { encoding: "utf8" }).trim();
  const nodeVer = nodeVerRaw.replace(/^v/, "");
  if (nodeVer !== manifest.node.version) {
    fail(`node.exe reported ${nodeVerRaw}, expected ${manifest.node.version}`);
  }
  ok(`node.exe --version -> ${nodeVerRaw}`);

  const dshVer = execFileSync(nodeExe, [binJs, "--version"], { encoding: "utf8" }).trim();
  if (dshVer !== HARNESS_VERSION) fail(`dsh reported ${dshVer}, expected ${HARNESS_VERSION}`);
  ok(`dsh --version -> ${dshVer}`);
} else {
  console.log(
    `[smoke] skip functional run on ${process.platform} (node.exe is a Windows binary; structural checks only)`
  );
}

console.log("[smoke] PASS");
