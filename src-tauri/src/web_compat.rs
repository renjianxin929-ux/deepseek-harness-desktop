//! Web Platform compatibility shim (old WKWebView on macOS Monterey).
//!
//! The bundled Harness rc.7 UI uses `AbortSignal.timeout()` /
//! `AbortSignal.any()` in its bounded-unary request path; macOS Monterey's
//! WKWebView does not provide them, which breaks Settings RPC. The shim is
//! injected at document start, before the Harness bundle, and only installs
//! feature-detected fallbacks when the native APIs are missing.
//!
//! See `web_compat.js` for the full rationale and the `__HD_WEB_COMPAT__`
//! observability marker.

/// Injected compat shim (self-contained JS; see `web_compat.js`).
pub const SCRIPT: &str = include_str!("web_compat.js");

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    #[test]
    fn shim_is_feature_detected_and_self_contained() {
        // Never overrides natives: both installs must be guarded by a
        // `typeof ... === "function"` check.
        assert!(
            SCRIPT.contains("MARK.timeoutNative = typeof AbortSignal.timeout === \"function\";"),
            "timeout polyfill must be feature-detected"
        );
        assert!(
            SCRIPT.contains("MARK.anyNative = typeof AbortSignal.any === \"function\";"),
            "any polyfill must be feature-detected"
        );
        // Self-contained: no network/filesystem access, no third-party fetch.
        assert!(!SCRIPT.contains("fetch("), "shim must not touch global fetch");
        assert!(!SCRIPT.contains("XMLHttpRequest"), "shim must not use XHR");
        assert!(!SCRIPT.contains("require("), "shim must not use Node require");
        // Observability marker required by the injection-order gate.
        assert!(
            SCRIPT.contains("window.__HD_WEB_COMPAT__"),
            "shim must expose window.__HD_WEB_COMPAT__"
        );
    }

    #[test]
    fn shim_exposes_no_secrets_or_tokens() {
        for token in [
            "ghp_", "AKIA", "-----BEGIN", "sk-", "xoxb-", "password", "secret", "token",
        ] {
            assert!(!SCRIPT.contains(token), "shim must not contain {token}");
        }
    }
}
