//! `/health` endpoint for Kubernetes liveness and readiness probes.
//!
//! The service is unhealthy only when its sync loop has stalled: no cycle has finished within
//! the fetch interval plus a grace period. A failing Arbor login or fetch does not make it
//! unhealthy, because restarting the pod wouldn't fix that; it shows up instead as a stale
//! "Last successful fetch" in Home Assistant, and in the `last_error` field here.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use chrono::{DateTime, Utc};
use serde::Serialize;

/// Allowance on top of the fetch interval for a slow cycle (about 75 requests on a first run).
const GRACE: Duration = Duration::from_secs(15 * 60);

#[derive(Clone)]
pub struct Health {
    inner: Arc<Mutex<HealthState>>,
    fetch_interval: Duration,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HealthState {
    pub started_at: DateTime<Utc>,
    pub last_cycle_finished_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct HealthReport {
    pub healthy: bool,
    #[serde(flatten)]
    pub state: HealthState,
}

impl Health {
    pub fn new(fetch_interval: Duration) -> Self {
        Self::starting_at(fetch_interval, Utc::now())
    }

    fn starting_at(fetch_interval: Duration, started_at: DateTime<Utc>) -> Self {
        let state = HealthState {
            started_at,
            last_cycle_finished_at: None,
            last_success_at: None,
            last_error: None,
        };
        Self {
            inner: Arc::new(Mutex::new(state)),
            fetch_interval,
        }
    }

    pub fn record_success(&self, at: DateTime<Utc>) {
        let mut state = self.lock();
        state.last_cycle_finished_at = Some(at);
        state.last_success_at = Some(at);
        state.last_error = None;
    }

    pub fn record_failure(&self, at: DateTime<Utc>, error: String) {
        let mut state = self.lock();
        state.last_cycle_finished_at = Some(at);
        state.last_error = Some(error);
    }

    pub fn report(&self, now: DateTime<Utc>) -> HealthReport {
        let state = self.lock().clone();
        let last_activity = state.last_cycle_finished_at.unwrap_or(state.started_at);
        let allowed = chrono::Duration::from_std(self.fetch_interval + GRACE)
            .unwrap_or(chrono::Duration::MAX);
        HealthReport {
            healthy: now - last_activity <= allowed,
            state,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HealthState> {
        // A panic while holding the lock can't leave this simple state inconsistent.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn router(health: Health) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .with_state(health)
}

async fn health_handler(State(health): State<Health>) -> (StatusCode, Json<HealthReport>) {
    let report = health.report(Utc::now());
    let status = if report.healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(report))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_secs(120 * 60);

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + chrono::Duration::minutes(minutes)
    }

    #[test]
    fn healthy_while_first_cycle_is_within_allowance() {
        let health = Health::starting_at(INTERVAL, at(0));
        assert!(health.report(at(135)).healthy);
        assert!(!health.report(at(136)).healthy);
    }

    #[test]
    fn a_failed_cycle_still_counts_as_progress() {
        let health = Health::starting_at(INTERVAL, at(0));
        health.record_failure(at(120), "Arbor login was rejected".into());

        let report = health.report(at(200));
        assert!(report.healthy);
        assert_eq!(
            report.state.last_error.as_deref(),
            Some("Arbor login was rejected")
        );
        assert_eq!(report.state.last_success_at, None);
    }

    #[test]
    fn success_clears_the_last_error() {
        let health = Health::starting_at(INTERVAL, at(0));
        health.record_failure(at(10), "boom".into());
        health.record_success(at(130));

        let report = health.report(at(140));
        assert_eq!(report.state.last_error, None);
        assert_eq!(report.state.last_success_at, Some(at(130)));
    }

    #[test]
    fn unhealthy_when_the_loop_stalls_after_a_cycle() {
        let health = Health::starting_at(INTERVAL, at(0));
        health.record_success(at(10));
        assert!(!health.report(at(10 + 136)).healthy);
    }
}
