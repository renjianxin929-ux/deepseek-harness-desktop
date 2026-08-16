#!/usr/bin/env node
// DeepSeek Harness Desktop — deterministic build-time runtime materializer.
//
// Recreates the self-contained bundled runtime from a clean source checkout.
// The network is used ONLY here (at build/packaging time); runtime startup
// performs no npm/npx/network bootstrap.
//
//   node scripts/materialize-runtime.mjs [--target <id>] [--out <dir>]
//
// Defaults: target = current platform, out = <repo>/runtime.
//
// What it does:
//   1. downloads the pinned Node artifact and verifies its official SHA-256,
//   2. `npm ci` the pinned @deepseek-ai/dsh@0.1.0-rc.6 closure from the
//      committed package-lock.json (exact, reproducible),
//   3. assembles runtime/<target>/node + runtime/<target>/harness,
//   4. writes runtime/manifest.json (integrity checksums),
//   5. writes runtime/NOTICE.md + runtime/THIRD_PARTY_LICENSES.json
//      (generated license inventory from the actual materialized tree).
//
// Idempotent: re-running produces the same layout and metadata.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
  rmSync,
  cpSync,
} from "node:fs";
import { join, dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, "..");

const NODE_VERSION = "22.22.3";
const NODE_DIST_BASE = `https://nodejs.org/dist/v${NODE_VERSION}`;
const DSH_PACKAGE = "@deepseek-ai/dsh";
const DSH_VERSION = "0.1.0-rc.6";
const MANIFEST_SCHEMA_VERSION = 1;

const TARGETS = {
  "darwin-arm64": {
    platform: "darwin",
    arch: "arm64",
    distArch: "darwin-arm64",
    nodeArchive: `node-v${NODE_VERSION}-darwin-arm64.tar.gz`,
    nodeTopDir: `node-v${NODE_VERSION}-darwin-arm64`,
    nodeBinRel: "bin/node",
  },
  "darwin-x64": {
    platform: "darwin",
    arch: "x64",
    distArch: "darwin-x64",
    nodeArchive: `node-v${NODE_VERSION}-darwin-x64.tar.gz`,
    nodeTopDir: `node-v${NODE_VERSION}-darwin-x64`,
    nodeBinRel: "bin/node",
  },
  "windows-x64": {
    platform: "win32",
    arch: "x64",
    distArch: "win-x64",
    nodeArchive: `node-v${NODE_VERSION}-win-x64.zip`,
    nodeTopDir: `node-v${NODE_VERSION}-win-x64`,
    nodeBinRel: "node.exe",
  },
};

function parseArgs(argv) {
  const args = { target: null, out: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--target") args.target = argv[++i];
    else if (argv[i] === "--out") args.out = argv[++i];
  }
  return args;
}

function currentTargetId() {
  const p = process.platform; // darwin | win32 | linux
  const a = process.arch; // arm64 | x64
  if (p === "darwin" && a === "arm64") return "darwin-arm64";
  if (p === "darwin" && a === "x64") return "darwin-x64";
  if (p === "win32" && a === "x64") return "windows-x64";
  throw new Error(`unsupported platform/arch: ${p}/${a} (supported: darwin-arm64, darwin-x64, windows-x64)`);
}

function sha256(buf) {
  return createHash("sha256").update(buf).digest("hex");
}

async function fetchBuffer(url) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`fetch ${url} -> HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

function run(cmd, args, opts = {}) {
  execFileSync(cmd, args, { stdio: "inherit", ...opts });
}

async function materializeNode(targetId, spec, outDir, workDir) {
  const nodeDir = join(outDir, targetId, "node");
  const nodeBin = join(nodeDir, spec.nodeBinRel);
  const archivePath = join(workDir, spec.nodeArchive);
  const shasumsPath = join(workDir, "SHASUMS256.txt");

  if (!existsSync(archivePath)) {
    console.log(`[runtime] downloading ${NODE_DIST_BASE}/${spec.nodeArchive}`);
    const archive = await fetchBuffer(`${NODE_DIST_BASE}/${spec.nodeArchive}`);
    writeFileSync(archivePath, archive);
  }
  if (!existsSync(shasumsPath)) {
    console.log(`[runtime] downloading ${NODE_DIST_BASE}/SHASUMS256.txt`);
    const shasums = await fetchBuffer(`${NODE_DIST_BASE}/SHASUMS256.txt`);
    writeFileSync(shasumsPath, shasums);
  }

  const shasums = readFileSync(shasumsPath, "utf8");
  const line = shasums.split("\n").find((l) => l.includes(`  ${spec.nodeArchive}`));
  if (!line) throw new Error(`SHASUMS256.txt has no entry for ${spec.nodeArchive}`);
  const expected = line.split(/\s+/)[0].toLowerCase();
  const actual = sha256(readFileSync(archivePath));
  if (actual !== expected) {
    throw new Error(`Node archive integrity check FAILED: expected ${expected}, got ${actual}`);
  }
  console.log(`[runtime] node archive sha256 verified: ${actual}`);

  // Extract with `tar` (handles .tar.gz and .zip via libarchive on macOS and
  // Windows 10+; Linux has GNU tar).
  const extractDir = join(workDir, `node-extract-${targetId}`);
  rmSync(extractDir, { recursive: true, force: true });
  mkdirSync(extractDir, { recursive: true });
  run("tar", ["-xf", archivePath, "-C", extractDir]);

  rmSync(nodeDir, { recursive: true, force: true });
  mkdirSync(join(nodeDir, dirname(spec.nodeBinRel)), { recursive: true });
  cpSync(join(extractDir, spec.nodeTopDir, spec.nodeBinRel), nodeBin);
  // Preserve Node's own license text for attribution.
  const nodeLicense = join(extractDir, spec.nodeTopDir, "LICENSE");
  if (existsSync(nodeLicense)) {
    cpSync(nodeLicense, join(nodeDir, "LICENSE"));
  }
  return nodeBin;
}

function materializeHarness(targetId, outDir, workDir) {
  const staging = join(workDir, "harness-stage");
  rmSync(staging, { recursive: true, force: true });
  mkdirSync(staging, { recursive: true });

  // Copy the committed, exact-pinned package manifest + lockfile and reproduce
  // the dependency closure deterministically with `npm ci`.
  cpSync(join(__dirname, "runtime-package.json"), join(staging, "package.json"));
  cpSync(join(__dirname, "runtime-package-lock.json"), join(staging, "package-lock.json"));
  console.log(`[runtime] npm ci @deepseek-ai/dsh@${DSH_VERSION} (from committed lockfile)`);
  run("npm", ["ci", "--no-audit", "--no-fund"], { cwd: staging });

  const harnessDir = join(outDir, targetId, "harness");
  rmSync(harnessDir, { recursive: true, force: true });
  mkdirSync(harnessDir, { recursive: true });
  cpSync(join(staging, "node_modules"), join(harnessDir, "node_modules"), { recursive: true });
  // npm's `.bin` directory holds CLI shim symlinks (absolute paths into the
  // staging dir). They are not used by `dsh web` (the Desktop launches the
  // harness entry directly) and would be broken after staging is removed, so
  // drop them rather than ship broken symlinks.
  rmSync(join(harnessDir, "node_modules", ".bin"), { recursive: true, force: true });
  cpSync(join(staging, "package.json"), join(harnessDir, "package.json"));
  cpSync(join(staging, "package-lock.json"), join(harnessDir, "package-lock.json"));

  // Verify the resolved dsh version matches the pin exactly.
  const pkg = JSON.parse(readFileSync(join(harnessDir, "node_modules", DSH_PACKAGE, "package.json"), "utf8"));
  if (pkg.version !== DSH_VERSION) {
    throw new Error(`@deepseek-ai/dsh resolved to ${pkg.version}, expected ${DSH_VERSION}`);
  }
  console.log(`[runtime] harness version verified: ${DSH_PACKAGE}@${pkg.version}`);
  return harnessDir;
}

function writeManifest(outDir, targetId, nodeBin, harnessDir) {
  const entryRel = join("node_modules", DSH_PACKAGE, "lib", "bin.js");
  const entryAbs = join(harnessDir, entryRel);
  const targets = {};
  for (const id of Object.keys(TARGETS)) {
    const t = TARGETS[id];
    const isCurrent = id === targetId;
    targets[id] = {
      node: join(id, "node", t.nodeBinRel),
      harness: join(id, "harness"),
      nodeSha256: isCurrent ? sha256(readFileSync(nodeBin)) : "",
      harnessEntrySha256: isCurrent ? sha256(readFileSync(entryAbs)) : "",
    };
  }
  const manifest = {
    schemaVersion: MANIFEST_SCHEMA_VERSION,
    harness: {
      package: DSH_PACKAGE,
      version: DSH_VERSION,
      entry: entryRel,
    },
    node: {
      version: NODE_VERSION,
      source: `${NODE_DIST_BASE}/${TARGETS[targetId].nodeArchive}`,
    },
    targets,
  };
  writeFileSync(join(outDir, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
  console.log(`[runtime] manifest written (checksums for ${targetId})`);
}

// ---------------------------------------------------------------------------
// License inventory (R4) — generated from the actual materialized tree.
// ---------------------------------------------------------------------------

function licenseExpression(pkg) {
  if (typeof pkg.license === "string") return pkg.license;
  if (pkg.license && typeof pkg.license === "object") return pkg.license.type || "(object)";
  if (Array.isArray(pkg.licenses) && pkg.licenses.length) {
    return pkg.licenses.map((l) => l.type || "(unknown)").join(" OR ");
  }
  return "(no license field)";
}

function collectLicenses(harnessDir) {
  const root = join(harnessDir, "node_modules");
  const inventory = [];
  const seen = new Set();
  const walk = (dir) => {
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (e.name === ".bin" || e.name.startsWith(".")) continue;
      const full = join(dir, e.name);
      if (e.isDirectory()) {
        const pkgPath = join(full, "package.json");
        if (existsSync(pkgPath)) {
          try {
            const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
            if (pkg.name && !seen.has(pkg.name)) {
              seen.add(pkg.name);
              const licenseFile = ["LICENSE", "LICENSE.md", "LICENSE.txt", "COPYING", "LICENCE"]
                .map((f) => join(full, f))
                .find((f) => existsSync(f));
              inventory.push({
                name: pkg.name,
                version: pkg.version || "(unknown)",
                license: licenseExpression(pkg),
                licenseFile: licenseFile ? relative(harnessDir, licenseFile) : null,
              });
            }
          } catch {
            /* skip malformed package.json */
          }
        }
        walk(full);
      }
    }
  };
  walk(root);
  inventory.sort((a, b) => a.name.localeCompare(b.name));
  return inventory;
}

function packageJsonCount(harnessDir) {
  const root = join(harnessDir, "node_modules");
  let count = 0;
  const walk = (dir) => {
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (e.name === ".bin" || e.name.startsWith(".")) continue;
      const full = join(dir, e.name);
      if (e.isDirectory()) {
        if (existsSync(join(full, "package.json"))) count += 1;
        walk(full);
      }
    }
  };
  walk(root);
  return count;
}

function lockfileEntryCount(harnessDir) {
  const lock = JSON.parse(readFileSync(join(harnessDir, "package-lock.json"), "utf8"));
  return Object.keys(lock.packages || {}).length;
}

function writeLicenseInventory(outDir, harnessDir) {
  const inventory = collectLicenses(harnessDir);
  const lgpl = inventory.filter((p) => /LGPL/i.test(p.license));
  const counts = {
    packageJsonCount: packageJsonCount(harnessDir),
    lockfilePackageEntryCount: lockfileEntryCount(harnessDir),
  };

  writeFileSync(
    join(outDir, "THIRD_PARTY_LICENSES.json"),
    JSON.stringify({ generatedBy: "scripts/materialize-runtime.mjs", counts, packages: inventory }, null, 2) + "\n"
  );

  const lgplRows = lgpl
    .map((p) => `- ${p.name}@${p.version} — ${p.license}`)
    .join("\n");

  const notice = `# Third-Party Notices — DeepSeek Harness Desktop bundled runtime

> This is an **engineering-level inventory for attribution**, generated from the
> actual materialized dependency tree by \`scripts/materialize-runtime.mjs\`.
> It is **not a legal opinion**. Bundled third-party packages carry their own
> license terms, which continue to apply; see each package's \`LICENSE\` file
> under \`runtime/<target>/harness/node_modules/**\`.

## Bundled components

1. **Node.js** v${NODE_VERSION} — MIT-style ("Copyright Node.js contributors"), with
   bundled third-party notices. License text: \`<target>/node/LICENSE\` (from the
   official distribution).
2. **DeepSeek Harness** \`${DSH_PACKAGE}@${DSH_VERSION}\` — MIT. License text:
   \`<target>/harness/node_modules/${DSH_PACKAGE}/LICENSE\`.
3. **Third-party dependency closure** — ${counts.packageJsonCount} packages
   (see counts below). Their individual licenses continue to apply.

## Inventory counts (distinct metrics — do not conflate)

- \`PACKAGE_JSON_COUNT=${counts.packageJsonCount}\` — number of \`package.json\`
  files found under the materialized \`node_modules\` tree (one per installed
  package instance).
- \`LOCKFILE_PACKAGE_ENTRY_COUNT=${counts.lockfilePackageEntryCount}\` — number of
  entries in \`package-lock.json\` \`packages\` map (includes the root entry and
  lockfile-resolution entries, and is therefore a different metric).

## LGPL-bearing bundled components (explicitly listed)

${lgplRows || "- (none found)"}

LGPL components may carry additional obligations (e.g. source availability);
their license texts are preserved alongside each package. This notice does not
assert that every such obligation has been satisfied.

## Full machine-readable inventory

See \`runtime/THIRD_PARTY_LICENSES.json\`.
`;

  writeFileSync(join(outDir, "NOTICE.md"), notice);
  console.log(`[runtime] license inventory written (${inventory.length} packages, ${lgpl.length} LGPL-bearing)`);
}

// ---------------------------------------------------------------------------

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const targetId = args.target || currentTargetId();
  const spec = TARGETS[targetId];
  if (!spec) throw new Error(`unknown target ${targetId}`);

  const outDir = args.out ? resolve(REPO_ROOT, args.out) : join(REPO_ROOT, "runtime");
  const workDir = join(outDir, ".materialize-work");
  mkdirSync(outDir, { recursive: true });
  mkdirSync(workDir, { recursive: true });

  console.log(`[runtime] materializing target=${targetId} out=${outDir}`);
  const nodeBin = await materializeNode(targetId, spec, outDir, workDir);
  const harnessDir = materializeHarness(targetId, outDir, workDir);
  writeManifest(outDir, targetId, nodeBin, harnessDir);
  writeLicenseInventory(outDir, harnessDir);

  rmSync(workDir, { recursive: true, force: true });
  console.log(`[runtime] done: ${outDir}`);
}

main().catch((err) => {
  console.error(`[runtime] materialization FAILED: ${err.message}`);
  process.exit(1);
});
