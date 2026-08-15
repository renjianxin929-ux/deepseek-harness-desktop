// Harness Desktop — appearance settings window.
// Communicates with the Rust backend over Tauri IPC (this window runs on the
// app's own origin). Wallpaper selection reads the file in the browser and
// sends its bytes to Rust; the image never leaves the machine.

import { invoke } from "@tauri-apps/api/core";

interface ThemeMeta {
  id: string;
  name: string;
  description: string;
  kind: string;
}

interface WallpaperSettings {
  active: boolean;
  fileName: string | null;
  fit: string;
  position: string;
  opacity: number;
  blur: number;
  overlay: number;
  overlayMode: string;
  version: number;
}

interface AppearanceSnapshot {
  activeTheme: string;
  motionEnabled: boolean;
  wallpaper: WallpaperSettings;
  themes: ThemeMeta[];
  compatMode: string;
  language: string;
}

type Lang = "zh-CN" | "en";

const STRINGS: Record<Lang, Record<string, string>> = {
  en: {
    appearance: "Appearance",
    subtitle: "Keep Harness official. Make it yours.",
    language: "Language",
    langSystem: "System",
    theme: "Theme",
    wallpaper: "Wallpaper",
    chooseImage: "Choose image…",
    wallpaperHint: "Stored locally on this Mac. Never uploaded.",
    noWallpaper: "No wallpaper selected.",
    fit: "Fit",
    fitCover: "Cover",
    fitContain: "Contain",
    fitFill: "Fill",
    fitAuto: "Original size",
    position: "Position",
    posCenter: "Center",
    posTop: "Top",
    posBottom: "Bottom",
    posLeft: "Left",
    posRight: "Right",
    posTopLeft: "Top left",
    posTopRight: "Top right",
    posBottomLeft: "Bottom left",
    posBottomRight: "Bottom right",
    overlayTone: "Overlay tone",
    overlayAuto: "Auto (follow theme)",
    overlayDark: "Dark",
    overlayLight: "Light",
    opacity: "Opacity",
    blur: "Blur",
    overlay: "Readability overlay",
    removeWallpaper: "Remove wallpaper",
    motion: "Motion",
    motionHint:
      "Lightweight, decorative motion. Respects the system “Reduce Motion” setting.",
    resetToOfficial: "Reset to Official",
    officialDesc: "The untouched DeepSeek Harness appearance.",
    oceanDesc: "Restrained ocean-inspired showcase.",
    starterDesc: "A documented template for your own theme.",
    customTheme: "Custom theme",
    builtIn: "Built-in",
    themeApplied: "Theme applied",
    wallpaperApplied: "Wallpaper applied",
    wallpaperRemoved: "Wallpaper removed",
    resetDone: "Reset to Official",
    readingImage: "Reading image…",
    errorPrefix: "Error:",
    errorLoading: "Error loading appearance:",
    unsupportedType: "Unsupported file type. Choose a PNG, JPG/JPEG, or WebP image.",
    imageTooLarge: "Image too large (max 24 MB).",
    off: "off",
  },
  "zh-CN": {
    appearance: "外观",
    subtitle: "保持官方 Harness，让它成为你的专属。",
    language: "语言",
    langSystem: "跟随系统",
    theme: "主题",
    wallpaper: "壁纸",
    chooseImage: "选择图片…",
    wallpaperHint: "仅保存在本机，绝不上传。",
    noWallpaper: "未选择壁纸。",
    fit: "适配方式",
    fitCover: "覆盖",
    fitContain: "包含",
    fitFill: "拉伸",
    fitAuto: "原始大小",
    position: "位置",
    posCenter: "居中",
    posTop: "顶部",
    posBottom: "底部",
    posLeft: "左侧",
    posRight: "右侧",
    posTopLeft: "左上",
    posTopRight: "右上",
    posBottomLeft: "左下",
    posBottomRight: "右下",
    overlayTone: "遮罩色调",
    overlayAuto: "自动（跟随主题）",
    overlayDark: "深色",
    overlayLight: "浅色",
    opacity: "透明度",
    blur: "模糊",
    overlay: "可读性遮罩",
    removeWallpaper: "移除壁纸",
    motion: "动效",
    motionHint: "轻量装饰动效，遵循系统“减少动态效果”设置。",
    resetToOfficial: "恢复官方样式",
    officialDesc: "未修改的 DeepSeek Harness 原始外观。",
    oceanDesc: "克制的海洋风展示主题。",
    starterDesc: "用于自建主题的文档化模板。",
    customTheme: "自定义主题",
    builtIn: "内置",
    themeApplied: "主题已应用",
    wallpaperApplied: "壁纸已应用",
    wallpaperRemoved: "壁纸已移除",
    resetDone: "已恢复官方样式",
    readingImage: "正在读取图片…",
    errorPrefix: "错误：",
    errorLoading: "加载外观设置出错：",
    unsupportedType: "不支持的文件类型，请选择 PNG、JPG/JPEG 或 WebP 图片。",
    imageTooLarge: "图片过大（最大 24 MB）。",
    off: "关",
  },
};

const MAX_WALLPAPER_BYTES = 24 * 1024 * 1024;

let current: AppearanceSnapshot | null = null;
let busy = false;
let lang: Lang = "en";

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el as T;
};

function setStatus(msg: string): void {
  $("hd-status").textContent = msg;
}

function detectSystemLang(): Lang {
  try {
    const nav = (navigator.language || "en").toLowerCase();
    return nav.startsWith("zh") ? "zh-CN" : "en";
  } catch {
    return "en";
  }
}

function t(key: string): string {
  return STRINGS[lang][key] ?? STRINGS.en[key] ?? key;
}

function applyLanguage(): void {
  const pref = current?.language ?? "system";
  lang = pref === "zh-CN" ? "zh-CN" : pref === "en" ? "en" : detectSystemLang();

  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    const key = el.getAttribute("data-i18n");
    if (key) el.textContent = t(key);
  });
  document.querySelectorAll<HTMLElement>("[data-i18n-option]").forEach((el) => {
    const key = el.getAttribute("data-i18n-option");
    if (key) el.textContent = t(key);
  });

  ($("hd-language") as HTMLSelectElement).value = pref;
  render();
}

async function refresh(): Promise<AppearanceSnapshot> {
  const snap = await invoke<AppearanceSnapshot>("get_appearance");
  current = snap;
  applyLanguage();
  return snap;
}

function render(): void {
  if (!current) return;
  renderThemes();
  renderWallpaper();
  ($("hd-motion") as HTMLInputElement).checked = current.motionEnabled;
}

function themeDescription(theme: ThemeMeta): string {
  if (theme.id === "official") return t("officialDesc");
  if (theme.id === "ocean") return t("oceanDesc");
  if (theme.id === "starter") return t("starterDesc");
  return theme.description || (theme.kind === "custom" ? t("customTheme") : t("builtIn"));
}

function renderThemes(): void {
  const container = $("hd-themes");
  container.innerHTML = "";
  for (const theme of current!.themes) {
    const card = document.createElement("button");
    card.type = "button";
    card.className = "hd-theme-card";
    card.setAttribute("role", "radio");
    card.setAttribute("aria-checked", String(theme.id === current!.activeTheme));
    card.dataset.themeId = theme.id;
    if (theme.id === current!.activeTheme) card.classList.add("selected");

    const name = document.createElement("span");
    name.className = "hd-theme-name";
    name.textContent = theme.name;

    const meta = document.createElement("span");
    meta.className = "hd-theme-meta";
    meta.textContent = themeDescription(theme);

    card.appendChild(name);
    card.appendChild(meta);
    card.addEventListener("click", () => selectTheme(theme.id));
    container.appendChild(card);
  }
}

function renderWallpaper(): void {
  const wp = current!.wallpaper;
  $("hd-wallpaper-empty").classList.toggle("hidden", wp.active);
  $("hd-wallpaper-controls").classList.toggle("hidden", !wp.active);
  if (!wp.active) return;

  ($("hd-fit") as HTMLSelectElement).value = wp.fit;
  ($("hd-position") as HTMLSelectElement).value = wp.position;
  ($("hd-overlay-mode") as HTMLSelectElement).value = wp.overlayMode;
  const opacity = $("hd-opacity") as HTMLInputElement;
  opacity.value = String(wp.opacity);
  $("hd-opacity-val").textContent = `${Math.round(wp.opacity * 100)}%`;
  const blur = $("hd-blur") as HTMLInputElement;
  blur.value = String(wp.blur);
  $("hd-blur-val").textContent = wp.blur > 0 ? `${wp.blur}px` : t("off");
  const overlay = $("hd-overlay") as HTMLInputElement;
  overlay.value = String(wp.overlay);
  $("hd-overlay-val").textContent = `${Math.round(wp.overlay * 100)}%`;
}

async function selectTheme(id: string): Promise<void> {
  await run(async () => {
    await invoke("set_theme", { themeId: id });
    setStatus(t("themeApplied"));
  });
}

async function run(fn: () => Promise<void>): Promise<void> {
  if (busy) return;
  busy = true;
  try {
    await fn();
    await refresh();
  } catch (e) {
    setStatus(`${t("errorPrefix")} ${String(e)}`);
    console.error(e);
  } finally {
    busy = false;
  }
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("read failed"));
    reader.readAsDataURL(file);
  });
}

function pushWallpaperSettings(): void {
  if (!current || !current.wallpaper.active) return;
  const settings: Partial<WallpaperSettings> = {
    active: true,
    fit: ($("hd-fit") as HTMLSelectElement).value,
    position: ($("hd-position") as HTMLSelectElement).value,
    overlayMode: ($("hd-overlay-mode") as HTMLSelectElement).value,
    opacity: Number(($("hd-opacity") as HTMLInputElement).value),
    blur: Number(($("hd-blur") as HTMLInputElement).value),
    overlay: Number(($("hd-overlay") as HTMLInputElement).value),
  };
  void run(async () => {
    await invoke("set_wallpaper", { settings });
  });
}

function bindControls(): void {
  $("hd-language").addEventListener("change", (e) => {
    void run(async () => {
      await invoke("set_language", { language: (e.target as HTMLSelectElement).value });
    });
  });

  $("hd-wallpaper-pick").addEventListener("click", () => {
    $("hd-wallpaper-file").click();
  });
  $("hd-wallpaper-file").addEventListener("change", async (e) => {
    const input = e.target as HTMLInputElement;
    const file = input.files && input.files[0];
    input.value = "";
    if (!file) return;
    // Localized pre-check before sending bytes to Rust (Rust remains the
    // authoritative validator).
    if (!/\.(png|jpe?g|webp)$/i.test(file.name)) {
      setStatus(t("unsupportedType"));
      return;
    }
    if (file.size > MAX_WALLPAPER_BYTES) {
      setStatus(t("imageTooLarge"));
      return;
    }
    await run(async () => {
      setStatus(t("readingImage"));
      const data = await readFileAsDataUrl(file);
      await invoke("set_wallpaper_bytes", { payload: { name: file.name, data } });
      setStatus(t("wallpaperApplied"));
    });
  });

  $("hd-wallpaper-remove").addEventListener("click", () => {
    void run(async () => {
      await invoke("remove_wallpaper");
      setStatus(t("wallpaperRemoved"));
    });
  });

  for (const id of ["hd-fit", "hd-position", "hd-overlay-mode"] as const) {
    $(id).addEventListener("change", pushWallpaperSettings);
  }
  for (const id of ["hd-opacity", "hd-blur", "hd-overlay"] as const) {
    $(id).addEventListener("input", () => {
      if (id === "hd-opacity") {
        $("hd-opacity-val").textContent = `${Math.round(Number(($("hd-opacity") as HTMLInputElement).value) * 100)}%`;
      }
      if (id === "hd-blur") {
        const v = Number(($("hd-blur") as HTMLInputElement).value);
        $("hd-blur-val").textContent = v > 0 ? `${v}px` : t("off");
      }
      if (id === "hd-overlay") {
        $("hd-overlay-val").textContent = `${Math.round(Number(($("hd-overlay") as HTMLInputElement).value) * 100)}%`;
      }
    });
    $(id).addEventListener("change", pushWallpaperSettings);
  }

  $("hd-motion").addEventListener("change", (e) => {
    void run(async () => {
      await invoke("set_motion", { enabled: (e.target as HTMLInputElement).checked });
    });
  });

  $("hd-reset").addEventListener("click", () => {
    void run(async () => {
      await invoke("reset_appearance");
      setStatus(t("resetDone"));
    });
  });
}

async function main(): Promise<void> {
  bindControls();
  try {
    await refresh();
    setStatus("");
  } catch (e) {
    setStatus(`${t("errorLoading")} ${String(e)}`);
    console.error(e);
  }
}

void main();
