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
    glass: "hd-glass",
    cinematic: "hd-cinematic",
    cinematicStyle: "hd-cinematic-style",
    backdrop: "hd-backdrop",
    ambient: "hd-ambient",
    wallpaper: "hd-wallpaper",
    video: "hd-video",
    scrim: "hd-scrim",
    button: "hd-appearance-button",
  };

  var pending = null;
  var bootstrapped = false;
  var observer = null;
  var bootTimer = null;
  var lastLevel = null;
  var lastMediaKey = null;
  var lastApplied = null;

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

      // Cinematic wrapper: the media (image/video) lives inside so the slow
      // scale/pan drift never moves the scrim or the UI.
      var cinematic = document.createElement("div");
      cinematic.id = IDS.cinematic;
      cinematic.style.cssText =
        "position:absolute;inset:0;pointer-events:none;overflow:hidden;" +
        "transition:opacity 260ms ease;";

      var wallpaper = document.createElement("div");
      wallpaper.id = IDS.wallpaper;
      wallpaper.style.cssText =
        "position:absolute;inset:0;pointer-events:none;background-image:none;";

      var video = document.createElement("video");
      video.id = IDS.video;
      video.setAttribute("muted", "");
      video.setAttribute("loop", "");
      video.setAttribute("playsinline", "");
      video.setAttribute("autoplay", "");
      video.setAttribute("aria-hidden", "true");
      video.setAttribute("data-hd-appearance", "true");
      video.style.cssText =
        "position:absolute;inset:0;pointer-events:none;width:100%;height:100%;" +
        "object-fit:cover;object-position:center;display:none;";

      var scrim = document.createElement("div");
      scrim.id = IDS.scrim;
      scrim.style.cssText = "position:absolute;inset:0;pointer-events:none;";

      cinematic.appendChild(wallpaper);
      cinematic.appendChild(video);
      node.appendChild(cinematic);
      node.appendChild(scrim);
      (document.body || document.documentElement).appendChild(node);
    }
    return node;
  }

  function removeBackdrop() {
    var node = el(IDS.backdrop);
    if (node) node.parentNode.removeChild(node);
    removeStyle("hd-scrim-style");
    removeStyle(IDS.cinematicStyle);
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

  // The surface tokens the glass-depth control is allowed to modulate. Brand,
  // label, accent and other tokens are never scaled (that would fade text or
  // brand colors and hurt contrast).
  var SURFACE_KEYS = {
    "--dsw-alias-bg-base": true,
    "--dsw-alias-bg-layer-1": true,
    "--dsw-alias-bg-layer-2": true,
    "--dsw-alias-bg-layer-3": true,
    "--dsw-alias-bg-module-platform": true,
    "--dsw-specific-sidebar-fill": true,
  };

  // Glass depth → material mapping. One product-level value (0..100) drives the
  // whole glass material, not just surface opacity:
  //   alpha   — surface tint transparency (0 = theme baseline)
  //   blur    — backdrop blur radius (px) on the glass layer
  //   saturate— backdrop saturation boost
  //   scrim   — global readability-overlay reduction (wallpaper stays vivid)
  //   edge    — glass edge-highlight intensity
  // All values are clamped so a corrupt/manual config is safe.
  function depthT(depth) {
    var d = typeof depth === "number" ? depth : 0;
    return Math.max(0, Math.min(100, d)) / 100;
  }

  // Per-surface depth response. CLEAR GLASS: higher depth makes the LARGE
  // surfaces (main canvas + sidebar) MORE transparent so the scene becomes
  // clearer, while the composer/dialog surfaces stay readable. Returns an alpha
  // multiplier interpolated from 1.0 (depth 0) to the key's clear-glass floor
  // (depth 100). Brand/label tokens are never scaled (handled by scaleSurfaceMap).
  function depthKeyFactor(key, t) {
    if (key === "--dsw-alias-bg-base") return 1 - 0.80 * t;          // main canvas 1.0 -> 0.20
    if (key === "--dsw-specific-sidebar-fill") return 1 - 0.60 * t;  // sidebar 1.0 -> 0.40
    if (key === "--dsw-alias-bg-layer-1") return 1 - 0.30 * t;       // cards 1.0 -> 0.70
    if (key === "--dsw-alias-bg-layer-2") return 1 - 0.20 * t;       // elevated 1.0 -> 0.80
    if (key === "--dsw-alias-bg-layer-3") return 1 - 0.07 * t;       // composer/dialog 1.0 -> 0.93
    if (key === "--dsw-alias-bg-module-platform") return 1 - 0.25 * t; // panels 1.0 -> 0.75
    return 1;
  }

  // Local composer blur only, bounded. NOT whole-viewport and NOT driven to a
  // maximum by depth (depth 100 must be the CLEAREST scene, not the blurriest).
  function depthToBlur(depth) {
    return 12;
  }
  function depthToSaturate(depth) {
    return 1.0; // no global saturation
  }
  function depthToScrimFactor(depth) {
    return 1 - 0.65 * depthT(depth); // scene stays clear at high depth
  }
  function depthToEdge(depth) {
    return 0.05 + 0.15 * depthT(depth); // subtle edge, modest increase
  }

  function clampAlpha(a) {
    // Clear-glass floor: the MAIN canvas may approach transparency so the scene
    // stays clear. Keep a tiny floor + ceiling so nothing becomes fully invisible
    // or fully opaque via the depth control.
    return Math.max(0.02, Math.min(0.98, a));
  }

  function parseAlphaComponent(s) {
    var v = parseFloat(String(s).trim());
    if (!isFinite(v)) return NaN;
    if (String(s).indexOf("%") >= 0) v = v / 100;
    return v;
  }

  // Scale the alpha of a single validated rgb()/rgba() color value. Non-color
  // or unparsable values are returned untouched (fail-safe, never guessed).
  function scaleColorAlpha(value, factor) {
    var s = String(value).trim();
    var m = /^rgba\(\s*([^,]+)\s*,\s*([^,]+)\s*,\s*([^,]+)\s*,\s*([^)]+)\s*\)$/.exec(s);
    if (m) {
      var a = parseAlphaComponent(m[4]);
      if (!isFinite(a)) return s;
      return "rgba(" + m[1].trim() + "," + m[2].trim() + "," + m[3].trim() + "," +
        clampAlpha(a * factor).toFixed(3) + ")";
    }
    var m2 = /^rgb\(\s*([^,]+)\s*,\s*([^,]+)\s*,\s*([^)]+)\s*\)$/.exec(s);
    if (m2) {
      if (factor >= 1) return s; // fully opaque already; nothing to deepen
      return "rgba(" + m2[1].trim() + "," + m2[2].trim() + "," + m2[3].trim() + "," +
        clampAlpha(factor).toFixed(3) + ")";
    }
    return s;
  }

  // Scale only the surface keys of a token/surface map by the per-key depth
  // factor. Brand/label tokens are never scaled.
  function scaleSurfaceMap(map, t) {
    if (t <= 0.001) return map; // depth 0: theme baseline unchanged
    var out = {};
    for (var k in map) {
      if (!Object.prototype.hasOwnProperty.call(map, k)) continue;
      var v = String(map[k]);
      var f = SURFACE_KEYS[k] ? depthKeyFactor(k, t) : 1;
      out[k] = f >= 0.999 ? v : scaleColorAlpha(v, f);
    }
    return out;
  }

  // Pure decision for whether motion CSS may be injected. Kept separate so the
  // reduce-motion / toggle behaviour is unit-testable without a DOM.
  function shouldApplyMotion(motionEnabled, reducedMotionActive, hasMotion) {
    return motionEnabled === true && !reducedMotionActive && !!hasMotion;
  }

  function themeTokenCss(theme, t) {
    var css = "";
    var tokens = theme && theme.tokens;
    if (tokens) {
      if (tokens.light) css += tokenRule("body", scaleSurfaceMap(tokens.light, t)) + "\n";
      if (tokens.dark) css += tokenRule("body[data-ds-dark-theme]", scaleSurfaceMap(tokens.dark, t)) + "\n";
    }
    return css;
  }

  function surfacesTokenCss(theme, t) {
    var css = "";
    var surfaces = theme && theme.surfaces;
    if (surfaces) {
      if (surfaces.light) css += tokenRule("body", scaleSurfaceMap(surfaces.light, t)) + "\n";
      if (surfaces.dark) css += tokenRule("body[data-ds-dark-theme]", scaleSurfaceMap(surfaces.dark, t)) + "\n";
    }
    return css;
  }

  // Default translucent surfaces used when a theme does not provide its own
  // (keeps the wallpaper visible against the official palette).
  function defaultSurfacesCss(t) {
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
    return tokenRule("body", scaleSurfaceMap(light, t)) + "\n" +
      tokenRule("body[data-ds-dark-theme]", scaleSurfaceMap(dark, t)) + "\n";
  }

  function scrimCss(wp, scrimFactor) {
    var strength = typeof wp.overlay === "number" ? wp.overlay : 0.5;
    strength = Math.max(0, Math.min(1, strength));
    // Glass depth reduces the global veil so the wallpaper stays vivid; local
    // surface tint + backdrop blur keep text readable instead.
    strength *= Math.max(0, Math.min(1, scrimFactor));
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

  // CLEAR GLASS material: LOCAL surfaces only. The composer/code surfaces get a
  // bounded backdrop blur + a subtle edge highlight. The main canvas / #root /
  // frame get NO viewport-wide blur (that was the "dirty frost" bug). Gated
  // behind @supports so unsupported WebViews degrade to translucent surfaces.
  function glassMaterialCss(material) {
    var css = "";
    if (material.blur >= 1) {
      css +=
        "@supports ((-webkit-backdrop-filter: blur(1px)) or (backdrop-filter: blur(1px))){" +
        "#root [contenteditable=\"true\"],#root textarea{" +
        "-webkit-backdrop-filter:blur(" + material.blur + "px);" +
        "backdrop-filter:blur(" + material.blur + "px);" +
        "}" +
        "}";
    }
    if (material.edge > 0.001) {
      css +=
        "#root [contenteditable=\"true\"],#root textarea,#root pre{" +
        "box-shadow:inset 0 1px 0 rgba(255,255,255," + material.edge.toFixed(3) + ")," +
        "0 1px 6px rgba(0,0,0," + (material.edge * 0.5).toFixed(3) + ");" +
        "}";
    }
    return css;
  }

  // Slow cinematic background-only motion (scale drift + pan) on the media
  // wrapper. Never animates text/composer/dialogs. The engine injects this only
  // when motion is enabled and reduced-motion is off; the media query is a
  // second line of defense.
  function cinematicCss() {
    return (
      "@keyframes hd-cinematic-drift{" +
      "from{transform:scale(1.00) translate3d(0,0,0);}" +
      "to{transform:scale(1.04) translate3d(-0.6%,-0.4%,0);}" +
      "}" +
      "#hd-cinematic{transform-origin:center center;will-change:transform;" +
      "animation:hd-cinematic-drift 32s ease-in-out infinite alternate;}" +
      "@media (prefers-reduced-motion: reduce){#hd-cinematic{animation:none;}}"
    );
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
      theme: lastApplied ? lastApplied.theme : null,
      glassDepth: lastApplied ? lastApplied.glassDepth : null,
      wallpaperActive: lastApplied ? lastApplied.wallpaperActive : null,
      mediaType: lastApplied ? lastApplied.mediaType : null,
      motionEnabled: lastApplied ? lastApplied.motionEnabled : null,
      error: window.__HD_STATE__ && window.__HD_STATE__.error ? window.__HD_STATE__.error : null,
    };
    log("level=" + level + (reason ? " (" + reason + ")" : ""));
    probeRealDom();
    // Best-effort diagnostic beacon to the Rust backend (no sensitive data).
    // Re-fires whenever the applied config actually changes so the main-window
    // engine state is observable end-to-end.
    try {
      var beaconKey =
        level + "|" +
        (lastApplied ? lastApplied.theme : "") + "|" +
        (lastApplied ? lastApplied.glassDepth : "") + "|" +
        (lastApplied && lastApplied.wallpaperActive ? "1" : "0") + "|" +
        (lastApplied ? lastApplied.mediaType : "");
      if (window.__HD_BEACON__ !== beaconKey) {
        window.__HD_BEACON__ = beaconKey;
        var img = new Image();
        img.src =
          "hd-beacon://state?level=" + encodeURIComponent(level) +
          "&reducedMotion=" + (reducedMotion() ? "1" : "0") +
          "&theme=" + encodeURIComponent(lastApplied ? lastApplied.theme : "") +
          "&glassDepth=" + encodeURIComponent(lastApplied ? lastApplied.glassDepth : "") +
          "&wallpaper=" + (lastApplied && lastApplied.wallpaperActive ? "1" : "0") +
          "&mediaType=" + encodeURIComponent(lastApplied ? lastApplied.mediaType : "") +
          "&motion=" + (lastApplied && lastApplied.motionEnabled ? "1" : "0") +
          "&reason=" + encodeURIComponent(reason || "");
      }
    } catch (_) {}
  }

  // REAL-DOM probe: reports the actual computed styles of the elements that
  // gate wallpaper/glass visibility. Emitted as a `hd-beacon://dom` image so the
  // Rust backend logs it. Non-sensitive (no credentials/sessions/paths).
  function probeRealDom() {
    try {
      function cs(el) {
        if (!el) return null;
        var s = window.getComputedStyle(el);
        return {
          bg: s.backgroundColor || "",
          bgImage: (s.backgroundImage || "").slice(0, 80),
          bf: (s.backdropFilter || s.webkitBackdropFilter || "").slice(0, 40),
          pos: s.position || "",
          z: s.zIndex || "auto",
        };
      }
      function tag(sel) {
        var el = document.querySelector(sel);
        if (!el) return null;
        return {
          cls: (el.className && String(el.className)) || "",
          tag: el.tagName,
          cs: cs(el),
        };
      }
      function composerParent() {
        var ta = document.querySelector("#root textarea, #root [contenteditable=\"true\"]");
        if (!ta) return null;
        // Walk up to find the composer container that actually paints the
        // `--dsw-specific-input-major` background (the textarea itself is
        // transparent).
        var n = ta.parentElement;
        for (var i = 0; i < 4 && n; i++) {
          var bg = (n && n.style) ? "" : "";
          try { bg = window.getComputedStyle(n).backgroundColor; } catch (_) {}
          if (bg && bg !== "rgba(0, 0, 0, 0)" && bg !== "transparent") {
            return {
              cls: (n.className && String(n.className)) || "",
              tag: n.tagName,
              cs: cs(n),
            };
          }
          n = n.parentElement;
        }
        return null;
      }
      function kids(el) {
        if (!el) return null;
        var arr = [];
        for (var i = 0; i < el.children.length; i++) {
          var c = el.children[i];
          arr.push((c.tagName || "?") + "#" + (c.id || "") + "." + (String(c.className || "").split(/\s+/)[0] || ""));
        }
        return arr;
      }
      var out = {
        body: cs(document.body),
        root: cs(document.getElementById("root")),
        frame: tag("#root > *"),
        frameChild: tag("#root > * > *"),
        sidebar: tag("#root > * > * > *"),
        composer: tag("#root [contenteditable=\"true\"], #root textarea"),
        composerParent: composerParent(),
        backdrop: tag("#hd-backdrop"),
        wallpaper: tag("#hd-wallpaper"),
        video: tag("#hd-video"),
        cinematic: tag("#hd-cinematic"),
        scrim: tag("#hd-scrim"),
        backdropKids: kids(document.getElementById("hd-backdrop")),
        cinematicKids: kids(document.getElementById("hd-cinematic")),
      };
      var json = JSON.stringify(out);
      if (json.length > 2600) json = json.slice(0, 2600);
      log("dom-probe", json);
      var img = new Image();
      img.src = "hd-beacon://dom?d=" + encodeURIComponent(json);
    } catch (e) {
      log("dom-probe error", e);
    }
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
    removeStyle(IDS.glass);
    removeStyle(IDS.cinematicStyle);
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
    lastApplied = {
      theme: theme && theme.id ? theme.id : "",
      glassDepth: typeof cfg.glassDepth === "number" ? cfg.glassDepth : 0,
      wallpaperActive: !!(cfg.wallpaper && cfg.wallpaper.active === true),
      mediaType: (cfg.wallpaper && cfg.wallpaper.mediaType) || "image",
      motionEnabled: cfg.motionEnabled !== false,
    };
    var material = {
      t: depthT(cfg.glassDepth),
      blur: depthToBlur(cfg.glassDepth),
      saturate: depthToSaturate(cfg.glassDepth),
      scrim: depthToScrimFactor(cfg.glassDepth),
      edge: depthToEdge(cfg.glassDepth),
    };

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
      removeStyle(IDS.glass);
      applyBackdrop(null, cfg.wallpaper, material);
      applyButton();
      setLevel("degraded", "core theme token contract unavailable");
      return;
    }

    // ---- Compatible: apply the full appearance ----
    applyButton();

    // Theme tokens.
    var tokenCss = themeTokenCss(theme, material.t);
    if (tokenCss) {
      ensureStyle(IDS.theme).textContent = tokenCss;
    } else {
      removeStyle(IDS.theme);
    }

    // Ambient theme layer + wallpaper/video + translucent surfaces + glass.
    applyBackdrop(theme, cfg.wallpaper, material);

    // Component styling (theme-provided, scoped CSS).
    var components = theme && theme.components;
    if (components && typeof components === "string" && components.trim()) {
      ensureStyle(IDS.components).textContent = components;
    } else {
      removeStyle(IDS.components);
    }

    // Theme motion (ambient drift) + engine cinematic background motion.
    var motionEnabled = cfg.motionEnabled !== false;
    var motion = theme && theme.motion;
    var hasMotion = motion && typeof motion === "string" && motion.trim();
    if (shouldApplyMotion(motionEnabled, reducedMotion(), hasMotion)) {
      ensureStyle(IDS.motion).textContent = motion;
    } else {
      removeStyle(IDS.motion);
    }
    // Cinematic pan/scale applies ONLY to a static IMAGE (the image itself is
    // still, so subtle motion adds life). A video already supplies its own
    // motion — never add an extra transform to it.
    var isVideoBg = !!(cfg.wallpaper && cfg.wallpaper.mediaType === "video");
    var hasStaticImage = !!(cfg.wallpaper && cfg.wallpaper.active === true) && !isVideoBg;
    if (shouldApplyMotion(motionEnabled, reducedMotion(), hasStaticImage)) {
      ensureStyle(IDS.cinematicStyle).textContent = cinematicCss();
    } else {
      removeStyle(IDS.cinematicStyle);
    }

    setLevel("compatible", reducedMotion() ? "reduced-motion respected" : "");
  }

  function applyBackdrop(theme, wp, material) {
    var hasAmbient = !!(theme && theme.asset && typeof theme.asset === "string" && theme.asset.trim());
    var hasWallpaper = !!(wp && wp.active === true && wp.url);
    var isVideo = !!(wp && wp.mediaType === "video");

    // Ambient decorative asset renders ONLY when there is no wallpaper/video.
    // When a wallpaper is active it is the background and must stay vivid, not
    // sit under a full-frame gradient veil (the washed-out look's root cause).
    if (hasAmbient && !hasWallpaper) {
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
      removeStyle(IDS.glass);
      return;
    }
    ensureBackdrop();

    // Real glass material (backdrop blur + edge highlight).
    var glassCss = glassMaterialCss(material);
    if (glassCss) {
      ensureStyle(IDS.glass).textContent = glassCss;
    } else {
      removeStyle(IDS.glass);
    }

    // Media (image or video). Crossfade on a real change.
    var url = wp.url || "";
    var fit = wp.fit || "cover";
    var position = wp.position || "center";
    var opacity = typeof wp.opacity === "number" ? wp.opacity : 0.6;
    var blur = typeof wp.blur === "number" ? wp.blur : 0;
    opacity = Math.max(0, Math.min(1, opacity));
    blur = Math.max(0, Math.min(80, blur));
    var mediaKey = (isVideo ? "v:" : "i:") + url + "|" + fit + "|" + position + "|" + blur.toFixed(1);

    var applyMedia = function () {
      if (isVideo) {
        renderVideo(url, fit, position, opacity, blur);
      } else {
        renderImage(url, fit, position, opacity, blur);
      }
    };

    if (mediaKey !== lastMediaKey) {
      if (lastMediaKey === null) {
        applyMedia(); // first render: no fade-in delay at boot
      } else {
        crossfadeMedia(applyMedia);
      }
      lastMediaKey = mediaKey;
    } else {
      applyMedia();
    }

    // Readability overlay (scrim) — reduced by glass depth.
    var scrim = el(IDS.scrim);
    if (scrim) {
      var sc = scrimCss(wp, material.scrim);
      if (sc) {
        ensureStyle("hd-scrim-style").textContent = sc;
      } else {
        removeStyle("hd-scrim-style");
        scrim.style.cssText = "position:absolute;inset:0;pointer-events:none;";
      }
    }

    // Translucent surfaces (theme-provided, else safe defaults) + transparent
    // body so the background shows through.
    var surfacesCss = surfacesTokenCss(theme, material.t);
    if (!surfacesCss.trim()) surfacesCss = defaultSurfacesCss(material.t);
    ensureStyle(IDS.surfaces).textContent =
      "body{background:transparent !important;}" + surfacesCss;
  }

  function renderImage(url, fit, position, opacity, blur) {
    var wall = el(IDS.wallpaper);
    var video = el(IDS.video);
    if (video) {
      try { video.pause(); } catch (_) {}
      video.style.display = "none";
      video.removeAttribute("src");
    }
    if (!wall) return;
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
  }

  function renderVideo(url, fit, position, opacity, blur) {
    var wall = el(IDS.wallpaper);
    var video = el(IDS.video);
    if (wall) wall.style.cssText = "position:absolute;inset:0;pointer-events:none;background-image:none;";
    if (!video) return;
    var objectFit =
      fit === "fill" ? "fill" :
      fit === "contain" ? "contain" :
      fit === "auto" ? "none" : "cover";
    video.style.cssText =
      "position:absolute;inset:0;pointer-events:none;width:100%;height:100%;" +
      "object-fit:" + objectFit + ";" +
      "object-position:" + position + ";" +
      "opacity:" + opacity.toFixed(3) + ";" +
      (blur > 0.1 ? "filter:blur(" + blur.toFixed(1) + "px);" : "filter:none;");
    video.setAttribute("muted", "");
    video.setAttribute("loop", "");
    video.setAttribute("playsinline", "");
    video.setAttribute("autoplay", "");
    if (video.src !== url) {
      video.src = url;
    }
    video.style.display = "block";
    try {
      var p = video.play();
      if (p && typeof p.catch === "function") {
        p.catch(function () { /* autoplay blocked — retry on first user gesture */ });
      }
    } catch (_) {}
  }

  function crossfadeMedia(applyFn) {
    var cinematic = el(IDS.cinematic);
    if (!cinematic || reducedMotion()) {
      applyFn();
      return;
    }
    cinematic.style.opacity = "0";
    setTimeout(function () {
      applyFn();
      cinematic.style.opacity = "1";
    }, 260);
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
    try {
      if (bootstrapped) {
        applyAppearance(appearance);
      } else {
        pending = appearance;
        boot();
      }
    } catch (e) {
      log("apply error", e);
      try {
        window.__HD_STATE__ = window.__HD_STATE__ || {};
        window.__HD_STATE__.error = String((e && e.message) || e);
      } catch (_) {}
    }
  };

  // Pure helpers exposed for diagnostics and automated tests (no privileges).
  window.__HD_INTERNALS__ = {
    tokenRule: tokenRule,
    scrimCss: scrimCss,
    coreTokensPresent: coreTokensPresent,
    depthT: depthT,
    depthKeyFactor: depthKeyFactor,
    depthToBlur: depthToBlur,
    depthToSaturate: depthToSaturate,
    depthToScrimFactor: depthToScrimFactor,
    depthToEdge: depthToEdge,
    scaleColorAlpha: scaleColorAlpha,
    scaleSurfaceMap: scaleSurfaceMap,
    shouldApplyMotion: shouldApplyMotion,
    glassMaterialCss: glassMaterialCss,
    cinematicCss: cinematicCss,
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
