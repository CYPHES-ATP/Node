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
}

#[derive(Debug, Clone, Copy)]
struct WorkUnitFailure {
    attempts: u8,
    retry_after_ms: u64,
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

    fn clear_failure(&mut self, campaign_id: &str, work_unit_id: &str) {
        self.failed_work_units
            .remove(&Self::failure_key(campaign_id, work_unit_id));
    }

    fn record_failure(
        &mut self,
        campaign_id: &str,
        work_unit_id: &str,
        now_ms: u64,
    ) -> WorkUnitFailure {
        let key = Self::failure_key(campaign_id, work_unit_id);
        let attempts = self
            .failed_work_units
            .get(&key)
            .map_or(1, |failure| failure.attempts.saturating_add(1));
        let delay_ms = if attempts >= MAX_RUN_ATTEMPTS_PER_CLAIM {
            WORK_UNIT_CLAIM_TTL_MS
        } else {
            WORK_UNIT_RETRY_DELAY_MS
        };
        let failure = WorkUnitFailure {
            attempts,
            retry_after_ms: now_ms.saturating_add(delay_ms),
        };
        self.failed_work_units.insert(key, failure);
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
        tracing::debug!("[CONTRIBUTE] verifier-only tick, nothing to verify");
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
        let Some(selection) = next_open_work_unit(store, &campaign_id, local_agent_id) else {
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
            counters.clear_failure(&campaign_id, &work_unit_id);
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
                counters.clear_failure(&campaign_id, &work_unit_id);
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
                    &campaign_id,
                    &work_unit_id,
                    crate::store::now_millis(),
                );
                tracing::warn!(
                    campaign = %campaign_id,
                    work_unit = %work_unit_id,
                    attempt = failure.attempts,
                    max_attempts_per_claim = MAX_RUN_ATTEMPTS_PER_CLAIM,
                    retry_after_ms = failure.retry_after_ms,
                    claim_ttl_ms = WORK_UNIT_CLAIM_TTL_MS,
                    %error,
                    "[CONTRIBUTE] work unit failed"
                );
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
) -> Option<WorkUnitSelection> {
    let snapshot = store.campaign_report_snapshot(campaign_id).ok()?;

    let has_contribution = |work_unit_id: &str| {
        snapshot.contributions.iter().any(|contribution| {
            contribution.work_unit_id == work_unit_id
                && contribution.worker_agent_id == local_agent_id
        })
    };

    // Resume an existing claim before taking a new one, or the claim expires
    // unfulfilled and the work unit churns.
    if let Some(claim) = snapshot.claims.iter().find(|claim| {
        claim.worker_agent_id == local_agent_id
            && claim.status == "claimed"
            && !has_contribution(&claim.work_unit_id)
    }) {
        return Some(WorkUnitSelection::ResumeExistingClaim(
            claim.work_unit_id.clone(),
        ));
    }

    snapshot
        .work_units
        .iter()
        .find(|unit| unit.status == "open" && !has_contribution(&unit.work_unit_id))
        .map(|unit| WorkUnitSelection::ClaimOpenUnit(unit.work_unit_id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(cap: u32) -> AutonomousConfig {
        AutonomousConfig {
            contribute: true,
            provider: "ollama".to_string(),
            model: "glm-5.2:cloud".to_string(),
            max_runtime_seconds: Some(1800),
            max_daily_work_units: cap,
        }
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
        let mut counters = AutonomousState::default();
        let first = counters.record_failure("campaign", "unit", 1_000);
        assert_eq!(first.attempts, 1);
        assert_eq!(first.retry_after_ms, 1_000 + WORK_UNIT_RETRY_DELAY_MS);
        assert!(!counters.retry_ready("campaign", "unit", first.retry_after_ms - 1));
        assert!(counters.retry_ready("campaign", "unit", first.retry_after_ms));

        let second = counters.record_failure("campaign", "unit", first.retry_after_ms);
        assert_eq!(second.attempts, 2);
        assert_eq!(
            second.retry_after_ms,
            first.retry_after_ms + WORK_UNIT_CLAIM_TTL_MS
        );
        assert!(!counters.retry_ready("campaign", "unit", second.retry_after_ms - 1));
    }

    #[test]
    fn a_newly_open_unit_clears_prior_failure_state() {
        let mut counters = AutonomousState::default();
        counters.record_failure("campaign", "unit", 1_000);
        counters.clear_failure("campaign", "unit");
        assert!(counters.retry_ready("campaign", "unit", 1_000));
    }
}
