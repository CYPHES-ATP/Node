//! The autonomous labor tick.
//!
//! Ported from `runGenesisAutoTick` in `src/App.tsx`, which drove the whole
//! network from a browser `setInterval`. The ordering and the backpressure gates
//! are deliberately identical to the cockpit's, because they encode hard-won
//! settlement behaviour:
//!
//! 1. **Verify first.** A node that only produces work while nobody verifies
//!    wedges the network — receipts pile up unsettled. Verification is the
//!    scarce duty, so it runs before anything else.
//! 2. **Refuse to claim while the local verification pool is dirty.** Claiming
//!    more work while receipts sit unverified strands them.
//! 3. **Only then claim and run.**
//!
//! Seeding new campaigns is intentionally not implemented here yet; see the
//! module note in `headless.rs` and the v0.17.3 release notes.

use std::collections::HashMap;

use crate::{
    audit_labor::WORK_UNIT_CLAIM_TTL_MS,
    commands::{claim_work_unit_headless, run_work_unit_headless, verify_next_pending_headless},
    events::EventSink,
    state::P2pState,
    store::{AtpStore, MAX_PENDING_CONTRIBUTIONS_PER_WORKER},
};

const WORK_UNIT_RETRY_DELAY_MS: u64 = 60_000;
const MAX_RUN_ATTEMPTS_PER_CLAIM: u8 = 2;
/// Total failed runs, across *all* claims and restarts, after which this node
/// stops attempting a work unit. A unit that fails deterministically -- an
/// oversized repository, a model that cannot emit a parseable answer -- would
/// otherwise be retried forever, because a re-claim resets the per-claim
/// counter and a restart used to drop the backoff entirely. Abandonment is
/// local only: the unit stays open for other workers, who may well have a
/// model or a context window that succeeds where this node failed.
const MAX_TOTAL_ATTEMPTS_BEFORE_ABANDON: u32 = 6;
/// How often an otherwise-silent verifier reports that it is alive.
///
/// The tick runs every 12s; logging idleness every tick would bury real events.
/// Five minutes is frequent enough that an operator tailing logs sees the node
/// is breathing, and rare enough to stay readable in `journalctl`.
const VERIFIER_IDLE_LOG_INTERVAL_MS: u64 = 300_000;

#[derive(Debug, Clone)]
pub struct AutonomousConfig {
    pub contribute: bool,
    pub provider: String,
    pub model: String,
    pub max_runtime_seconds: Option<u64>,
    pub max_daily_work_units: u32,
}

/// Mutable counters the loop carries between ticks.
///
/// The cockpit persists the equivalent in `localStorage`; a headless daemon
/// keeps them in process and rolls them at UTC midnight. The practical
/// difference is that a restart resets the day's tally, which is acceptable for
/// a spend guard but means the cap is a throttle, not an accounting record.
#[derive(Debug, Default)]
pub struct AutonomousState {
    work_units_today: u32,
    day: Option<i64>,
    failed_work_units: HashMap<String, WorkUnitFailure>,
    last_idle_log_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct WorkUnitFailure {
    /// Failures under the current claim. Reset when a fresh claim is taken, so
    /// the short retry delay stays per-claim as before.
    attempts: u8,
    /// Failures over the lifetime of this work unit on this node. Never reset
    /// except on success, and persisted, so it survives both re-claims and
    /// restarts. This is what bounds the retry loop.
    total_attempts: u32,
    retry_after_ms: u64,
    abandoned: bool,
}

impl AutonomousState {
    fn roll_day(&mut self, now_ms: i64) {
        let today = now_ms / 86_400_000;
        if self.day != Some(today) {
            self.day = Some(today);
            self.work_units_today = 0;
        }
    }

    fn at_daily_cap(&self, config: &AutonomousConfig) -> bool {
        config.max_daily_work_units > 0 && self.work_units_today >= config.max_daily_work_units
    }

    fn failure_key(campaign_id: &str, work_unit_id: &str) -> String {
        format!("{campaign_id}:{work_unit_id}")
    }

    fn retry_ready(&self, campaign_id: &str, work_unit_id: &str, now_ms: u64) -> bool {
        self.failed_work_units
            .get(&Self::failure_key(campaign_id, work_unit_id))
            .is_none_or(|failure| now_ms >= failure.retry_after_ms)
    }

    /// True at most once per `VERIFIER_IDLE_LOG_INTERVAL_MS`.
    fn should_log_idle(&mut self, now_ms: u64) -> bool {
        let due = self
            .last_idle_log_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= VERIFIER_IDLE_LOG_INTERVAL_MS);
        if due {
            self.last_idle_log_ms = Some(now_ms);
        }
        due
    }

    fn is_abandoned(&self, campaign_id: &str, work_unit_id: &str) -> bool {
        self.failed_work_units
            .get(&Self::failure_key(campaign_id, work_unit_id))
            .is_some_and(|failure| failure.abandoned)
    }

    /// Load persisted backoff state. Called once at startup; without it a
    /// restart clears every give-up decision this node has made.
    pub fn hydrate(&mut self, store: &AtpStore) {
        match store.load_worker_work_unit_failures() {
            Ok(rows) => {
                let abandoned = rows.iter().filter(|row| row.5).count();
                for (campaign_id, work_unit_id, attempts, total_attempts, retry_after_ms, is_ab) in
                    rows
                {
                    self.failed_work_units.insert(
                        Self::failure_key(&campaign_id, &work_unit_id),
                        WorkUnitFailure {
                            attempts,
                            total_attempts,
                            retry_after_ms,
                            abandoned: is_ab,
                        },
                    );
                }
                if !self.failed_work_units.is_empty() {
                    tracing::info!(
                        tracked = self.failed_work_units.len(),
                        abandoned,
                        "[CONTRIBUTE] restored work unit backoff state"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not restore work unit backoff state");
            }
        }
    }

    /// Clear all failure state for a unit. Only correct after a *successful*
    /// run -- clearing on re-claim is what previously made the attempt budget
    /// unbounded.
    fn clear_failure(&mut self, store: &AtpStore, campaign_id: &str, work_unit_id: &str) {
        if self
            .failed_work_units
            .remove(&Self::failure_key(campaign_id, work_unit_id))
            .is_some()
        {
            if let Err(error) = store.clear_worker_work_unit_failure(campaign_id, work_unit_id) {
                tracing::warn!(%error, "could not clear persisted work unit backoff");
            }
        }
    }

    /// Reset only the per-claim attempt counter, preserving the lifetime total.
    fn reset_claim_attempts(&mut self, store: &AtpStore, campaign_id: &str, work_unit_id: &str) {
        let key = Self::failure_key(campaign_id, work_unit_id);
        let Some(failure) = self.failed_work_units.get_mut(&key) else {
            return;
        };
        failure.attempts = 0;
        let failure = *failure;
        if let Err(error) = store.upsert_worker_work_unit_failure(
            campaign_id,
            work_unit_id,
            failure.attempts,
            failure.total_attempts,
            failure.retry_after_ms,
            failure.abandoned,
            None,
        ) {
            tracing::warn!(%error, "could not persist work unit backoff");
        }
    }

    fn record_failure(
        &mut self,
        store: &AtpStore,
        campaign_id: &str,
        work_unit_id: &str,
        now_ms: u64,
        error_text: &str,
    ) -> WorkUnitFailure {
        let key = Self::failure_key(campaign_id, work_unit_id);
        let previous = self
            .failed_work_units
            .get(&key)
            .copied()
            .unwrap_or_default();
        let attempts = previous.attempts.saturating_add(1);
        let total_attempts = previous.total_attempts.saturating_add(1);
        let abandoned = total_attempts >= MAX_TOTAL_ATTEMPTS_BEFORE_ABANDON;
        let delay_ms = if attempts >= MAX_RUN_ATTEMPTS_PER_CLAIM {
            WORK_UNIT_CLAIM_TTL_MS
        } else {
            WORK_UNIT_RETRY_DELAY_MS
        };
        let failure = WorkUnitFailure {
            attempts,
            total_attempts,
            retry_after_ms: now_ms.saturating_add(delay_ms),
            abandoned,
        };
        self.failed_work_units.insert(key, failure);
        if let Err(error) = store.upsert_worker_work_unit_failure(
            campaign_id,
            work_unit_id,
            failure.attempts,
            failure.total_attempts,
            failure.retry_after_ms,
            failure.abandoned,
            Some(error_text),
        ) {
            tracing::warn!(%error, "could not persist work unit backoff");
        }
        failure
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WorkUnitSelection {
    ResumeExistingClaim(String),
    ClaimOpenUnit(String),
}

impl WorkUnitSelection {
    fn work_unit_id(&self) -> &str {
        match self {
            Self::ResumeExistingClaim(id) | Self::ClaimOpenUnit(id) => id,
        }
    }
}

/// One pass of the labor loop. Never panics and never propagates: a tick that
/// fails must not take the process down, because the next tick may well succeed
/// once the relay reconnects or GitHub's rate limit resets.
pub async fn tick(
    events: &EventSink,
    state: &P2pState,
    store: &AtpStore,
    local_agent_id: &str,
    config: &AutonomousConfig,
    counters: &mut AutonomousState,
) {
    counters.roll_day(crate::store::now_millis() as i64);
    // 1. Verifier duty.
    match verify_next_pending_headless(events, state, store).await {
        Ok(Some(issued)) => {
            tracing::info!(
                contribution_id = %issued.contribution_id,
                worker = %issued.worker_agent_id,
                credits = issued.credit_total,
                "[CONTRIBUTE] verified peer receipt"
            );
            // Verification is the scarce duty. Having done one, yield the tick
            // so a backlog drains steadily instead of racing local work.
            return;
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(%error, "verification pass failed");
            return;
        }
    }

    if !config.contribute {
        // This used to be a `debug!`, which meant a healthy idle verifier and a
        // node that had silently lost its relay produced identical output at
        // the default log level: nothing at all. Report the connectivity state
        // that distinguishes them, throttled so it stays readable.
        if counters.should_log_idle(crate::store::now_millis()) {
            let (peers, relay_connected, rendezvous_registered) = state
                .inner
                .lock()
                .map(|inner| {
                    (
                        inner.active_peer_links.len(),
                        inner.relay_connected,
                        inner.rendezvous_registered,
                    )
                })
                .unwrap_or((0, false, false));
            tracing::info!(
                connected_peers = peers,
                relay_connected,
                rendezvous_registered,
                "[VERIFIER] idle: nothing pending to verify"
            );
        }
        return;
    }

    if counters.at_daily_cap(config) {
        tracing::info!(
            completed = counters.work_units_today,
            cap = config.max_daily_work_units,
            "[CONTRIBUTE] daily work unit cap reached"
        );
        return;
    }

    // 2. Backpressure. Mirrors the cockpit's selfPending gate.
    match store.pending_contribution_count_for_worker(local_agent_id) {
        Ok(pending) if pending >= MAX_PENDING_CONTRIBUTIONS_PER_WORKER => {
            tracing::info!(
                pending,
                "[CONTRIBUTE] paused: submitted receipts awaiting independent verification"
            );
            return;
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, "could not read worker backpressure");
            return;
        }
    }

    // 3. Claim and run.
    let campaigns = match store.list_protocol_campaigns() {
        Ok(campaigns) => campaigns,
        Err(error) => {
            tracing::warn!(%error, "could not list campaigns");
            return;
        }
    };

    for campaign in campaigns {
        let campaign_id = campaign.campaign_id.clone();
        let Some(selection) = next_open_work_unit(store, &campaign_id, local_agent_id, counters)
        else {
            continue;
        };
        let work_unit_id = selection.work_unit_id().to_string();
        let now_ms = crate::store::now_millis();

        if matches!(selection, WorkUnitSelection::ResumeExistingClaim(_))
            && !counters.retry_ready(&campaign_id, &work_unit_id, now_ms)
        {
            tracing::debug!(
                campaign = %campaign_id,
                work_unit = %work_unit_id,
                "claimed work unit is cooling down after a failed run"
            );
            continue;
        }

        if matches!(selection, WorkUnitSelection::ClaimOpenUnit(_)) {
            counters.reset_claim_attempts(store, &campaign_id, &work_unit_id);
            if let Err(error) =
                claim_work_unit_headless(events, state, store, &campaign_id, &work_unit_id).await
            {
                tracing::debug!(
                    campaign = %campaign_id,
                    work_unit = %work_unit_id,
                    %error,
                    "claim rejected, trying the next campaign"
                );
                continue;
            }
        } else {
            tracing::info!(
                campaign = %campaign_id,
                work_unit = %work_unit_id,
                model = %config.model,
                "[CONTRIBUTE] resuming existing work unit claim"
            );
        }

        tracing::info!(
            campaign = %campaign_id,
            work_unit = %work_unit_id,
            model = %config.model,
            "[CONTRIBUTE] claimed work unit"
        );

        match run_work_unit_headless(
            events,
            state,
            store,
            &campaign_id,
            &work_unit_id,
            &config.provider,
            &config.model,
            config.max_runtime_seconds,
        )
        .await
        {
            Ok(contribution) => {
                counters.clear_failure(store, &campaign_id, &work_unit_id);
                counters.work_units_today += 1;
                tracing::info!(
                    campaign = %campaign_id,
                    work_unit = %work_unit_id,
                    findings = contribution.findings.len(),
                    "[CONTRIBUTE] submitted contribution"
                );
            }
            Err(error) => {
                let failure = counters.record_failure(
                    store,
                    &campaign_id,
                    &work_unit_id,
                    crate::store::now_millis(),
                    &error,
                );
                tracing::warn!(
                    campaign = %campaign_id,
                    work_unit = %work_unit_id,
                    attempt = failure.attempts,
                    max_attempts_per_claim = MAX_RUN_ATTEMPTS_PER_CLAIM,
                    total_attempts = failure.total_attempts,
                    max_total_attempts = MAX_TOTAL_ATTEMPTS_BEFORE_ABANDON,
                    retry_after_ms = failure.retry_after_ms,
                    claim_ttl_ms = WORK_UNIT_CLAIM_TTL_MS,
                    %error,
                    "[CONTRIBUTE] work unit failed"
                );
                if failure.abandoned {
                    tracing::info!(
                        campaign = %campaign_id,
                        work_unit = %work_unit_id,
                        total_attempts = failure.total_attempts,
                        "[CONTRIBUTE] abandoning work unit on this node after repeated failures; \
                         it stays open for other workers"
                    );
                }
            }
        }
        // One unit per tick, so verification keeps getting a turn.
        return;
    }

    tracing::debug!("[CONTRIBUTE] no open work found");
}

/// Find a work unit in this campaign that the node may claim: either one it has
/// already claimed but not yet submitted, or an open one.
fn next_open_work_unit(
    store: &AtpStore,
    campaign_id: &str,
    local_agent_id: &str,
    counters: &AutonomousState,
) -> Option<WorkUnitSelection> {
    let snapshot = store.campaign_report_snapshot(campaign_id).ok()?;

    let has_contribution = |work_unit_id: &str| {
        snapshot.contributions.iter().any(|contribution| {
            contribution.work_unit_id == work_unit_id
                && contribution.worker_agent_id == local_agent_id
        })
    };
    // A unit this node has given up on must not be selected again, or the
    // abandonment is decorative.
    let abandoned = |work_unit_id: &str| counters.is_abandoned(campaign_id, work_unit_id);

    // Resume an existing claim before taking a new one, or the claim expires
    // unfulfilled and the work unit churns.
    if let Some(claim) = snapshot.claims.iter().find(|claim| {
        claim.worker_agent_id == local_agent_id
            && claim.status == "claimed"
            && !has_contribution(&claim.work_unit_id)
            && !abandoned(&claim.work_unit_id)
    }) {
        return Some(WorkUnitSelection::ResumeExistingClaim(
            claim.work_unit_id.clone(),
        ));
    }

    // Scan past abandoned units rather than giving up on the whole campaign:
    // one poisoned unit must not hide the rest of the campaign's work.
    snapshot
        .work_units
        .iter()
        .find(|unit| {
            unit.status == "open"
                && !has_contribution(&unit.work_unit_id)
                && !abandoned(&unit.work_unit_id)
        })
        .map(|unit| WorkUnitSelection::ClaimOpenUnit(unit.work_unit_id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::AtpStore;

    fn config(cap: u32) -> AutonomousConfig {
        AutonomousConfig {
            contribute: true,
            provider: "ollama".to_string(),
            model: "glm-5.2:cloud".to_string(),
            max_runtime_seconds: Some(1800),
            max_daily_work_units: cap,
        }
    }

    fn empty_store() -> AtpStore {
        AtpStore::in_memory_for_tests()
    }

    #[test]
    fn the_idle_heartbeat_is_throttled_not_per_tick() {
        // The tick runs every 12s. Logging idleness every tick would bury the
        // events an operator actually needs to see.
        let mut counters = AutonomousState::default();
        assert!(counters.should_log_idle(0), "first idle tick reports");
        assert!(!counters.should_log_idle(12_000), "next tick stays quiet");
        assert!(
            !counters.should_log_idle(VERIFIER_IDLE_LOG_INTERVAL_MS - 1),
            "still quiet just before the interval"
        );
        assert!(
            counters.should_log_idle(VERIFIER_IDLE_LOG_INTERVAL_MS),
            "reports again once the interval elapses"
        );
        assert!(
            !counters.should_log_idle(VERIFIER_IDLE_LOG_INTERVAL_MS + 12_000),
            "and the throttle re-arms from the last report"
        );
    }

    #[test]
    fn a_reclaim_resets_per_claim_attempts_but_not_the_lifetime_total() {
        // The regression: clearing all failure state on re-claim made the
        // attempt budget unbounded, so a work unit whose claim kept expiring
        // was retried forever.
        let store = empty_store();
        let mut counters = AutonomousState::default();
        for _ in 0..MAX_RUN_ATTEMPTS_PER_CLAIM {
            counters.record_failure(&store, "c1", "w1", 0, "boom");
        }
        counters.reset_claim_attempts(&store, "c1", "w1");
        let key = AutonomousState::failure_key("c1", "w1");
        let failure = counters.failed_work_units.get(&key).copied().unwrap();
        assert_eq!(failure.attempts, 0, "per-claim counter resets");
        assert_eq!(
            failure.total_attempts, MAX_RUN_ATTEMPTS_PER_CLAIM as u32,
            "lifetime total survives the re-claim"
        );
        assert!(!failure.abandoned);
    }

    #[test]
    fn a_work_unit_is_abandoned_once_the_lifetime_budget_is_spent() {
        let store = empty_store();
        let mut counters = AutonomousState::default();
        for attempt in 1..MAX_TOTAL_ATTEMPTS_BEFORE_ABANDON {
            let failure = counters.record_failure(&store, "c1", "w1", 0, "boom");
            assert!(!failure.abandoned, "not abandoned at attempt {attempt}");
            assert!(!counters.is_abandoned("c1", "w1"));
            // A re-claim between attempts must not extend the budget.
            counters.reset_claim_attempts(&store, "c1", "w1");
        }
        let failure = counters.record_failure(&store, "c1", "w1", 0, "boom");
        assert!(failure.abandoned);
        assert_eq!(failure.total_attempts, MAX_TOTAL_ATTEMPTS_BEFORE_ABANDON);
        assert!(counters.is_abandoned("c1", "w1"));
        // Abandonment is per work unit, never campaign-wide.
        assert!(!counters.is_abandoned("c1", "w2"));
    }

    #[test]
    fn backoff_and_abandonment_survive_a_restart() {
        // The reported bug: closing the app dropped every give-up decision, so
        // a poisoned work unit came straight back after a restart.
        let store = empty_store();
        let mut counters = AutonomousState::default();
        for _ in 0..MAX_TOTAL_ATTEMPTS_BEFORE_ABANDON {
            counters.record_failure(&store, "c1", "w1", 1_000, "boom");
            counters.reset_claim_attempts(&store, "c1", "w1");
        }
        assert!(counters.is_abandoned("c1", "w1"));

        let mut restarted = AutonomousState::default();
        assert!(
            !restarted.is_abandoned("c1", "w1"),
            "a fresh process starts empty"
        );
        restarted.hydrate(&store);
        assert!(
            restarted.is_abandoned("c1", "w1"),
            "hydrate restores the give-up decision"
        );
        assert!(
            !restarted.retry_ready("c1", "w1", 0),
            "backoff also restored"
        );
    }

    #[test]
    fn a_successful_run_clears_the_record_everywhere() {
        let store = empty_store();
        let mut counters = AutonomousState::default();
        counters.record_failure(&store, "c1", "w1", 0, "boom");
        counters.clear_failure(&store, "c1", "w1");
        assert!(counters.retry_ready("c1", "w1", 0));
        assert!(!counters.is_abandoned("c1", "w1"));

        let mut restarted = AutonomousState::default();
        restarted.hydrate(&store);
        assert!(
            restarted.retry_ready("c1", "w1", 0),
            "the row is gone from the store too, not just memory"
        );
    }

    #[test]
    fn daily_cap_blocks_only_after_the_cap_is_reached() {
        let mut counters = AutonomousState::default();
        counters.roll_day(0);
        assert!(!counters.at_daily_cap(&config(2)));
        counters.work_units_today = 1;
        assert!(!counters.at_daily_cap(&config(2)));
        counters.work_units_today = 2;
        assert!(counters.at_daily_cap(&config(2)));
    }

    #[test]
    fn a_zero_cap_means_unlimited_not_stopped() {
        let mut counters = AutonomousState::default();
        counters.roll_day(0);
        counters.work_units_today = 10_000;
        assert!(!counters.at_daily_cap(&config(0)));
    }

    #[test]
    fn the_tally_rolls_at_utc_midnight() {
        let mut counters = AutonomousState::default();
        let day_one = 1_784_000_000_000i64;
        counters.roll_day(day_one);
        counters.work_units_today = 500;
        assert!(counters.at_daily_cap(&config(500)));

        // Same day, still capped.
        counters.roll_day(day_one + 3_600_000);
        assert_eq!(counters.work_units_today, 500);

        // Next UTC day, tally clears.
        counters.roll_day(day_one + 86_400_000);
        assert_eq!(counters.work_units_today, 0);
        assert!(!counters.at_daily_cap(&config(500)));
    }

    #[test]
    fn failed_work_units_cool_down_and_stop_after_two_runs_per_claim() {
        let store = empty_store();
        let mut counters = AutonomousState::default();
        let first = counters.record_failure(&store, "campaign", "unit", 1_000, "boom");
        assert_eq!(first.attempts, 1);
        assert_eq!(first.retry_after_ms, 1_000 + WORK_UNIT_RETRY_DELAY_MS);
        assert!(!counters.retry_ready("campaign", "unit", first.retry_after_ms - 1));
        assert!(counters.retry_ready("campaign", "unit", first.retry_after_ms));

        let second =
            counters.record_failure(&store, "campaign", "unit", first.retry_after_ms, "boom");
        assert_eq!(second.attempts, 2);
        assert_eq!(
            second.retry_after_ms,
            first.retry_after_ms + WORK_UNIT_CLAIM_TTL_MS
        );
        assert!(!counters.retry_ready("campaign", "unit", second.retry_after_ms - 1));
    }
}
