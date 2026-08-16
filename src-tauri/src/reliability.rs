//! DeepSeek Harness Desktop — desktop reliability state machine.
//!
//! V0.2 moves Harness Desktop from "can launch Harness" toward a trustworthy
//! long-running Desktop host. This module owns the **explicit, testable**
//! reliability state machine. The truth hierarchy is authoritative:
//!
//!   **backend truth > client DOM heuristic**
//!
//! (The V0.1 injected `session_guard.js` remains the session-truth mechanism;
//! no DOM quiet-window logic is reintroduced here.)
//!
//! Recovery semantics are a hard invariant: recovery restores
//! connection/runtime/UI state only. It never resubmits a prompt, retries agent
//! work, duplicates a tool call, cancels a task, or starts a second task.

/// Reliability lifecycle. Transitions are driven by [`next_state`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliabilityState {
    /// Resolving runtime / spawning / waiting for first readiness.
    Starting,
    /// First readiness observed (HTTP 200); not yet confirmed healthy.
    Ready,
    /// Health checks passing.
    Healthy,
    /// A health check failed while the owned child is still alive (transient).
    Degraded,
    /// Consecutive health failures; still polling conservatively.
    Recovering,
    /// Owned child exited unexpectedly OR health failures passed the budget.
    Failed,
}

impl ReliabilityState {
    pub fn name(self) -> &'static str {
        match self {
            ReliabilityState::Starting => "STARTING",
            ReliabilityState::Ready => "READY",
            ReliabilityState::Healthy => "HEALTHY",
            ReliabilityState::Degraded => "DEGRADED",
            ReliabilityState::Recovering => "RECOVERING",
            ReliabilityState::Failed => "FAILED",
        }
    }
}

/// Why the reliability machine entered `Failed`. Used to build an honest,
/// reason-specific user message (a live-but-unhealthy child must never be
/// reported as "process exited").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureReason {
    /// The owned Harness process tree exited.
    ProcessExited,
    /// The process is still alive but loopback health failed past the budget.
    HealthUnavailable,
    /// The Harness never became ready during startup.
    ReadinessFailed,
}

impl FailureReason {
    /// Human-readable, reason-specific message (no recovery suffix).
    pub fn message(self) -> &'static str {
        match self {
            FailureReason::ProcessExited => "The Harness process exited unexpectedly.",
            FailureReason::HealthUnavailable => {
                "The Harness backend became unresponsive while it was still running."
            }
            FailureReason::ReadinessFailed => "The Harness failed to become ready.",
        }
    }
}

/// Pure helper: the failure reason implied by this tick, when the state machine
/// would enter `Failed`. `None` means no failure on this tick.
///
/// * `child_alive` — whether the owned harness tree is still running.
/// * `consecutive_failures` — failures in a row (already updated for this tick).
/// * `fail_after` — failures before entering `Failed`.
pub fn failure_reason(
    child_alive: bool,
    consecutive_failures: u32,
    fail_after: u32,
) -> Option<FailureReason> {
    if !child_alive {
        return Some(FailureReason::ProcessExited);
    }
    if consecutive_failures >= fail_after {
        return Some(FailureReason::HealthUnavailable);
    }
    None
}

/// Pure state-machine transition.
///
/// * `child_alive` — whether the owned harness tree is still running.
/// * `health_ok` — whether the loopback readiness/health check just passed.
/// * `consecutive_failures` — failures in a row (already updated for this tick).
/// * `recover_after` — failures before entering `Recovering`.
/// * `fail_after` — failures before entering `Failed`.
///
/// Invariants:
///   * An unexpected child exit is authoritative and always yields `Failed`
///     (backend truth > any transient health signal).
///   * `Failed` is terminal (only a manual restart leaves it).
///   * A passing check promotes to `Healthy` (recovery), never restarts work.
pub fn next_state(
    state: ReliabilityState,
    child_alive: bool,
    health_ok: bool,
    consecutive_failures: u32,
    recover_after: u32,
    fail_after: u32,
) -> ReliabilityState {
    if !child_alive {
        return ReliabilityState::Failed;
    }
    if state == ReliabilityState::Failed {
        return ReliabilityState::Failed;
    }
    if health_ok {
        return ReliabilityState::Healthy;
    }
    // Child alive but health failing.
    if consecutive_failures >= fail_after {
        return ReliabilityState::Failed;
    }
    if consecutive_failures >= recover_after {
        return ReliabilityState::Recovering;
    }
    ReliabilityState::Degraded
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECOVER: u32 = 3;
    const FAIL: u32 = 12;

    #[test]
    fn child_exit_is_authoritative_even_if_health_ok() {
        // A stale "health ok" reading racing a child exit must still fail.
        assert_eq!(
            next_state(ReliabilityState::Healthy, false, true, 0, RECOVER, FAIL),
            ReliabilityState::Failed
        );
        assert_eq!(
            next_state(ReliabilityState::Recovering, false, false, 5, RECOVER, FAIL),
            ReliabilityState::Failed
        );
    }

    #[test]
    fn ready_promotes_to_healthy_on_first_pass() {
        assert_eq!(
            next_state(ReliabilityState::Ready, true, true, 0, RECOVER, FAIL),
            ReliabilityState::Healthy
        );
    }

    #[test]
    fn single_failure_degrades_then_recovers() {
        assert_eq!(
            next_state(ReliabilityState::Healthy, true, false, 1, RECOVER, FAIL),
            ReliabilityState::Degraded
        );
        assert_eq!(
            next_state(ReliabilityState::Degraded, true, true, 0, RECOVER, FAIL),
            ReliabilityState::Healthy
        );
    }

    #[test]
    fn consecutive_failures_enter_recovering_then_recover() {
        assert_eq!(
            next_state(ReliabilityState::Healthy, true, false, 2, RECOVER, FAIL),
            ReliabilityState::Degraded
        );
        assert_eq!(
            next_state(ReliabilityState::Degraded, true, false, 3, RECOVER, FAIL),
            ReliabilityState::Recovering
        );
        assert_eq!(
            next_state(ReliabilityState::Recovering, true, false, 5, RECOVER, FAIL),
            ReliabilityState::Recovering
        );
        // Recovery is conservative: a passing check returns to Healthy.
        assert_eq!(
            next_state(ReliabilityState::Recovering, true, true, 0, RECOVER, FAIL),
            ReliabilityState::Healthy
        );
    }

    #[test]
    fn failure_budget_ends_in_failed() {
        assert_eq!(
            next_state(ReliabilityState::Recovering, true, false, FAIL, RECOVER, FAIL),
            ReliabilityState::Failed
        );
        // Failed is terminal.
        assert_eq!(
            next_state(ReliabilityState::Failed, true, true, 0, RECOVER, FAIL),
            ReliabilityState::Failed
        );
    }

    #[test]
    fn starting_is_transient_and_not_monitored_but_safe() {
        assert_eq!(
            next_state(ReliabilityState::Starting, true, true, 0, RECOVER, FAIL),
            ReliabilityState::Healthy
        );
    }

    #[test]
    fn state_names_are_stable_uppercase() {
        assert_eq!(ReliabilityState::Starting.name(), "STARTING");
        assert_eq!(ReliabilityState::Ready.name(), "READY");
        assert_eq!(ReliabilityState::Healthy.name(), "HEALTHY");
        assert_eq!(ReliabilityState::Degraded.name(), "DEGRADED");
        assert_eq!(ReliabilityState::Recovering.name(), "RECOVERING");
        assert_eq!(ReliabilityState::Failed.name(), "FAILED");
    }

    #[test]
    fn failure_reason_child_dead_is_process_exited() {
        assert_eq!(
            failure_reason(false, 0, 12),
            Some(FailureReason::ProcessExited)
        );
        // Even with a passing health reading, a dead child is authoritative.
        assert_eq!(
            failure_reason(false, 0, 12),
            Some(FailureReason::ProcessExited)
        );
    }

    #[test]
    fn failure_reason_health_budget_is_health_unavailable() {
        // Child alive + failures past budget => health-unavailable, NOT exited.
        assert_eq!(
            failure_reason(true, 12, 12),
            Some(FailureReason::HealthUnavailable)
        );
        assert_eq!(
            failure_reason(true, 20, 12),
            Some(FailureReason::HealthUnavailable)
        );
    }

    #[test]
    fn failure_reason_below_budget_is_none() {
        assert_eq!(failure_reason(true, 0, 12), None);
        assert_eq!(failure_reason(true, 11, 12), None);
    }

    #[test]
    fn failure_messages_are_distinct() {
        let exited = FailureReason::ProcessExited.message();
        let unhealthy = FailureReason::HealthUnavailable.message();
        let readiness = FailureReason::ReadinessFailed.message();
        assert_ne!(exited, unhealthy);
        assert_ne!(exited, readiness);
        assert_ne!(unhealthy, readiness);
        // The specific regression: the health path must NOT claim "exited".
        assert!(
            !unhealthy.contains("exited"),
            "health-unavailable message must not say the process exited: {unhealthy}"
        );
        assert!(exited.contains("exited"));
    }
}
