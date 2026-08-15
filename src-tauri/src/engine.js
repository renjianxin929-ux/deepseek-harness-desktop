// DeepSeek Harness Desktop — injected appearance engine (surface layer only).
//
// This script is injected by the Rust backend into every page loaded in the
// main WebView (both the local loading page and the official DeepSeek Harness
// Web UI). It is intentionally self-contained: it has no access to Tauri IPC,
// no access to Harness credentials/sessions, and no ability to run arbitrary
// theme code. Themes are declarative JSON (tokens + constrained CSS strings)
// that this engine turns into a single <style> element and a few fixed,
// pointer-events:none background layers appended to <body>.
//
// The engine NEVER touches the React tree inside #root, so an upstream Harness
// DOM change cannot corrupt the application. If the Harness UI can no longer be
// targeted confidently, the engine degrades or falls back without breaking
// anything.
//
// Public surface (used by the Rust backend):
//   window.__HD_APPLY__(appearance) — (re)apply an appearance payload.
//   document.documentElement.dataset.hdAppearanceState — "compatible" |
//     "degraded" | "fallback" (observable for diagnostics/testing).
//   window.__HD_STATE__ — { level, reason, reducedMotion, appliedAt }.
(function () {
  "use strict";

  var ENGINE_VERSION = 1;

  // CSS selectors / tokens that identify the official Harness UI.
  var ROOT_SELECTOR = "#root";
  // Core token contract. If these no longer resolve, token-based theming is
  // unsafe and the engine must degrade rather than guess.
  var CORE_TOKENS = ["--dsw-alias-bg-base", "--dsw-alias-label-primary"];

  var IDS = {
    theme: "hd-theme",
    surfaces: "hd-surfaces",
    components: "hd-components",
    motion: "hd-motion",
    backdrop: "hd-backdrop",
    ambient: "hd-ambient",
    wallpaper: "hd-wallpaper",
    scrim: "hd-scrim",
    button: "hd-appearance-button",
  };

  var pending = null;
  var bootstrapped = false;
  var observer = null;
  var bootTimer = null;
  var lastLevel = null;

  function log() {
    try {
      if (window.console && console.info) {
        console.info.apply(console, ["[DeepSeek Harness Desktop appearance]"].concat(
          Array.prototype.slice.call(arguments)
        ));
      }
    } catch (_) { /* ignore */ }
  }

  function safe(fn) {
    try { return fn(); } catch (e) { log("error", e); return null; }
  }

  function el(id) {
    return document.getElementById(id);
  }

  function ensureStyle(id) {
    var node = el(id);
    if (!node) {
      node = document.createElement("style");
      node.id = id;
      node.setAttribute("data-hd-appearance", "true");
      (document.head || document.documentElement).appendChild(node);
    }
    return node;
  }

  function removeStyle(id) {
    var node = el(id);
    if (node) node.parentNode.removeChild(node);
  }

  function ensureBackdrop() {
    var node = el(IDS.backdrop);
    if (!node) {
      node = document.createElement("div");
      node.id = IDS.backdrop;
      node.setAttribute("aria-hidden", "true");
      node.setAttribute("data-hd-appearance", "true");
      node.style.cssText =
        "position:fixed;inset:0;z-index:-1;pointer-events:none;" +
        "overflow:hidden;contain:strict;";
      var wallpaper = document.createElement("div");
      wallpaper.id = IDS.wallpaper;
      var scrim = document.createElement("div");
      scrim.id = IDS.scrim;
      scrim.style.cssText = "position:absolute;inset:0;pointer-events:none;";
      node.appendChild(wallpaper);
      node.appendChild(scrim);
      (document.body || document.documentElement).appendChild(node);
    }
    return node;
  }

  function removeBackdrop() {
    var node = el(IDS.backdrop);
    if (node) node.parentNode.removeChild(node);
    removeStyle("hd-scrim-style");
  }

  function ensureButton() {
    var node = el(IDS.button);
    if (!node) {
      node = document.createElement("button");
      node.id = IDS.button;
      node.type = "button";
      node.setAttribute("aria-label", "Appearance");
      node.setAttribute("title", "Appearance");
      node.setAttribute("data-hd-appearance", "true");
      node.textContent = "\u2699"; // gear; replaced visually below
      node.addEventListener("click", function () {
        // Intercepted by the Rust backend (on_navigation) which opens the
        // appearance settings window. The Harness page itself never changes.
        try { window.location.href = "hd-appearance://open"; } catch (_) {}
      });
      (document.body || document.documentElement).appendChild(node);
    }
    return node;
  }

  function removeButton() {
    var node = el(IDS.button);
    if (node) node.parentNode.removeChild(node);
  }

  function buttonCss() {
    // Subtle, high-contrast, non-blocking control in the bottom-right corner.
    return (
      "#hd-appearance-button{position:fixed;right:16px;bottom:16px;z-index:2147483000;" +
      "width:36px;height:36px;border-radius:10px;border:1px solid rgba(128,128,128,.35);" +
      "background:rgba(30,30,34,.72);color:rgba(255,255,255,.85);cursor:pointer;" +
      "display:flex;align-items:center;justify-content:center;font:500 16px/1 " +
      "-apple-system,'SF Pro Text',sans-serif;backdrop-filter:blur(8px);" +
      "-webkit-backdrop-filter:blur(8px);box-shadow:0 4px 16px rgba(0,0,0,.25);" +
      "opacity:.62;transition:opacity .15s ease;font-family:inherit;}" +
      "#hd-appearance-button:hover{opacity:1;}" +
      "#hd-appearance-button:focus-visible{outline:2px solid rgba(120,160,255,.9);outline-offset:2px;}" +
      "@media (prefers-reduced-motion:reduce){#hd-appearance-button{transition:none;}}"
    );
  }

  function tokenRule(selector, map) {
    var parts = [];
    for (var k in map) {
      if (!Object.prototype.hasOwnProperty.call(map, k)) continue;
      var name = k.trim();
      var value = String(map[k]).trim();
      if (!name || !value) continue;
      // Only accept custom-property overrides; reject anything that could be a
      // CSS declaration like `background:url(...)` to keep themes token-scoped.
      if (name.slice(0, 2) !== "--") continue;
      if (/[;{}]/.test(value)) continue;
      parts.push(name + ":" + value + " !important");
    }
    if (!parts.length) return "";
    return selector + "{" + parts.join(";") + "}";
  }

  function themeTokenCss(theme) {
    var css = "";
    var tokens = theme && theme.tokens;
    if (tokens) {
      if (tokens.light) css += tokenRule("body", tokens.light) + "\n";
      if (tokens.dark) css += tokenRule("body[data-ds-dark-theme]", tokens.dark) + "\n";
    }
    return css;
  }

  function surfacesTokenCss(theme) {
    var css = "";
    var surfaces = theme && theme.surfaces;
    if (surfaces) {
      if (surfaces.light) css += tokenRule("body", surfaces.light) + "\n";
      if (surfaces.dark) css += tokenRule("body[data-ds-dark-theme]", surfaces.dark) + "\n";
    }
    return css;
  }

  // Default translucent surfaces used when a theme does not provide its own
  // (keeps the wallpaper visible against the official palette).
  function defaultSurfacesCss() {
    var light = {
      "--dsw-alias-bg-base": "rgba(255,255,255,.62)",
      "--dsw-alias-bg-layer-1": "rgba(255,255,255,.66)",
      "--dsw-alias-bg-layer-2": "rgba(255,255,255,.72)",
      "--dsw-alias-bg-layer-3": "rgba(255,255,255,.82)",
      "--dsw-alias-bg-module-platform": "rgba(248,249,250,.68)",
      "--dsw-specific-sidebar-fill": "rgba(247,248,249,.55)",
    };
    var dark = {
      "--dsw-alias-bg-base": "rgba(21,21,23,.55)",
      "--dsw-alias-bg-layer-1": "rgba(35,35,36,.60)",
      "--dsw-alias-bg-layer-2": "rgba(44,44,46,.68)",
      "--dsw-alias-bg-layer-3": "rgba(53,54,56,.76)",
      "--dsw-alias-bg-module-platform": "rgba(35,35,36,.66)",
      "--dsw-specific-sidebar-fill": "rgba(27,27,28,.5)",
    };
    return tokenRule("body", light) + "\n" + tokenRule("body[data-ds-dark-theme]", dark) + "\n";
  }

  function scrimCss(wp) {
    var strength = typeof wp.overlay === "number" ? wp.overlay : 0.5;
    strength = Math.max(0, Math.min(1, strength));
    if (strength <= 0.005) return "";
    var mode = wp.overlayMode || "auto";
    var color;
    if (mode === "light") {
      color = "255,255,255";
    } else if (mode === "dark") {
      color = "0,0,0";
    } else {
      // auto: follow the active Harness theme for correct contrast.
      return (
        "#hd-scrim{background:rgba(0,0,0," + (strength * 0.9).toFixed(3) + ");}" +
        "body[data-ds-dark-theme] #hd-scrim{background:rgba(0,0,0," +
        strength.toFixed(3) + ");}"
      );
    }
    return "#hd-scrim{background:rgba(" + color + "," + strength.toFixed(3) + ");}";
  }

  function setLevel(level, reason) {
    lastLevel = level;
    try {
      document.documentElement.setAttribute("data-hd-appearance-state", level);
    } catch (_) {}
    window.__HD_STATE__ = {
      level: level,
      reason: reason || "",
      reducedMotion: reducedMotion(),
      appliedAt: Date.now(),
      engineVersion: ENGINE_VERSION,
    };
    log("level=" + level + (reason ? " (" + reason + ")" : ""));
    // Best-effort diagnostic beacon to the Rust backend (no sensitive data).
    try {
      if (window.__HD_BEACON__ !== level) {
        window.__HD_BEACON__ = level;
        var img = new Image();
        img.src =
          "hd-beacon://state?level=" + encodeURIComponent(level) +
          "&reducedMotion=" + (reducedMotion() ? "1" : "0") +
          "&reason=" + encodeURIComponent(reason || "");
      }
    } catch (_) {}
  }

  function reducedMotion() {
    try {
      return !!(
        window.matchMedia &&
        window.matchMedia("(prefers-reduced-motion: reduce)").matches
      );
    } catch (_) {
      return false;
    }
  }

  function coreTokensPresent() {
    try {
      var cs = window.getComputedStyle(document.body);
      for (var i = 0; i < CORE_TOKENS.length; i++) {
        var v = cs.getPropertyValue(CORE_TOKENS[i]);
        if (!v || v.trim() === "") return false;
      }
      return true;
    } catch (_) {
      return false;
    }
  }

  function rootMounted() {
    var root = document.querySelector(ROOT_SELECTOR);
    if (!root) return false;
    return root.childElementCount > 0 || (root.textContent || "").trim().length > 0;
  }

  function clearAppearance() {
    removeStyle(IDS.theme);
    removeStyle(IDS.surfaces);
    removeStyle(IDS.components);
    removeStyle(IDS.motion);
    removeStyle("hd-scrim-style");
    removeStyle("hd-button-style");
    removeStyle("hd-ambient-style");
    removeBackdrop();
    removeButton();
  }

  function applyAppearance(cfg) {
    if (!cfg || typeof cfg !== "object") return;
    var mode = cfg.compatMode || "normal";
    var theme = cfg.theme || null;

    // Simulated mismatch hooks (used by the acceptance tests).
    if (mode === "simulate-fallback") {
      clearAppearance();
      setLevel("fallback", "simulated appearance-target mismatch");
      return;
    }

    if (!rootMounted()) {
      // Harness UI has not mounted yet (or this is not the Harness page).
      return;
    }

    var tokensOk = coreTokensPresent();

    if (mode === "simulate-degraded") {
      tokensOk = false;
    }

    if (!tokensOk) {
      // Token contract changed upstream: token/component/motion styling is
      // unsafe. Keep only the additive, safe pieces (wallpaper + button).
      removeStyle(IDS.theme);
      removeStyle(IDS.surfaces);
      removeStyle(IDS.components);
      removeStyle(IDS.motion);
      applyBackdrop(null, cfg.wallpaper);
      applyButton();
      setLevel("degraded", "core theme token contract unavailable");
      return;
    }

    // ---- Compatible: apply the full appearance ----
    applyButton();

    // Theme tokens.
    var tokenCss = themeTokenCss(theme);
    if (tokenCss) {
      ensureStyle(IDS.theme).textContent = tokenCss;
    } else {
      removeStyle(IDS.theme);
    }

    // Ambient theme layer + wallpaper + translucent surfaces (glass effect).
    applyBackdrop(theme, cfg.wallpaper);

    // Component styling (theme-provided, scoped CSS).
    var components = theme && theme.components;
    if (components && typeof components === "string" && components.trim()) {
      ensureStyle(IDS.components).textContent = components;
    } else {
      removeStyle(IDS.components);
    }

    // Motion (optional, and always disabled under reduced-motion).
    var motionEnabled = cfg.motionEnabled !== false;
    var motion = theme && theme.motion;
    if (
      motionEnabled &&
      !reducedMotion() &&
      motion &&
      typeof motion === "string" &&
      motion.trim()
    ) {
      ensureStyle(IDS.motion).textContent = motion;
    } else {
      removeStyle(IDS.motion);
    }

    setLevel("compatible", reducedMotion() ? "reduced-motion respected" : "");
  }

  function applyBackdrop(theme, wp) {
    var hasAmbient = !!(theme && theme.asset && typeof theme.asset === "string" && theme.asset.trim());
    var hasWallpaper = !!(wp && wp.active === true);

    // Ambient decorative asset (theme-provided). Rendered as a background-image
    // on the app frame (`#root > *`) so it sits above the frame's opaque
    // background color but below the content, and stays visible even without a
    // wallpaper. If the upstream frame selector changes, this simply stops
    // matching (safe no-op).
    if (hasAmbient) {
      ensureStyle("hd-ambient-style").textContent =
        "#root > * {" +
        "background-image:" + theme.asset + ";" +
        "background-size:cover;background-position:center;background-repeat:no-repeat;" +
        "}";
    } else {
      removeStyle("hd-ambient-style");
    }

    if (!hasWallpaper) {
      removeBackdrop();
      removeStyle(IDS.surfaces);
      return;
    }
    ensureBackdrop();

    // Wallpaper image.
    var wall = el(IDS.wallpaper);
    if (wall) {
      if (hasWallpaper) {
        var url = wp.url || "";
        var fit = wp.fit || "cover";
        var position = wp.position || "center";
        var opacity = typeof wp.opacity === "number" ? wp.opacity : 0.6;
        var blur = typeof wp.blur === "number" ? wp.blur : 0;
        opacity = Math.max(0, Math.min(1, opacity));
        blur = Math.max(0, Math.min(80, blur));
        var size =
          fit === "fill" ? "100% 100%" :
          fit === "contain" ? "contain" :
          fit === "auto" ? "auto" : "cover";
        wall.style.cssText =
          "position:absolute;inset:0;pointer-events:none;" +
          "background-image:url('" + url + "');" +
          "background-size:" + size + ";" +
          "background-position:" + position + ";" +
          "background-repeat:no-repeat;" +
          "opacity:" + opacity.toFixed(3) + ";" +
          (blur > 0.1
            ? "filter:blur(" + blur.toFixed(1) + "px);transform:scale(1.03);"
            : "filter:none;transform:none;");
      } else {
        wall.style.cssText =
          "position:absolute;inset:0;pointer-events:none;background-image:none;";
      }
    }

    // Readability overlay (scrim) — only for the wallpaper.
    var scrim = el(IDS.scrim);
    if (scrim) {
      if (hasWallpaper) {
        var sc = scrimCss(wp);
        if (sc) {
          ensureStyle("hd-scrim-style").textContent = sc;
        } else {
          removeStyle("hd-scrim-style");
          scrim.style.cssText = "position:absolute;inset:0;pointer-events:none;";
        }
      } else {
        removeStyle("hd-scrim-style");
        scrim.style.cssText = "position:absolute;inset:0;pointer-events:none;";
      }
    }

    // Translucent surfaces (theme-provided, else safe defaults) — only when a
    // wallpaper is active (the glass effect). The Harness body background is
    // made transparent so the wallpaper is not hidden behind an extra opaque
    // layer; panels keep a single translucent tint on top for readability.
    if (hasWallpaper) {
      var surfacesCss = surfacesTokenCss(theme);
      if (!surfacesCss.trim()) surfacesCss = defaultSurfacesCss();
      ensureStyle(IDS.surfaces).textContent =
        "body{background:transparent !important;}" + surfacesCss;
    } else {
      removeStyle(IDS.surfaces);
    }
  }

  function applyButton() {
    ensureButton();
    ensureStyle("hd-button-style").textContent = buttonCss();
  }

  function scheduleApply() {
    if (bootTimer) return;
    var attempts = 0;
    var maxAttempts = 200; // ~20s at 100ms
    bootTimer = setInterval(function () {
      attempts++;
      if (rootMounted()) {
        stop();
        bootstrapped = true;
        if (pending) { applyAppearance(pending); pending = null; }
      } else if (attempts >= maxAttempts) {
        stop();
        // Not the Harness page (loading page) or Harness never mounted:
        // leave the page untouched.
      }
    }, 100);
  }

  function stop() {
    if (bootTimer) { clearInterval(bootTimer); bootTimer = null; }
    if (observer) { observer.disconnect(); observer = null; }
  }

  function boot() {
    if (bootstrapped) return;
    // Distinguish the Harness page (has #root) from the local loading page.
    if (!document.getElementById("root")) {
      return; // loading page: no appearance applied here
    }
    scheduleApply();
  }

  // Public API for the Rust backend.
  window.__HD_APPLY__ = function (appearance) {
    if (bootstrapped) {
      applyAppearance(appearance);
    } else {
      pending = appearance;
      boot();
    }
  };

  // Pure helpers exposed for diagnostics and automated tests (no privileges).
  window.__HD_INTERNALS__ = {
    tokenRule: tokenRule,
    scrimCss: scrimCss,
    coreTokensPresent: coreTokensPresent,
  };

  window.__HD_STATE__ = window.__HD_STATE__ || null;

  // Initial payload supplied by the Rust backend at window creation time.
  if (!pending && window.__HD_INITIAL__) {
    pending = window.__HD_INITIAL__;
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
