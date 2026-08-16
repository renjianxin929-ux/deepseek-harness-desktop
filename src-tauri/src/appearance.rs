//! DeepSeek Harness Desktop — appearance system (surface layer only).
//!
//! This module owns the DIY appearance layer that sits *on top of* the
//! official DeepSeek Harness Web UI without ever modifying Harness Core:
//!
//! * declarative themes (tokens + constrained CSS strings + assets) — no
//!   arbitrary JavaScript, no access to credentials/sessions/shell/filesystem;
//! * user-selected local wallpaper (copied into the app config dir, never the
//!   repository, never uploaded);
//! * optional lightweight motion with `prefers-reduced-motion` handling;
//! * a compatibility gate that degrades or falls back safely when the upstream
//!   Harness UI can no longer be targeted confidently.
//!
//! The injected [`ENGINE`] (see `engine.js`) is the only code that runs inside
//! the Harness page. It is self-contained, applies the appearance via a single
//! `<style>` element plus a few `pointer-events:none` background layers, and
//! never touches the React tree inside `#root`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager, Runtime};

use crate::css_scope::{reject_url_tokens, validate_theme_css, validate_token_value};

/// Injected engine (self-contained JS; see `engine.js`).
const ENGINE: &str = include_str!("engine.js");

/// Built-in themes shipped with the app.
const OCEAN_THEME: &str = include_str!("../../themes/ocean/theme.json");
const STARTER_THEME: &str = include_str!("../../themes/starter/theme.json");

pub const OFFICIAL_THEME_ID: &str = "official";
pub const CONFIG_FILE: &str = "appearance.json";
pub const WALLPAPER_DIR: &str = "wallpaper";
pub const THEMES_DIR: &str = "themes";

/// Max size (bytes) for a decorative asset file inlined from a custom theme.
const MAX_ASSET_BYTES: u64 = 512 * 1024;
/// Max accepted wallpaper size (bytes) — keeps local copies bounded.
const MAX_WALLPAPER_BYTES: u64 = 24 * 1024 * 1024;

const ALLOWED_WALLPAPER_EXT: &[&str] = &["png", "jpg", "jpeg", "webp"];
/// Local decorative asset types allowed inside a theme's `assets/` directory.
const ALLOWED_ASSET_EXT: &[&str] = &["svg", "png", "jpg", "jpeg", "webp"];
/// Image MIME types allowed for `data:` URIs in the `asset` field.
const ALLOWED_DATA_IMAGE_MIME: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/webp",
    "image/svg+xml",
];
/// CSS gradient functions allowed for the `asset` field (V0.1 needs only the
/// plain/repeating linear/radial/conic set; no other image functions).
const ALLOWED_GRADIENT_FUNCTIONS: &[&str] = &[
    "linear-gradient",
    "radial-gradient",
    "conic-gradient",
    "repeating-linear-gradient",
    "repeating-radial-gradient",
    "repeating-conic-gradient",
];

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TokenSet {
    pub light: BTreeMap<String, String>,
    pub dark: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tokens: TokenSet,
    pub surfaces: Option<TokenSet>,
    pub components: Option<String>,
    pub motion: Option<String>,
    /// CSS `background` value (gradient, `url(data:...)`, or `assets/<file>` for
    /// custom themes). Used as the ambient decorative layer.
    pub asset: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperSettings {
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default = "default_fit")]
    pub fit: String,
    #[serde(default = "default_position")]
    pub position: String,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub blur: f64,
    #[serde(default = "default_overlay")]
    pub overlay: f64,
    #[serde(default = "default_overlay_mode")]
    pub overlay_mode: String,
    /// Incremented on every wallpaper change so the WebView re-fetches.
    #[serde(default)]
    pub version: u64,
}

impl Default for WallpaperSettings {
    fn default() -> Self {
        Self {
            active: false,
            file_name: None,
            fit: default_fit(),
            position: default_position(),
            opacity: default_opacity(),
            blur: 0.0,
            overlay: default_overlay(),
            overlay_mode: default_overlay_mode(),
            version: 0,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_fit() -> String {
    "cover".into()
}
fn default_position() -> String {
    "center".into()
}
fn default_opacity() -> f64 {
    0.6
}
fn default_overlay() -> f64 {
    0.45
}
fn default_overlay_mode() -> String {
    "auto".into()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppearanceConfig {
    pub version: u32,
    pub theme: String,
    pub motion_enabled: bool,
    pub wallpaper: WallpaperSettings,
    /// Appearance UI language: "system" | "zh-CN" | "en".
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            version: 1,
            theme: OFFICIAL_THEME_ID.into(),
            motion_enabled: true,
            wallpaper: WallpaperSettings::default(),
            language: default_language(),
        }
    }
}

fn default_language() -> String {
    "system".into()
}

/// Summary of an available theme (sent to the settings window).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String, // "builtin" | "custom"
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSnapshot {
    pub active_theme: String,
    pub motion_enabled: bool,
    pub wallpaper: WallpaperSettings,
    pub themes: Vec<ThemeMeta>,
    pub compat_mode: String,
    pub language: String,
}

/// Runtime state shared by commands, the wallpaper protocol and the navigator.
pub struct AppearanceState {
    pub config: Mutex<AppearanceConfig>,
    pub config_dir: PathBuf,
    pub themes_dir: PathBuf,
    pub wallpaper_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Init / persistence
// ---------------------------------------------------------------------------

impl AppearanceState {
    /// Initialize appearance state. Never fails: if the config directory cannot
    /// be created, a fallback location is used and writes will simply fail
    /// gracefully later — an appearance problem must never prevent Harness from
    /// reaching a usable state.
    pub fn init<R: Runtime>(app: &AppHandle<R>) -> Self {
        let config_dir = resolve_config_dir(app);
        let themes_dir = config_dir.join(THEMES_DIR);
        let wallpaper_dir = config_dir.join(WALLPAPER_DIR);
        for d in [&config_dir, &themes_dir, &wallpaper_dir] {
            let _ = fs::create_dir_all(d);
        }

        let config = load_config(&config_dir.join(CONFIG_FILE));
        let state = Self {
            config: Mutex::new(config),
            config_dir,
            themes_dir,
            wallpaper_dir,
        };
        // If the persisted theme no longer resolves (e.g. a custom theme was
        // deleted), fall back to Official rather than leaving a broken choice.
        validate_active_theme(&state);
        state
    }
}

/// Resolve the appearance config directory. `HD_CONFIG_DIR` is a dev/test
/// override; by default the platform app-config directory is used.
fn resolve_config_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    if let Ok(d) = std::env::var("HD_CONFIG_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("harness-desktop"))
}

fn validate_active_theme(state: &AppearanceState) {
    let mut cfg = state.config.lock().unwrap_or_else(|p| p.into_inner());
    if cfg.theme != OFFICIAL_THEME_ID && resolve_theme(state, &cfg.theme).is_err() {
        cfg.theme = OFFICIAL_THEME_ID.into();
    }
}

fn load_config(path: &Path) -> AppearanceConfig {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return AppearanceConfig::default(),
    };
    match serde_json::from_str::<AppearanceConfig>(&text) {
        Ok(mut cfg) => {
            sanitize(&mut cfg);
            cfg
        }
        Err(_) => AppearanceConfig::default(),
    }
}

/// Clamp and normalize a config so a corrupt/manual file can never put the
/// appearance system into an invalid state.
fn sanitize(cfg: &mut AppearanceConfig) {
    let valid_fits = ["cover", "contain", "fill", "auto"];
    let valid_positions = [
        "center",
        "top",
        "bottom",
        "left",
        "right",
        "top-left",
        "top-right",
        "bottom-left",
        "bottom-right",
    ];
    let valid_overlay_modes = ["auto", "dark", "light"];
    if !valid_fits.contains(&cfg.wallpaper.fit.as_str()) {
        cfg.wallpaper.fit = default_fit();
    }
    if !valid_positions.contains(&cfg.wallpaper.position.as_str()) {
        cfg.wallpaper.position = default_position();
    }
    if !valid_overlay_modes.contains(&cfg.wallpaper.overlay_mode.as_str()) {
        cfg.wallpaper.overlay_mode = default_overlay_mode();
    }
    cfg.wallpaper.opacity = cfg.wallpaper.opacity.clamp(0.0, 1.0);
    cfg.wallpaper.blur = cfg.wallpaper.blur.clamp(0.0, 80.0);
    cfg.wallpaper.overlay = cfg.wallpaper.overlay.clamp(0.0, 1.0);
    if !is_safe_theme_id(&cfg.theme) {
        // Reject ids that could escape the themes directory.
        cfg.theme = OFFICIAL_THEME_ID.into();
    }
    cfg.language = normalize_language(&cfg.language);
}

/// Normalize a language preference to one of the supported values, falling back
/// to "system".
fn normalize_language(language: &str) -> String {
    match language {
        "zh-CN" | "en" => language.to_string(),
        _ => "system".to_string(),
    }
}

/// A theme id must be a safe single path component (no traversal).
fn is_safe_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains('\0')
}

fn save_config(state: &AppearanceState) -> Result<(), String> {
    let path = state.config_dir.join(CONFIG_FILE);
    let cfg = state
        .config
        .lock()
        .map_err(|_| "appearance lock poisoned")?
        .clone();
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("serialize config: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("write config: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("commit config: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

fn official_theme() -> Theme {
    Theme {
        id: OFFICIAL_THEME_ID.into(),
        name: "Official".into(),
        description: "The untouched DeepSeek Harness appearance.".into(),
        ..Default::default()
    }
}

fn parse_theme(id: &str, json: &str) -> Result<Theme, String> {
    let mut theme: Theme =
        serde_json::from_str(json).map_err(|e| format!("parse theme {id}: {e}"))?;
    if theme.id.is_empty() {
        theme.id = id.to_string();
    }
    if theme.name.is_empty() {
        theme.name = id.to_string();
    }
    Ok(theme)
}

pub fn resolve_theme(state: &AppearanceState, id: &str) -> Result<Theme, String> {
    if id == OFFICIAL_THEME_ID {
        return Ok(official_theme());
    }
    if id == "ocean" {
        let theme = parse_theme(id, OCEAN_THEME)?;
        validate_theme_security(&theme)?;
        return Ok(theme);
    }
    if id == "starter" {
        let theme = parse_theme(id, STARTER_THEME)?;
        validate_theme_security(&theme)?;
        return Ok(theme);
    }
    // Custom theme: <app-config>/themes/<id>/theme.json. The id must be a safe
    // single path component so it cannot escape the themes directory, and the
    // resolved theme directory must really live inside the themes root (no
    // symlink escape).
    if !is_safe_theme_id(id) {
        return Err(format!("invalid theme id '{id}'"));
    }
    let themes_root_canon = fs::canonicalize(&state.themes_dir)
        .map_err(|e| format!("cannot resolve themes directory: {e}"))?;
    let dir = state.themes_dir.join(id);
    let theme_dir_canon = fs::canonicalize(&dir)
        .map_err(|_| format!("custom theme '{id}' not found at {}", dir.display()))?;
    if !theme_dir_canon.starts_with(&themes_root_canon) {
        return Err(format!("theme '{id}' escapes the themes directory"));
    }
    let path = theme_dir_canon.join("theme.json");
    let text = fs::read_to_string(&path)
        .map_err(|_| format!("custom theme '{id}' not found at {}", path.display()))?;
    let mut theme = parse_theme(id, &text)?;
    theme.id = id.to_string();
    validate_theme_security(&theme)?;
    if let Some(asset) = theme.asset.as_deref() {
        if let Some(inlined) = process_asset(&theme_dir_canon, asset)? {
            theme.asset = Some(inlined);
        }
    }
    Ok(theme)
}

/// Validate the scoped CSS fields. Fails closed: a theme whose `components` or
/// `motion` cannot be proven safe is rejected before it can reach the engine.
fn validate_theme_css_fields(theme: &Theme) -> Result<(), String> {
    if let Some(css) = theme.components.as_deref() {
        validate_theme_css(css, "components")?;
    }
    if let Some(css) = theme.motion.as_deref() {
        validate_theme_css(css, "motion")?;
    }
    Ok(())
}

/// Validate every structural field of a theme (components/motion CSS + token and
/// surface keys). Any violation rejects the whole theme — never partial/silent
/// correction.
fn validate_theme_security(theme: &Theme) -> Result<(), String> {
    validate_theme_css_fields(theme)?;
    validate_token_maps(theme)?;
    Ok(())
}

/// Every token/surface key must be provably ONE valid CSS custom property name.
/// The engine interpolates these keys into `body{<key>:<value>}`; a key that can
/// smuggle a grammar delimiter (`;`, `:`, `{`, `}`, whitespace, comment, escape,
/// newline, …) would let a theme escape the `body` declaration and emit global
/// CSS. Only a narrow ASCII `--`-prefixed dashed-ident is accepted, and no
/// escape is ever allowed (an escape can decode to a delimiter).
fn validate_token_key(key: &str) -> Result<(), String> {
    let bytes = key.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'-' || bytes[1] != b'-' {
        return Err(format!(
            "token key '{key}' must be a custom property name (start with '--')"
        ));
    }
    // First char after "--" must be a name-start (ASCII letter or underscore),
    // matching the CSS `<ident-token>` rule (a digit or hyphen here would split
    // the token into `--` + something else).
    if !(bytes[2].is_ascii_alphabetic() || bytes[2] == b'_') {
        return Err(format!(
            "token key '{key}' is not a valid custom property name"
        ));
    }
    // Remaining chars must be ASCII name chars (letter, digit, `_`, `-`).
    if !bytes[3..]
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!(
            "token key '{key}' is not a valid custom property name"
        ));
    }
    Ok(())
}

fn validate_token_maps(theme: &Theme) -> Result<(), String> {
    for (key, value) in theme.tokens.light.iter().chain(theme.tokens.dark.iter()) {
        validate_token_key(key)?;
        validate_token_value(value)?;
    }
    if let Some(surfaces) = &theme.surfaces {
        for (key, value) in surfaces.light.iter().chain(surfaces.dark.iter()) {
            validate_token_key(key)?;
            validate_token_value(value)?;
        }
    }
    Ok(())
}

/// Process the `asset` field of a custom theme. Returns `Some(value)` when the
/// asset must be inlined/re-serialized (local file or canonical data URI), or
/// `None` when the value is kept as-is (a proven-safe CSS gradient).
///
/// Only three forms are accepted: `assets/<file>` (canonical filesystem
/// containment), `data:image/*;base64,` (re-serialized from decoded bytes), or a
/// single CSS gradient function. Everything else is rejected fail-closed.
fn process_asset(theme_dir: &Path, asset: &str) -> Result<Option<String>, String> {
    let trimmed = asset.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.starts_with("assets/") && !trimmed.contains("://") {
        return inline_local_asset(theme_dir, trimmed);
    }
    if trimmed
        .get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("data:"))
    {
        return canonicalize_data_image(trimmed).map(Some);
    }
    validate_gradient(trimmed)?;
    Ok(None)
}

/// Validate a gradient `asset` value is EXACTLY one image function: allowed
/// function name, balanced parentheses, no `;`/`{`/`}`, no URL-bearing content,
/// no trailing tokens, no comments/escapes/strings/brackets, and a narrow
/// character set. The value is kept verbatim only after this proof.
fn validate_gradient(value: &str) -> Result<(), String> {
    if value.contains(';') || value.contains('{') || value.contains('}') {
        return Err("gradient must be a single value (no ';', '{', or '}')".into());
    }
    // Narrow character set: letters/digits + the handful of symbols a gradient
    // legitimately uses. This rejects quotes, comments, escapes, brackets, `@`,
    // `:`, `/`, `*`, newlines, and any other structure-altering character.
    if !value.bytes().all(|b| {
        b.is_ascii_alphanumeric()
            || matches!(b, b'#' | b'%' | b'(' | b')' | b',' | b'.' | b' ' | b'-')
    }) {
        return Err("gradient contains unsupported characters".into());
    }
    // Syntax-aware rejection of url() / unquoted URLs (defense in depth).
    reject_url_tokens_in(value)?;

    // Exactly one function whose name is allow-listed.
    let open = value
        .find('(')
        .ok_or_else(|| "gradient must be a function".to_string())?;
    if open == 0 {
        return Err("gradient must be a function".into());
    }
    let name = &value[..open];
    let name_lower = name.to_ascii_lowercase();
    if !ALLOWED_GRADIENT_FUNCTIONS.contains(&name_lower.as_str()) {
        return Err(format!("unsupported gradient function '{name}'"));
    }

    // Parentheses must be balanced and the single top-level function must close
    // as the final character (no trailing tokens of any kind).
    let mut depth: i32 = 0;
    let mut closed_at: Option<usize> = None;
    for (i, &b) in value.as_bytes().iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth < 0 {
                    return Err("unbalanced ')' in gradient".into());
                }
                if depth == 0 && closed_at.is_none() {
                    closed_at = Some(i);
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("unclosed gradient function".into());
    }
    match closed_at {
        Some(pos) if pos == value.len() - 1 => Ok(()),
        _ => Err("gradient must be exactly one value (no trailing tokens)".into()),
    }
}

/// Resolve and inline a local `assets/<file>` reference with strict canonical
/// containment inside the theme's own `assets/` directory.
fn inline_local_asset(theme_dir: &Path, asset: &str) -> Result<Option<String>, String> {
    let candidate = resolve_contained_asset(theme_dir, asset)?;

    let ext = candidate
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| "asset has no extension".to_string())?;
    if !ALLOWED_ASSET_EXT.contains(&ext.as_str()) {
        return Err(format!(
            "unsupported asset type '.{ext}' — supported: SVG, PNG, JPG/JPEG, WebP"
        ));
    }

    let meta = fs::metadata(&candidate).map_err(|e| format!("stat asset: {e}"))?;
    if meta.len() > MAX_ASSET_BYTES {
        return Err(format!("asset too large (> {} bytes)", MAX_ASSET_BYTES));
    }
    let bytes = fs::read(&candidate).map_err(|e| format!("read asset: {e}"))?;

    // Magic-byte validation for raster types; SVG is left to its existing
    // (textual) handling and never gains remote-loading capability.
    let magic_ext = match ext.as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "webp" => Some("webp"),
        _ => None,
    };
    if let Some(magic_ext) = magic_ext {
        let mut head = [0u8; 16];
        let n = bytes.len().min(16);
        head[..n].copy_from_slice(&bytes[..n]);
        if !looks_like_image(&head, magic_ext) {
            return Err("asset content does not match its image type".into());
        }
    }

    let mime = mime_for_path(&candidate);
    Ok(Some(format!(
        "url(data:{mime};base64,{})",
        base64_encode(&bytes)
    )))
}

/// Canonical-containment check for a local theme asset.
///
/// The logical path must be a clean relative path like `assets/foo.svg` or
/// `assets/background/image.png`. The resolved file's canonical path must lie
/// strictly inside the theme's canonical `assets/` directory, so neither
/// `../` traversal nor symlinks can escape the theme.
fn resolve_contained_asset(theme_dir: &Path, asset: &str) -> Result<PathBuf, String> {
    let logical = validate_logical_asset_path(asset)?;

    let theme_dir_canon =
        fs::canonicalize(theme_dir).map_err(|e| format!("cannot resolve theme dir: {e}"))?;
    let asset_root = theme_dir_canon.join("assets");
    let asset_root_canon = fs::canonicalize(&asset_root)
        .map_err(|_| "theme assets directory is missing".to_string())?;
    // The assets root itself must not be a symlink escaping the theme.
    if !asset_root_canon.starts_with(&theme_dir_canon) {
        return Err("theme assets directory escapes the theme directory".to_string());
    }

    let candidate = asset_root_canon.join(&logical);
    if !candidate.is_file() {
        return Err(format!(
            "asset reference not found: {}",
            candidate.display()
        ));
    }
    let candidate_canon =
        fs::canonicalize(&candidate).map_err(|e| format!("cannot resolve asset: {e}"))?;
    if !candidate_canon.starts_with(&asset_root_canon) {
        return Err("asset escapes its theme assets directory".to_string());
    }
    Ok(candidate_canon)
}

/// Validate a logical asset path and return its relative form (the part after
/// the `assets/` prefix). Rejects absolute paths, `.`, `..`, backslashes, empty
/// paths, and protocol-relative / scheme-prefixed values.
fn validate_logical_asset_path(asset: &str) -> Result<PathBuf, String> {
    let rel = asset
        .strip_prefix("assets/")
        .ok_or_else(|| "asset must start with 'assets/'".to_string())?;
    if rel.is_empty() {
        return Err("asset path is empty".into());
    }
    if rel.contains('\\') {
        return Err("asset paths must use forward slashes".into());
    }
    if rel.contains(':') {
        return Err("asset path must not contain a scheme".into());
    }
    let mut out = PathBuf::new();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err("asset path must not contain empty, '.', or '..' segments".into());
        }
        out.push(part);
    }
    Ok(out)
}

/// Fully parse and canonicalize a `data:` image URI. The raw user string is
/// never returned as-is: the MIME is validated, the base64 payload is decoded
/// strictly (no trailing/ignored characters, no `;`/`}`/padding suffix), the
/// bytes are size-capped and magic-byte checked, and a canonical data URI is
/// re-serialized from the MIME + decoded bytes.
fn canonicalize_data_image(data: &str) -> Result<String, String> {
    let rest = data
        .get(5..)
        .ok_or_else(|| "invalid data URI".to_string())?;
    let (meta, payload) = rest
        .split_once(',')
        .ok_or_else(|| "data URI is missing a comma".to_string())?;
    let meta_lower = meta.to_ascii_lowercase();
    if !meta_lower.ends_with(";base64") {
        return Err("only base64 data URIs are allowed".into());
    }
    let mime = &meta[..meta.len() - ";base64".len()];
    let mime_lower = mime.to_ascii_lowercase();
    if !ALLOWED_DATA_IMAGE_MIME.contains(&mime_lower.as_str()) {
        return Err(format!("unsupported data URI MIME '{mime}'"));
    }
    // Strict full-payload decode: rejects any trailing content (including `;`,
    // `}`, `=`, whitespace, or extra data after valid padding).
    let decoded = decode_base64_strict(payload)?;
    if decoded.len() as u64 > MAX_ASSET_BYTES {
        return Err(format!(
            "data URI image too large (> {} bytes)",
            MAX_ASSET_BYTES
        ));
    }
    let magic_ext = match mime_lower.as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/webp" => Some("webp"),
        _ => None,
    };
    if let Some(ext) = magic_ext {
        let mut head = [0u8; 16];
        let n = decoded.len().min(16);
        head[..n].copy_from_slice(&decoded[..n]);
        if !looks_like_image(&head, ext) {
            return Err("data URI content does not match its image type".into());
        }
    }
    // Re-serialize from MIME + decoded bytes (lowercased canonical MIME).
    Ok(format!(
        "data:{};base64,{}",
        mime_lower,
        base64_encode(&decoded)
    ))
}

/// Strict base64 decoder for data URIs: consumes the ENTIRE payload, rejects
/// whitespace, non-alphabet characters, `=` padding not at the end, trailing
/// data after padding, and invalid lengths. Produces the decoded bytes.
fn decode_base64_strict(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err("empty base64 payload".into());
    }
    // Split off trailing padding.
    let mut pad = 0usize;
    let mut data_end = bytes.len();
    for &b in bytes.iter().rev() {
        if b == b'=' {
            pad += 1;
            data_end -= 1;
        } else {
            break;
        }
    }
    if pad > 2 {
        return Err("invalid base64 padding".into());
    }
    let data = &bytes[..data_end];
    if data.is_empty() {
        return Err("empty base64 payload".into());
    }
    // A base64 group cannot have exactly one leftover sextet.
    if data.len() % 4 == 1 {
        return Err("invalid base64 length".into());
    }
    if pad > 0 && (data.len() + pad) % 4 != 0 {
        return Err("invalid base64 padding".into());
    }
    let mut out = Vec::with_capacity(data.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &b in data {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("invalid base64 character".into()),
        } as u32;
        buffer = (buffer << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    if out.is_empty() {
        return Err("empty base64 payload".into());
    }
    Ok(out)
}

/// Reject any URL-bearing token in a CSS value fragment (syntax-aware).
fn reject_url_tokens_in(value: &str) -> Result<(), String> {
    let mut input = cssparser::ParserInput::new(value);
    let mut parser = cssparser::Parser::new(&mut input);
    reject_url_tokens(&mut parser)
}

pub fn list_themes(state: &AppearanceState) -> Vec<ThemeMeta> {
    let mut themes = vec![
        ThemeMeta {
            id: OFFICIAL_THEME_ID.into(),
            name: "Official".into(),
            description: "The untouched DeepSeek Harness appearance.".into(),
            kind: "builtin".into(),
        },
        ThemeMeta {
            id: "ocean".into(),
            name: "Ocean".into(),
            description: "Restrained ocean-inspired showcase.".into(),
            kind: "builtin".into(),
        },
        ThemeMeta {
            id: "starter".into(),
            name: "Starter (Sunset)".into(),
            description: "A documented template for your own theme.".into(),
            kind: "builtin".into(),
        },
    ];
    if let Ok(entries) = fs::read_dir(&state.themes_dir) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if id == OFFICIAL_THEME_ID || id == "ocean" || id == "starter" {
                continue;
            }
            if !dir.join("theme.json").is_file() {
                continue;
            }
            let name = fs::read_to_string(dir.join("theme.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<Theme>(&t).ok())
                .map(|t| {
                    if t.name.is_empty() {
                        id.clone()
                    } else {
                        t.name
                    }
                })
                .unwrap_or_else(|| id.clone());
            themes.push(ThemeMeta {
                id: id.clone(),
                name,
                description: String::new(),
                kind: "custom".into(),
            });
        }
    }
    themes
}

// ---------------------------------------------------------------------------
// Base64 (small self-contained codec; avoids an extra crate dependency)
// ---------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for c in input.chars() {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            '=' => break,
            '\n' | '\r' | ' ' | '\t' => continue,
            _ => return Err("invalid base64 input".into()),
        };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Wallpaper
// ---------------------------------------------------------------------------

pub fn wallpaper_file_path(state: &AppearanceState) -> Option<PathBuf> {
    let cfg = state.config.lock().ok()?;
    let name = cfg.wallpaper.file_name.as_ref()?;
    Some(state.wallpaper_dir.join(name))
}

fn mime_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn validate_wallpaper(path: &Path) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| "file has no extension".to_string())?;
    if !ALLOWED_WALLPAPER_EXT.contains(&ext.as_str()) {
        return Err(format!(
            "unsupported image type '.{ext}' — supported: PNG, JPG/JPEG, WebP"
        ));
    }
    let meta = fs::metadata(path).map_err(|e| format!("cannot read file: {e}"))?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    if meta.len() == 0 {
        return Err("file is empty".into());
    }
    if meta.len() > MAX_WALLPAPER_BYTES {
        return Err(format!(
            "image too large (max {} MB)",
            MAX_WALLPAPER_BYTES / 1024 / 1024
        ));
    }
    // Light magic-byte check so an unsupported/renamed file fails safely.
    let mut head = [0u8; 16];
    if let Ok(bytes) = fs::read(path) {
        let n = bytes.len().min(16);
        head[..n].copy_from_slice(&bytes[..n]);
        if !looks_like_image(&head, &ext) {
            return Err("file content does not match its image type".into());
        }
    }
    Ok(())
}

fn looks_like_image(head: &[u8; 16], ext: &str) -> bool {
    match ext {
        "png" => head.starts_with(&[0x89, b'P', b'N', b'G']),
        "jpg" | "jpeg" => head.starts_with(&[0xFF, 0xD8, 0xFF]),
        "webp" => head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP",
        _ => true,
    }
}

/// Derive the stored file name from a user-provided file name (extension
/// determines the stored image type; falls back to `.png`).
fn stored_name_for(user_name: &str) -> String {
    let ext = Path::new(user_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| ALLOWED_WALLPAPER_EXT.contains(&e.as_str()))
        .unwrap_or_else(|| "png".to_string());
    format!("wallpaper.{ext}")
}

/// Store validated image bytes into the local wallpaper directory (never the
/// repository) and apply.
pub fn store_wallpaper_bytes<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppearanceState,
    user_name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("image is empty".into());
    }
    if bytes.len() as u64 > MAX_WALLPAPER_BYTES {
        return Err(format!(
            "image too large (max {} MB)",
            MAX_WALLPAPER_BYTES / 1024 / 1024
        ));
    }
    let ext = Path::new(user_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| "file has no extension".to_string())?;
    if !ALLOWED_WALLPAPER_EXT.contains(&ext.as_str()) {
        return Err(format!(
            "unsupported image type '.{ext}' — supported: PNG, JPG/JPEG, WebP"
        ));
    }
    let mut head = [0u8; 16];
    let n = bytes.len().min(16);
    head[..n].copy_from_slice(&bytes[..n]);
    if !looks_like_image(&head, &ext) {
        return Err("file content does not match its image type".into());
    }

    let name = stored_name_for(user_name);
    let dest = state.wallpaper_dir.join(&name);
    fs::write(&dest, bytes).map_err(|e| format!("write wallpaper: {e}"))?;

    {
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "appearance lock poisoned")?;
        cfg.wallpaper.active = true;
        cfg.wallpaper.file_name = Some(name);
        cfg.wallpaper.version = cfg.wallpaper.version.wrapping_add(1);
    }
    save_config(state)?;
    apply_to_main_window(app, state);
    Ok(())
}

/// Copy a user-selected image file into the local wallpaper directory. Used by
/// tests and by the `HD_FORCE_WALLPAPER` startup hook; the interactive path
/// goes through `set_wallpaper_bytes`.
pub fn install_wallpaper_from_path<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppearanceState,
    source: &Path,
) -> Result<(), String> {
    validate_wallpaper(source)?;
    let bytes = fs::read(source).map_err(|e| format!("read wallpaper: {e}"))?;
    let name = source
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "wallpaper.png".into());
    store_wallpaper_bytes(app, state, &name, &bytes)
}

pub fn remove_wallpaper_file(state: &AppearanceState) {
    if let Some(p) = wallpaper_file_path(state) {
        let _ = fs::remove_file(p);
    }
}

// ---------------------------------------------------------------------------
// Payload + injection
// ---------------------------------------------------------------------------

fn compat_mode() -> String {
    std::env::var("HD_COMPAT_MODE").unwrap_or_else(|_| "normal".into())
}

/// The payload handed to the injected engine (initial + live updates).
pub fn engine_payload(state: &AppearanceState) -> Result<serde_json::Value, String> {
    let cfg = state
        .config
        .lock()
        .map_err(|_| "appearance lock poisoned")?
        .clone();
    let theme = resolve_theme(state, &cfg.theme)?;
    let wallpaper_url = if cfg.wallpaper.active && cfg.wallpaper.file_name.is_some() {
        crate::platform::custom_scheme_url(
            "hd-wallpaper",
            &format!("current?v={}", cfg.wallpaper.version),
        )
    } else {
        String::new()
    };
    Ok(json!({
        "compatMode": compat_mode(),
        "motionEnabled": cfg.motion_enabled,
        "theme": {
            "id": theme.id,
            "tokens": theme.tokens,
            "surfaces": theme.surfaces,
            "components": theme.components,
            "motion": theme.motion,
            "asset": theme.asset,
        },
        "wallpaper": {
            "active": cfg.wallpaper.active && !wallpaper_url.is_empty(),
            "url": wallpaper_url,
            "fit": cfg.wallpaper.fit,
            "position": cfg.wallpaper.position,
            "opacity": cfg.wallpaper.opacity,
            "blur": cfg.wallpaper.blur,
            "overlay": cfg.wallpaper.overlay,
            "overlayMode": cfg.wallpaper.overlay_mode,
        },
    }))
}

/// The initialization script injected into the main window. It runs on every
/// page load (loading page and Harness page) and only acts on the Harness page.
pub fn build_init_script(state: &AppearanceState) -> String {
    let payload = engine_payload(state)
        .unwrap_or_else(|_| json!({"compatMode": "normal", "motionEnabled": true, "theme": {}, "wallpaper": {"active": false}}));
    let mut script = String::with_capacity(ENGINE.len() + 128);
    script.push_str("window.__HD_INITIAL__ = ");
    script.push_str(&payload.to_string());
    script.push_str(";\n");
    script.push_str(ENGINE);
    script
}

/// Apply the current appearance live (without reload) to the main window.
pub fn apply_to_main_window<R: Runtime>(app: &AppHandle<R>, state: &AppearanceState) {
    let payload = match engine_payload(state) {
        Ok(p) => p,
        Err(_) => return,
    };
    let js = format!(
        "try{{ if (window.__HD_APPLY__) window.__HD_APPLY__({}); }}catch(e){{}}",
        payload
    );
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.eval(&js);
    }
}

// ---------------------------------------------------------------------------
// Wallpaper custom protocol
// ---------------------------------------------------------------------------

pub fn serve_wallpaper(
    state: &AppearanceState,
    _webview_label: &str,
    _request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::{header::CONTENT_TYPE, Response, StatusCode};
    let not_found = |code: StatusCode, msg: &str| {
        Response::builder()
            .status(code)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(msg.as_bytes().to_vec())
            .unwrap()
    };
    let path = match wallpaper_file_path(state) {
        Some(p) if p.is_file() => p,
        _ => return not_found(StatusCode::NOT_FOUND, "no wallpaper"),
    };
    match fs::read(&path) {
        Ok(bytes) => {
            let mime = mime_for_path(&path);
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, mime)
                .header("Cache-Control", "private, max-age=0")
                .body(bytes)
                .unwrap()
        }
        Err(_) => not_found(StatusCode::NOT_FOUND, "wallpaper unreadable"),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands (invoked from the settings window, app's own origin)
// ---------------------------------------------------------------------------

fn snapshot<R: Runtime>(
    _app: &AppHandle<R>,
    state: &AppearanceState,
) -> Result<AppearanceSnapshot, String> {
    let cfg = state
        .config
        .lock()
        .map_err(|_| "appearance lock poisoned")?
        .clone();
    Ok(AppearanceSnapshot {
        active_theme: cfg.theme.clone(),
        motion_enabled: cfg.motion_enabled,
        wallpaper: cfg.wallpaper.clone(),
        themes: list_themes(state),
        compat_mode: compat_mode(),
        language: cfg.language.clone(),
    })
}

#[tauri::command]
pub fn get_appearance<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppearanceState>,
) -> Result<AppearanceSnapshot, String> {
    snapshot(&app, &state)
}

#[tauri::command]
pub fn set_theme<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppearanceState>,
    theme_id: String,
) -> Result<AppearanceSnapshot, String> {
    // Validate the theme resolves before persisting.
    resolve_theme(&state, &theme_id)?;
    {
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "appearance lock poisoned")?;
        cfg.theme = theme_id;
    }
    save_config(&state)?;
    apply_to_main_window(&app, &state);
    snapshot(&app, &state)
}

#[tauri::command]
pub fn set_motion<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppearanceState>,
    enabled: bool,
) -> Result<AppearanceSnapshot, String> {
    {
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "appearance lock poisoned")?;
        cfg.motion_enabled = enabled;
    }
    save_config(&state)?;
    apply_to_main_window(&app, &state);
    snapshot(&app, &state)
}

#[tauri::command]
pub fn set_language<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppearanceState>,
    language: String,
) -> Result<AppearanceSnapshot, String> {
    {
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "appearance lock poisoned")?;
        cfg.language = normalize_language(&language);
    }
    save_config(&state)?;
    snapshot(&app, &state)
}

#[tauri::command]
pub fn set_wallpaper<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppearanceState>,
    settings: WallpaperSettings,
) -> Result<AppearanceSnapshot, String> {
    {
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "appearance lock poisoned")?;
        let mut s = settings;
        // Preserve the installed file + version; these are managed here.
        s.file_name = cfg.wallpaper.file_name.clone();
        s.version = cfg.wallpaper.version;
        cfg.wallpaper = s;
        sanitize(&mut cfg);
    }
    save_config(&state)?;
    apply_to_main_window(&app, &state);
    snapshot(&app, &state)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperBytes {
    pub name: String,
    /// Base64-encoded image bytes (optionally a full `data:` URI).
    pub data: String,
}

/// Install a wallpaper from bytes read by the settings window's
/// `<input type="file">`. The image never leaves the app: it is validated,
/// written to the local wallpaper directory, and never uploaded.
#[tauri::command]
pub fn set_wallpaper_bytes<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppearanceState>,
    payload: WallpaperBytes,
) -> Result<AppearanceSnapshot, String> {
    let b64 = match payload.data.split_once(";base64,") {
        Some((_, b)) => b.to_string(),
        None => payload.data.clone(),
    };
    let bytes = base64_decode(&b64)?;
    store_wallpaper_bytes(&app, &state, &payload.name, &bytes)?;
    snapshot(&app, &state)
}

#[tauri::command]
pub fn remove_wallpaper<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppearanceState>,
) -> Result<AppearanceSnapshot, String> {
    remove_wallpaper_file(&state);
    {
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "appearance lock poisoned")?;
        cfg.wallpaper.active = false;
        cfg.wallpaper.file_name = None;
        cfg.wallpaper.version = cfg.wallpaper.version.wrapping_add(1);
    }
    save_config(&state)?;
    apply_to_main_window(&app, &state);
    snapshot(&app, &state)
}

#[tauri::command]
pub fn reset_appearance<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppearanceState>,
) -> Result<AppearanceSnapshot, String> {
    remove_wallpaper_file(&state);
    {
        let mut cfg = state
            .config
            .lock()
            .map_err(|_| "appearance lock poisoned")?;
        *cfg = AppearanceConfig::default();
    }
    save_config(&state)?;
    apply_to_main_window(&app, &state);
    snapshot(&app, &state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppearanceState {
        let dir = std::env::temp_dir().join(format!("hd-test-{}", std::process::id()));
        let themes_dir = dir.join(THEMES_DIR);
        let wallpaper_dir = dir.join(WALLPAPER_DIR);
        fs::create_dir_all(&themes_dir).unwrap();
        fs::create_dir_all(&wallpaper_dir).unwrap();
        AppearanceState {
            config: Mutex::new(AppearanceConfig::default()),
            config_dir: dir,
            themes_dir,
            wallpaper_dir,
        }
    }

    fn write_temp(bytes: &[u8], name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(name);
        fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn base64_round_trip() {
        for input in [
            b"".as_slice(),
            b"f".as_slice(),
            b"fo".as_slice(),
            b"foo".as_slice(),
            b"foobar".as_slice(),
            b"\x89PNG\r\n\x1a\n hello world".as_slice(),
        ] {
            let enc = base64_encode(input);
            let dec = base64_decode(&enc).unwrap();
            assert_eq!(dec, input, "round-trip failed for {:?}", input);
        }
    }

    #[test]
    fn base64_decode_rejects_garbage() {
        assert!(base64_decode("not base64!!!").is_err());
    }

    #[test]
    fn image_magic_detection() {
        let mut png = [0u8; 16];
        png[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert!(looks_like_image(&png, "png"));

        let mut jpg = [0u8; 16];
        jpg[..3].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
        assert!(looks_like_image(&jpg, "jpg"));

        let mut webp = [0u8; 16];
        webp[..4].copy_from_slice(b"RIFF");
        webp[8..12].copy_from_slice(b"WEBP");
        assert!(looks_like_image(&webp, "webp"));

        let mut not_png = [0u8; 16];
        not_png[..4].copy_from_slice(b"GIF8");
        assert!(!looks_like_image(&not_png, "png"));
    }

    #[test]
    fn wallpaper_validation_rejects_invalid() {
        // A GIF renamed to .png must be rejected by the magic-byte check.
        let mut gif = [0u8; 20];
        gif[..4].copy_from_slice(b"GIF8");
        let p = write_temp(&gif, "fake.png");
        assert!(validate_wallpaper(&p).is_err());

        // Unsupported extension.
        let p2 = write_temp(&[0xFF, 0xD8, 0xFF], "photo.gif");
        assert!(validate_wallpaper(&p2).is_err());

        // Empty file.
        let p3 = write_temp(&[], "empty.png");
        assert!(validate_wallpaper(&p3).is_err());
    }

    #[test]
    fn wallpaper_validation_accepts_png() {
        let mut png = vec![0u8; 32];
        png[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        let p = write_temp(&png, "real.png");
        assert!(validate_wallpaper(&p).is_ok());
    }

    #[test]
    fn sanitize_clamps_and_resets() {
        let mut cfg = AppearanceConfig {
            theme: "../escape".into(),
            wallpaper: WallpaperSettings {
                fit: "bogus".into(),
                position: "nowhere".into(),
                opacity: 5.0,
                blur: -3.0,
                overlay: 2.0,
                overlay_mode: "weird".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        sanitize(&mut cfg);
        assert_eq!(cfg.theme, OFFICIAL_THEME_ID);
        assert_eq!(cfg.wallpaper.fit, "cover");
        assert_eq!(cfg.wallpaper.position, "center");
        assert_eq!(cfg.wallpaper.opacity, 1.0);
        assert_eq!(cfg.wallpaper.blur, 0.0);
        assert_eq!(cfg.wallpaper.overlay, 1.0);
        assert_eq!(cfg.wallpaper.overlay_mode, "auto");

        // A safe custom id must survive sanitize.
        let mut ok = AppearanceConfig {
            theme: "mystyle".into(),
            ..Default::default()
        };
        sanitize(&mut ok);
        assert_eq!(ok.theme, "mystyle");
    }

    #[test]
    fn validate_active_theme_resets_missing_custom() {
        let state = test_state();
        state.config.lock().unwrap().theme = "missing-theme".into();
        validate_active_theme(&state);
        assert_eq!(state.config.lock().unwrap().theme, OFFICIAL_THEME_ID);
    }

    #[test]
    fn stored_name_keeps_supported_extension() {
        assert_eq!(stored_name_for("a.PNG"), "wallpaper.png");
        assert_eq!(stored_name_for("b.webp"), "wallpaper.webp");
        assert_eq!(stored_name_for("c.jpeg"), "wallpaper.jpeg");
        assert_eq!(stored_name_for("d.gif"), "wallpaper.png");
    }

    #[test]
    fn builtin_themes_parse() {
        assert_eq!(
            resolve_theme(&test_state(), OFFICIAL_THEME_ID).unwrap().id,
            "official"
        );
        let ocean = resolve_theme(&test_state(), "ocean").unwrap();
        assert_eq!(ocean.id, "ocean");
        assert!(!ocean.tokens.light.is_empty());
        assert!(!ocean.tokens.dark.is_empty());
        assert!(ocean
            .asset
            .as_ref()
            .unwrap()
            .starts_with("data:image/svg+xml;base64,"));
        let starter = resolve_theme(&test_state(), "starter").unwrap();
        assert_eq!(starter.id, "starter");
        assert!(starter
            .components
            .as_ref()
            .unwrap()
            .contains("border-radius"));
        assert!(starter.motion.as_ref().unwrap().contains("@keyframes"));
    }

    #[test]
    fn custom_theme_asset_inlined() {
        let state = test_state();
        let dir = state.themes_dir.join("mytheme");
        fs::create_dir_all(dir.join("assets")).unwrap();
        let svg = "<svg xmlns='http://www.w3.org/2000/svg'></svg>";
        fs::write(dir.join("assets").join("bg.svg"), svg).unwrap();
        fs::write(
            dir.join("theme.json"),
            r#"{"id":"mytheme","name":"My","asset":"assets/bg.svg","tokens":{"light":{"--dsw-alias-bg-base":"rgb(1,2,3)"},"dark":{}}}"#,
        )
        .unwrap();
        let theme = resolve_theme(&state, "mytheme").unwrap();
        let asset = theme.asset.unwrap();
        assert!(
            asset.starts_with("url(data:image/svg+xml;base64,"),
            "got: {asset}"
        );
    }

    #[test]
    fn engine_payload_official_has_no_wallpaper() {
        let state = test_state();
        let p = engine_payload(&state).unwrap();
        assert_eq!(p["theme"]["id"], "official");
        assert_eq!(p["wallpaper"]["active"], false);
        assert_eq!(p["wallpaper"]["url"], "");
    }

    #[test]
    fn engine_payload_wallpaper_url_bumps_with_version() {
        let state = test_state();
        {
            let mut cfg = state.config.lock().unwrap();
            cfg.wallpaper.active = true;
            cfg.wallpaper.file_name = Some("wallpaper.png".into());
            cfg.wallpaper.version = 7;
        }
        let p = engine_payload(&state).unwrap();
        assert_eq!(
            p["wallpaper"]["url"],
            crate::platform::custom_scheme_url("hd-wallpaper", "current?v=7")
        );
        assert_eq!(p["wallpaper"]["active"], true);
    }

    #[test]
    fn config_round_trip_through_file() {
        let state = test_state();
        let path = state.config_dir.join(CONFIG_FILE);
        {
            let mut cfg = state.config.lock().unwrap();
            cfg.theme = "ocean".into();
            cfg.motion_enabled = false;
            cfg.wallpaper.opacity = 0.3;
        }
        save_config(&state).unwrap();
        let loaded = load_config(&path);
        assert_eq!(loaded.theme, "ocean");
        assert!(!loaded.motion_enabled);
        assert_eq!(loaded.wallpaper.opacity, 0.3);
    }

    // -----------------------------------------------------------------------
    // Asset containment
    // -----------------------------------------------------------------------

    fn write_theme_json(state: &AppearanceState, id: &str, asset: &str) -> PathBuf {
        let dir = state.themes_dir.join(id);
        fs::create_dir_all(dir.join("assets")).unwrap();
        let json = serde_json::json!({
            "id": id,
            "name": id,
            "asset": asset,
            "tokens": { "light": {}, "dark": {} },
        });
        fs::write(dir.join("theme.json"), json.to_string()).unwrap();
        dir
    }

    fn png_bytes() -> Vec<u8> {
        let mut png = vec![0u8; 32];
        png[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        png
    }

    #[test]
    fn asset_normal_and_nested_accepted() {
        let state = test_state();
        let dir = write_theme_json(&state, "asset-normal", "assets/bg.svg");
        fs::write(
            dir.join("assets").join("bg.svg"),
            "<svg xmlns='http://www.w3.org/2000/svg'></svg>",
        )
        .unwrap();
        let nested = write_theme_json(&state, "asset-nested", "assets/background/image.png");
        fs::create_dir_all(nested.join("assets").join("background")).unwrap();
        fs::write(
            nested.join("assets").join("background").join("image.png"),
            png_bytes(),
        )
        .unwrap();

        let normal = resolve_theme(&state, "asset-normal").unwrap();
        assert!(normal
            .asset
            .unwrap()
            .starts_with("url(data:image/svg+xml;base64,"));
        let nested_theme = resolve_theme(&state, "asset-nested").unwrap();
        assert!(nested_theme
            .asset
            .unwrap()
            .starts_with("url(data:image/png;base64,"));
    }

    #[test]
    fn asset_invalid_paths_rejected() {
        let state = test_state();
        for (id, asset) in [
            ("abs", "/etc/passwd"),
            ("dot", "."),
            ("dotdot", ".."),
            ("dot-slash", "./secret.svg"),
            ("dotdot-slash", "../secret.svg"),
            ("escape", "assets/../../etc/passwd"),
            ("trailing-slash", "assets/"),
            ("double-slash", "assets//x.svg"),
            ("backslash", "assets/foo\\bar.svg"),
            ("protocol-rel", "//evil.com/x.png"),
            ("file", "file:///etc/passwd"),
            ("http", "http://evil.com/x.png"),
            ("https", "https://evil.com/x.png"),
            ("bare-path", "secret.svg"),
        ] {
            write_theme_json(&state, id, asset);
            assert!(
                resolve_theme(&state, id).is_err(),
                "expected rejection for asset {asset:?}"
            );
        }
    }

    #[test]
    fn asset_missing_rejected() {
        let state = test_state();
        write_theme_json(&state, "asset-missing", "assets/nope.svg");
        assert!(resolve_theme(&state, "asset-missing").is_err());
    }

    #[test]
    fn asset_unsupported_extension_rejected() {
        let state = test_state();
        let dir = write_theme_json(&state, "asset-ext", "assets/foo.txt");
        fs::write(dir.join("assets").join("foo.txt"), b"not an image").unwrap();
        assert!(resolve_theme(&state, "asset-ext").is_err());
    }

    #[test]
    fn asset_oversized_rejected() {
        let state = test_state();
        let dir = write_theme_json(&state, "asset-big", "assets/big.svg");
        let file = dir.join("assets").join("big.svg");
        fs::File::create(&file)
            .unwrap()
            .set_len(MAX_ASSET_BYTES + 1)
            .unwrap();
        assert!(resolve_theme(&state, "asset-big").is_err());
    }

    #[test]
    fn asset_raster_magic_mismatch_rejected() {
        let state = test_state();

        // PNG extension but not PNG content.
        let dir = write_theme_json(&state, "asset-fake-png", "assets/fake.png");
        fs::write(dir.join("assets").join("fake.png"), b"GIF89a").unwrap();
        assert!(resolve_theme(&state, "asset-fake-png").is_err());

        // JPEG extension but not JPEG content.
        let dir = write_theme_json(&state, "asset-fake-jpg", "assets/fake.jpg");
        fs::write(dir.join("assets").join("fake.jpg"), b"notjpeg").unwrap();
        assert!(resolve_theme(&state, "asset-fake-jpg").is_err());

        // WebP extension but not WebP content.
        let dir = write_theme_json(&state, "asset-fake-webp", "assets/fake.webp");
        fs::write(dir.join("assets").join("fake.webp"), b"notwebp").unwrap();
        assert!(resolve_theme(&state, "asset-fake-webp").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn asset_external_symlink_rejected() {
        let state = test_state();
        let dir = write_theme_json(&state, "asset-extlink", "assets/evil.svg");
        let outside = state.config_dir.join("outside-secret.txt");
        fs::write(&outside, b"secret").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("assets").join("evil.svg")).unwrap();
        assert!(resolve_theme(&state, "asset-extlink").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn asset_symlink_root_escape_rejected() {
        let state = test_state();
        let dir = write_theme_json(&state, "asset-rootlink", "assets/foo.svg");
        fs::remove_dir_all(dir.join("assets")).unwrap();
        let outside = state.config_dir.join("outside-assets");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("foo.svg"), "<svg/>").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("assets")).unwrap();
        assert!(resolve_theme(&state, "asset-rootlink").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn asset_other_theme_access_rejected() {
        let state = test_state();
        let dir_a = write_theme_json(&state, "theme-a", "assets/secret.svg");
        let dir_b = write_theme_json(&state, "theme-b", "assets/real.svg");
        fs::write(dir_b.join("assets").join("real.svg"), "<svg/>").unwrap();
        std::os::unix::fs::symlink(
            dir_b.join("assets").join("real.svg"),
            dir_a.join("assets").join("secret.svg"),
        )
        .unwrap();
        assert!(resolve_theme(&state, "theme-a").is_err());
    }

    #[test]
    fn asset_data_image_valid_accepted() {
        let state = test_state();
        write_theme_json(
            &state,
            "data-svg",
            &format!("data:image/svg+xml;base64,{}", base64_encode(b"<svg/>")),
        );
        let theme = resolve_theme(&state, "data-svg").unwrap();
        assert!(theme
            .asset
            .unwrap()
            .starts_with("data:image/svg+xml;base64,"));

        write_theme_json(
            &state,
            "data-png",
            &format!("data:image/png;base64,{}", base64_encode(&png_bytes())),
        );
        let theme = resolve_theme(&state, "data-png").unwrap();
        assert!(theme.asset.unwrap().starts_with("data:image/png;base64,"));
    }

    #[test]
    fn asset_data_image_invalid_rejected() {
        let state = test_state();

        // Non-image MIME.
        write_theme_json(
            &state,
            "data-html",
            "data:text/html;base64,PGh0bWw+PC9odG1sPg==",
        );
        assert!(resolve_theme(&state, "data-html").is_err());

        // Not base64.
        write_theme_json(&state, "data-nonb64", "data:image/png,rawbytes");
        assert!(resolve_theme(&state, "data-nonb64").is_err());

        // Oversized decoded payload.
        let big = vec![b'a'; (MAX_ASSET_BYTES as usize) + 1];
        write_theme_json(
            &state,
            "data-big",
            &format!("data:image/svg+xml;base64,{}", base64_encode(&big)),
        );
        assert!(resolve_theme(&state, "data-big").is_err());
    }

    #[test]
    fn ocean_asset_remains_accepted() {
        let ocean = resolve_theme(&test_state(), "ocean").unwrap();
        assert!(ocean
            .asset
            .unwrap()
            .starts_with("data:image/svg+xml;base64,"));
        let starter = resolve_theme(&test_state(), "starter").unwrap();
        assert!(starter.asset.unwrap().starts_with("linear-gradient("));
    }

    #[test]
    fn invalid_theme_id_rejected() {
        let state = test_state();
        for id in ["../escape", "a/b", "a\\b", "..", "."] {
            assert!(
                resolve_theme(&state, id).is_err(),
                "expected rejection for id {id:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Token / surface key safety (BLOCKER 1)
    // -----------------------------------------------------------------------

    #[test]
    fn token_key_valid_forms_accepted() {
        for key in [
            "--dsw-alias-bg-base",
            "--dsw-alias-brand-primary-new-colorprimary-new-color",
            "--x",
            "--foo-bar",
            "--foo_bar",
            "--_leading_underscore",
            "--a1",
        ] {
            validate_token_key(key).unwrap_or_else(|e| panic!("expected valid key {key:?}: {e}"));
        }
    }

    #[test]
    fn token_key_malicious_forms_rejected() {
        for key in [
            // Grammar delimiters / rule injection.
            "--x; } html { color:red",
            "--x: red",
            "--x}",
            "--x{",
            // Whitespace / comment / newline.
            "--x y",
            "--x\ty",
            "--x\ny",
            "--x/*",
            // Escaped delimiter / raw escape.
            "--x\\:y",
            "--x\\}",
            "--x\\;",
            // Malformed custom property identifiers.
            "--",
            "-",
            "--5foo",
            "---foo",
            "x",
            " --x",
            "--x ",
            "--x!important",
        ] {
            assert!(
                validate_token_key(key).is_err(),
                "expected rejection for key {key:?}"
            );
        }
    }

    #[test]
    fn malicious_token_key_rejects_whole_theme() {
        let state = test_state();
        let dir = state.themes_dir.join("bad-token");
        fs::create_dir_all(&dir).unwrap();
        let json = serde_json::json!({
            "id": "bad-token",
            "name": "Bad",
            "tokens": {
                "light": { "--ok": "rgb(1,2,3)", "--x; } html { color:red": "red" },
                "dark": {}
            },
        });
        fs::write(dir.join("theme.json"), json.to_string()).unwrap();
        assert!(resolve_theme(&state, "bad-token").is_err());
    }

    #[test]
    fn malicious_surface_key_rejects_whole_theme() {
        let state = test_state();
        let dir = state.themes_dir.join("bad-surface");
        fs::create_dir_all(&dir).unwrap();
        let json = serde_json::json!({
            "id": "bad-surface",
            "name": "Bad",
            "tokens": { "light": {}, "dark": {} },
            "surfaces": {
                "light": { "--ok": "rgba(0,0,0,.5)", "--x\\}": "red" },
                "dark": {}
            },
        });
        fs::write(dir.join("theme.json"), json.to_string()).unwrap();
        assert!(resolve_theme(&state, "bad-surface").is_err());
    }

    // -----------------------------------------------------------------------
    // Token / surface value safety (TOKEN_VALUE_URL_REACHABILITY)
    // -----------------------------------------------------------------------

    #[test]
    fn token_value_valid_color_forms_accepted() {
        for v in [
            "rgb(25, 108, 150)",
            "rgba(243, 247, 250, 0.92)",
            "rgba(0,0,0,.5)",
            "rgb(100%, 0%, 0%)",
            "rgb(255,255,255)",
            "rgba(26, 21, 20, 0.55)",
        ] {
            validate_token_value(v).unwrap_or_else(|e| panic!("expected valid value {v:?}: {e}"));
        }
    }

    #[test]
    fn token_value_url_forms_rejected() {
        for v in [
            "url(https://example.invalid/x.png)",
            "URL(https://example.invalid/x.png)",
            "url(\"https://example.invalid/x.png\")",
            "u\\72l(https://example.invalid/x.png)",
            "\\75rl(https://example.invalid/x.png)",
            "url(data:image/png;base64,AAAA)",
            "url(foo.png)",
            "url(/**/https://example.invalid/x.png)",
            "image-set(url(x) 1x)",
            "image-set(\"https://example.invalid/x.png\" 1x)",
            "-webkit-image-set(url(x) 1x)",
            "cross-fade(red, blue)",
            "image(url(x))",
        ] {
            assert!(
                validate_token_value(v).is_err(),
                "expected rejection for value {v:?}"
            );
        }
    }

    #[test]
    fn token_value_trailing_and_malformed_rejected() {
        for v in [
            "rgb(25, 108, 150) none",
            "rgb(25, 108, 150)/*trailing*/",
            "rgb(25, 108, 150, 0.5)",
            "rgb(25, 108)",
            "rgb(25, 108, 150,)",
            "rgb(25, 108, url(x))",
            "rgb(25, 108, var(--x))",
            "rgb(25, 108, calc(100% - 10px))",
            "rgb(25 108 150)",
            "rgb(25, 108, 150",
            "rgb 25, 108, 150",
            "var(--dsw-alias-bg-base)",
            "calc(100% - 10px)",
            "hsl(120, 50%, 50%)",
            "red",
            "#fff",
            "transparent",
            "rgb(25, 108, 150) !important",
        ] {
            assert!(
                validate_token_value(v).is_err(),
                "expected rejection for value {v:?}"
            );
        }
    }

    #[test]
    fn malicious_token_value_rejects_whole_theme() {
        let state = test_state();
        let dir = state.themes_dir.join("bad-value");
        fs::create_dir_all(&dir).unwrap();
        let json = serde_json::json!({
            "id": "bad-value",
            "name": "Bad",
            "tokens": {
                "light": { "--dsw-alias-bg-base": "url(https://example.invalid/x.png)" },
                "dark": {}
            },
        });
        fs::write(dir.join("theme.json"), json.to_string()).unwrap();
        assert!(resolve_theme(&state, "bad-value").is_err());
    }

    #[test]
    fn var_indirection_rejected_whole_theme() {
        let state = test_state();
        let dir = state.themes_dir.join("var-theme");
        fs::create_dir_all(&dir).unwrap();
        let json = serde_json::json!({
            "id": "var-theme",
            "name": "Var",
            "tokens": {
                "light": { "--a": "var(--b)", "--b": "url(https://example.invalid/x.png)" },
                "dark": {}
            },
        });
        fs::write(dir.join("theme.json"), json.to_string()).unwrap();
        assert!(resolve_theme(&state, "var-theme").is_err());
    }

    #[test]
    fn valid_token_value_theme_resolves_into_payload() {
        let state = test_state();
        let dir = state.themes_dir.join("good-value");
        fs::create_dir_all(&dir).unwrap();
        let json = serde_json::json!({
            "id": "good-value",
            "name": "Good",
            "tokens": {
                "light": { "--dsw-alias-bg-base": "rgba(243, 247, 250, 0.92)" },
                "dark": {}
            },
        });
        fs::write(dir.join("theme.json"), json.to_string()).unwrap();
        let theme = resolve_theme(&state, "good-value").unwrap();
        assert_eq!(
            theme.tokens.light["--dsw-alias-bg-base"],
            "rgba(243, 247, 250, 0.92)"
        );
    }

    // -----------------------------------------------------------------------
    // Gradient asset safety (BLOCKER 2)
    // -----------------------------------------------------------------------

    #[test]
    fn gradient_valid_forms_accepted() {
        for g in [
            "linear-gradient(180deg, #3a1d2e 0%, #2a1520 34%, #1c1116 100%)",
            "radial-gradient(circle, red, blue)",
            "conic-gradient(from 0deg, red, blue)",
            "repeating-linear-gradient(45deg, #000 0px, #fff 10px)",
            "linear-gradient(rgba(0,0,0,.5), #fff)",
        ] {
            validate_gradient(g).unwrap_or_else(|e| panic!("expected valid gradient {g:?}: {e}"));
        }
    }

    #[test]
    fn gradient_malicious_forms_rejected() {
        for g in [
            // Rule / declaration escape.
            "linear-gradient(red, blue); } html { color: red",
            "linear-gradient(red, blue); } html{color:red}",
            // Trailing tokens.
            "linear-gradient(red, blue) none",
            "linear-gradient(red, blue) ",
            // URL-bearing content (colon/slash caught by whitelist, bare caught by url scan).
            "linear-gradient(url(http://evil), red)",
            "linear-gradient(url(foo), red)",
            // Unknown / unsafe function.
            "cross-fade(red, blue)",
            "-webkit-gradient(linear, left top, left bottom, from(red), to(blue))",
            "image-set(url(x) 1x)",
            // Malformed / unclosed / unbalanced.
            "linear-gradient(red, blue",
            "linear-gradient red",
            "linear-gradient)",
            "linear-gradient((red, blue)",
            // Multiple image values.
            "linear-gradient(red, blue), linear-gradient(green, yellow)",
            // Comment / escape / string bypass.
            "linear-gradient(red, blue) /* } */",
            "linear-gradient(red, blue)/*x*/",
            "linear-gradient(red, blue)\\}",
            "linear-gradient(\"red\", blue)",
        ] {
            assert!(
                validate_gradient(g).is_err(),
                "expected rejection for gradient {g:?}"
            );
        }
    }

    #[test]
    fn malicious_gradient_asset_rejects_theme() {
        let state = test_state();
        write_theme_json(
            &state,
            "bad-gradient",
            "linear-gradient(red, blue); } html { color: red }",
        );
        assert!(resolve_theme(&state, "bad-gradient").is_err());
    }

    // -----------------------------------------------------------------------
    // Data image canonicalization (BLOCKER 2)
    // -----------------------------------------------------------------------

    #[test]
    fn data_image_canonicalized_from_decoded_bytes() {
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg'></svg>";
        let input = format!("data:image/svg+xml;base64,{}", base64_encode(svg));
        let out = canonicalize_data_image(&input).unwrap();
        // Canonical: same MIME, re-encoded bytes (must be identical for valid input).
        assert_eq!(out, input);

        // Mixed-case MIME is canonicalized to lowercase.
        let mixed = format!("data:IMAGE/SVG+XML;base64,{}", base64_encode(svg));
        let out = canonicalize_data_image(&mixed).unwrap();
        assert_eq!(
            out,
            format!("data:image/svg+xml;base64,{}", base64_encode(svg))
        );

        let png = png_bytes();
        let png_input = format!("data:image/png;base64,{}", base64_encode(&png));
        let out = canonicalize_data_image(&png_input).unwrap();
        assert_eq!(out, png_input);
    }

    #[test]
    fn data_image_malicious_forms_rejected() {
        // Unsupported MIME.
        assert!(canonicalize_data_image("data:text/html;base64,PGh0bWw+PC9odG1sPg==").is_err());
        // Not base64.
        assert!(canonicalize_data_image("data:image/png,rawbytes").is_err());
        // Malformed base64 (non-alphabet char).
        assert!(canonicalize_data_image("data:image/png;base64,not valid base64!!!").is_err());
        // Valid base64 + trailing CSS.
        assert!(canonicalize_data_image(
            "data:image/svg+xml;base64,PHN2Zy8+; } html { color: red }"
        )
        .is_err());
        // Valid padding + trailing CSS.
        assert!(
            canonicalize_data_image("data:image/png;base64,AAA=; } html { color: red }").is_err()
        );
        // Trailing token after decoded payload.
        assert!(canonicalize_data_image("data:image/svg+xml;base64,PHN2Zy8+ extra").is_err());
        // Invalid padding in the middle.
        assert!(canonicalize_data_image("data:image/png;base64,A=A").is_err());
        // Whitespace inside payload (not strictly consumed).
        assert!(canonicalize_data_image("data:image/svg+xml;base64,PHN2 Zy8+").is_err());
    }

    #[test]
    fn data_image_asset_reaches_engine_as_canonical() {
        let state = test_state();
        let raw = format!(
            "data:IMAGE/SVG+XML;base64,{}",
            base64_encode(b"<svg xmlns='http://www.w3.org/2000/svg'></svg>")
        );
        write_theme_json(&state, "data-canon", &raw);
        let theme = resolve_theme(&state, "data-canon").unwrap();
        let asset = theme.asset.unwrap();
        // Re-serialized canonical form, not the raw mixed-case string.
        assert_eq!(
            asset,
            format!(
                "data:image/svg+xml;base64,{}",
                base64_encode(b"<svg xmlns='http://www.w3.org/2000/svg'></svg>")
            )
        );
    }
}

/// Open (or focus) the appearance settings window.
pub fn open_appearance_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("appearance") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let builder = tauri::WebviewWindowBuilder::new(
        app,
        "appearance",
        tauri::WebviewUrl::App("appearance.html".into()),
    )
    .title("Appearance")
    .inner_size(560.0, 640.0)
    .min_inner_size(480.0, 540.0)
    .resizable(true)
    .center();
    if let Err(e) = builder.build() {
        eprintln!("[appearance] failed to open settings window: {e}");
    }
}
