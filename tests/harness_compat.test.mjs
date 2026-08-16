// Compatibility probe against the REAL bundled rc.6 Harness source. Detects
// selector/token drift that would silently break the appearance layer (the
// engine relies on #root + the --dsw-alias-* / --dsw-specific-* tokens, not on
// hashed component classes). Skips gracefully when the bundled runtime isn't
// materialized (it is gitignored).
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import assert from "node:assert";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const harnessRoot = join(root, "runtime", "darwin-arm64", "harness", "node_modules", "@deepseek-ai");

const indexHtml = join(harnessRoot, "dsh-web-frontend", "dist", "index.html");
const themeCss = join(harnessRoot, "dsh-client-ui-theme", "lib", "styles", "design-platform.css");
const layoutJs = join(harnessRoot, "dsh-client-ui-layout", "lib", "client.js");
const frontendJs = join(harnessRoot, "dsh-web-frontend", "dist", "assets", "index-Dqw48FrP.js");

if (!existsSync(indexHtml) || !existsSync(themeCss) || !existsSync(layoutJs) || !existsSync(frontendJs)) {
  console.log("harness_compat.test.mjs: skipped (bundled runtime not materialized)");
  process.exit(0);
}

const html = readFileSync(indexHtml, "utf8");
const theme = readFileSync(themeCss, "utf8");
const layout = readFileSync(layoutJs, "utf8");
const frontend = readFileSync(frontendJs, "utf8");

// 1. #root mount point (the engine's ROOT_SELECTOR + boot gate).
assert.ok(html.includes('id="root"'), "#root mount must exist in Harness index.html");

// 2. Every token the engine overrides must be defined in the Harness token sheet.
const ENGINE_TOKENS = [
  "--dsw-alias-bg-base",
  "--dsw-alias-bg-layer-1",
  "--dsw-alias-bg-layer-2",
  "--dsw-alias-bg-layer-3",
  "--dsw-alias-bg-module-platform",
  "--dsw-specific-sidebar-fill",
  "--dsw-specific-input-major",
  "--dsw-alias-label-primary",
];
for (const token of ENGINE_TOKENS) {
  assert.ok(theme.includes(token), `token ${token} must be defined in design-platform.css`);
}

// 3. The real viewport-covering surfaces must CONSUME the tokens (not hardcoded
//    colors), so the engine's body-level token overrides actually take effect.
assert.ok(
  layout.includes("var(--dsw-alias-bg-base)"),
  "app frame must use var(--dsw-alias-bg-base) as its background"
);
assert.ok(
  layout.includes("var(--dsw-specific-sidebar-fill)"),
  "sidebar must use var(--dsw-specific-sidebar-fill) as its background"
);

// 4. The local composer surface the engine/theme target must be real. React
//    `contentEditable` renders as contenteditable="true"; `textarea` is also
//    accepted by the theme selector.
assert.ok(
  frontend.includes("contentEditable") || frontend.includes("textarea"),
  "composer must be a real contenteditable/textarea surface"
);

console.log("harness_compat.test.mjs: all assertions passed");
