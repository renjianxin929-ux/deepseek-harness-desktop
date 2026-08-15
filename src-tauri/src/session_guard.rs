//! Stale session guard — embeds the injected reconciliation script and pins
//! its safety invariants at compile-time/regression-test time.
//!
//! The guard is deliberately a plain injected page script (same sandbox as the
//! appearance engine): it has no Tauri IPC, so it cannot stop/restart the
//! harness process, and it is structurally unable to submit prompts or cancel
//! turns — its only side effects are a read-only `session.list` fetch and, at
//! most, a same-origin `location.reload()`. The tests below assert that
//! contract so a future edit cannot silently widen the guard's surface.

/// Injected guard script (self-contained JS; see `session_guard.js`).
pub const SCRIPT: &str = include_str!("session_guard.js");

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    // The guard must never be able to (re)submit a prompt, cancel a turn,
    // create/fork a session, mutate the queue, or reach any Tauri process
    // control. These tokens must never appear anywhere in the injected script.
    const FORBIDDEN: &[&str] = &[
        "session.prompt",
        "session.cancel",
        "session.create",
        "session.fork",
        "session.updateQueue",
        "session.attachment",
        "session.selectModel",
        "session.rename",
        "__TAURI__",
        "window.__TAURI_INTERNALS__",
    ];

    #[test]
    fn guard_script_contains_no_destructive_operations() {
        for token in FORBIDDEN {
            assert!(
                !SCRIPT.contains(token),
                "guard script must not contain forbidden operation: {token}"
            );
        }
    }

    #[test]
    fn guard_script_only_reads_session_list_and_reloads() {
        // The only backend query is the read-only session list.
        assert!(SCRIPT.contains("\"/api/session.list\""));
        assert!(SCRIPT.contains("method: \"session.list\""));
        // The only recovery action is a same-origin reload.
        assert!(SCRIPT.contains("location.reload()"));
        // The re-syncing banner is non-destructive and locale-aware.
        assert!(SCRIPT.contains("重新同步任务状态"));
        assert!(SCRIPT.contains("Re-syncing task state"));
    }

    #[test]
    fn guard_script_reconciles_per_current_session_not_global() {
        // Truth is resolved against the official persisted current-session
        // selection, never a global "any session running" heuristic.
        assert!(SCRIPT.contains("\"dsh.sessions.current\""));
        assert!(SCRIPT.contains("currentRunning"));
        assert!(
            !SCRIPT.contains("anyRunning"),
            "guard must not use the global any-running heuristic"
        );
    }

    #[test]
    fn guard_script_has_no_dom_freshness_dependency() {
        // V0.1 stale-session fail-safe: the guard polls the current session's
        // backend truth while the client shows running, and must NOT gate
        // reconciliation on a
        // DOM-mutation quiet window. A ticking elapsed clock (or continuous DOM
        // churn) can therefore never suppress a reconciliation.
        assert!(SCRIPT.contains("POLL_INTERVAL_MS"));
        assert!(SCRIPT.contains("= 30000"));
        assert!(
            !SCRIPT.contains("STALE_AFTER_MS"),
            "the stale quiet window must be removed"
        );
        assert!(
            !SCRIPT.contains("MutationObserver"),
            "DOM-mutation tracking must be removed"
        );
        assert!(
            !SCRIPT.contains("COSMETIC_SELECTOR"),
            "cosmetic DOM filtering must be removed"
        );
        assert!(
            !SCRIPT.contains("lastProgressAt"),
            "last-progress timestamp must be removed"
        );
    }
}
