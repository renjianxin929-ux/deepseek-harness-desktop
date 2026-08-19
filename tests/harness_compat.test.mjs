// Compatibility probe against the REAL bundled rc.7 Harness source. Detects
// selector/token drift that would silently break the appearance layer (the
// engine relies on #root + the --dsw-alias-* / --dsw-specific-* tokens, not on
// hashed component classes). Skips gracefully when the bundled runtime isn't
// materialized (it is gitignored).
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import assert from "node:assert";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// Pick the materialized runtime target dir for the CURRENT host (darwin-arm64
// on Apple Silicon, darwin-x64 on Intel macOS, windows-x64 on Windows) so the
// probe always runs against the real closure whenever a runtime is
// materialized, and skips gracefully otherwise.
function hostRuntimeTarget() {
  if (process.platform === "darwin") return process.arch === "arm64" ? "darwin-arm64" : "darwin-x64";
  if (process.platform === "win32") return "windows-x64";
  return null;
}
const runtimeTarget = hostRuntimeTarget();
const harnessRoot = runtimeTarget
  ? join(root, "runtime", runtimeTarget, "harness", "node_modules", "@deepseek-ai")
  : join(root, "runtime", "__none__", "harness", "node_modules", "@deepseek-ai");

const indexHtml = join(harnessRoot, "dsh-web-frontend", "dist", "index.html");
const themeCss = join(harnessRoot, "dsh-client-ui-theme", "lib", "styles", "design-platform.css");
const layoutJs = join(harnessRoot, "dsh-client-ui-layout", "lib", "client.js");
const conversationJs = join(harnessRoot, "dsh-client-ui-conversation", "lib", "client.js");

// The frontend asset filenames carry content hashes that change between
// releases; discover them instead of pinning a hash.
function findAsset(namePrefix, dir) {
  const assetsDir = join(dir, "assets");
  if (!existsSync(assetsDir)) return null;
  const hit = readdirSync(assetsDir).find((f) => f.startsWith(namePrefix) && f.endsWith(".js"));
  return hit ? join(assetsDir, hit) : null;
}
const frontendJs = findAsset("index-", join(harnessRoot, "dsh-web-frontend", "dist"));

if (
  !existsSync(indexHtml) ||
  !existsSync(themeCss) ||
  !existsSync(layoutJs) ||
  !frontendJs ||
  !existsSync(conversationJs)
) {
  console.log("harness_compat.test.mjs: skipped (bundled runtime not materialized)");
  process.exit(0);
}

const html = readFileSync(indexHtml, "utf8");
const theme = readFileSync(themeCss, "utf8");
const layout = readFileSync(layoutJs, "utf8");
const frontend = readFileSync(frontendJs, "utf8");
const conversation = readFileSync(conversationJs, "utf8");

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

// 5. Composer input text readability contract (V0.2 hotfix). The official
//    composer paints the user's text on a separate absolutely-positioned,
//    pointer-events:none layer (the "backdrop") while the textarea's own text
//    is transparent (`color:#0000`). The engine's fix targets exactly that
//    layer; if upstream stops using this pattern the fix degrades to a no-op
//    and the input text reverts to upstream styling (still readable).
assert.ok(
  conversation.includes("color:#0000"),
  "composer textarea must keep its transparent own-text (backdrop pattern)"
);
assert.ok(
  conversation.includes("pointer-events:none") &&
    conversation.includes("position:absolute") &&
    conversation.includes("label-primary"),
  "composer backdrop text layer must exist (absolute, pointer-events:none, label-primary)"
);
assert.ok(
  conversation.includes("caret-color") || conversation.includes("caretColor"),
  "composer caret must be explicitly colored (visible against glass)"
);

console.log("harness_compat.test.mjs: all assertions passed");
