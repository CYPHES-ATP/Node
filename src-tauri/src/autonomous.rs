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

use crate::{
    commands::{claim_work_unit_headless, run_work_unit_headless, verify_next_pending_headless},
    events::EventSink,
    state::P2pState,
    store::{AtpStore, MAX_PENDING_CONTRIBUTIONS_PER_WORKER},
};

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
        let Some(work_unit_id) = next_open_work_unit(store, &campaign_id, local_agent_id) else {
            continue;
        };

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
                counters.work_units_today += 1;
                tracing::info!(
                    campaign = %campaign_id,
                    work_unit = %work_unit_id,
                    findings = contribution.findings.len(),
                    "[CONTRIBUTE] submitted contribution"
                );
            }
            Err(error) => {
                tracing::warn!(
                    campaign = %campaign_id,
                    work_unit = %work_unit_id,
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
) -> Option<String> {
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
        return Some(claim.work_unit_id.clone());
    }

    snapshot
        .work_units
        .iter()
        .find(|unit| unit.status == "open" && !has_contribution(&unit.work_unit_id))
        .map(|unit| unit.work_unit_id.clone())
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
}
