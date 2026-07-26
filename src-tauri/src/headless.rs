//! Headless worker entry point.
//!
//! CYPHES is a Tauri desktop app, and until now *every* node required a rendered
//! webview to do any work at all: the labor loop lived in `App.tsx` on a
//! `setInterval`, and `start_node` was only ever invoked from the frontend. A
//! node on WSL2, a server, or any host without a display would start the GTK
//! shell, fail to composite a webview, and then sit at idle forever — process
//! alive, relay unconnected, no work claimed.
//!
//! This module is the path that does not need a display. It never constructs a
//! Tauri runtime (doing so initialises GTK, which is exactly what fails), and
//! instead builds the same `AtpStore` and `P2pState` the GUI manages, starts the
//! same swarm, and drives the same tick.
//!
//! Configuration is read from the environment. The variable names match the ones
//! headless operators are already using in the field, so an existing systemd
//! unit or shell profile keeps working without edits.

use std::time::Duration;

use tokio::sync::mpsc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::{
    atp::agent_id,
    autonomous::{self, AutonomousConfig, AutonomousState},
    events::EventSink,
    p2p::{load_or_create_identity, spawn_swarm},
    state::P2pState,
    store::AtpStore,
};

/// Interval between autonomous ticks. Matches the cockpit's
/// `AUTO_TICK_INTERVAL_MS` so headless and GUI nodes behave identically.
const TICK_INTERVAL: Duration = Duration::from_secs(12);

/// Returns true when the process was asked to run without a UI, by either
/// `CYPHES_HEADLESS=1` or a `--headless` argument.
pub fn requested() -> bool {
    truthy("CYPHES_HEADLESS") || std::env::args().any(|argument| argument == "--headless")
}

fn truthy(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Headless runtime settings.
///
/// The GUI keeps the equivalent state in `window.localStorage`, which a headless
/// process cannot reach, so these come from the environment instead.
#[derive(Debug, Clone)]
pub struct HeadlessConfig {
    /// Claim and run work units. When false the node participates as a
    /// verifier only, which is still useful: network settlement stalls when no
    /// independent verifier is online.
    pub contribute: bool,
    /// Keep ticking. When false a single tick runs and the process exits, which
    /// makes the mode usable from cron and from tests.
    pub loop_forever: bool,
    pub provider: String,
    pub model: Option<String>,
    pub max_runtime_seconds: Option<u64>,
    pub max_daily_work_units: u32,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            contribute: false,
            loop_forever: true,
            provider: "ollama".to_string(),
            model: None,
            max_runtime_seconds: Some(30 * 60),
            max_daily_work_units: 500,
        }
    }
}

impl HeadlessConfig {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            contribute: truthy("CYPHES_CONTRIBUTE"),
            loop_forever: std::env::var("CYPHES_CONTRIBUTE_LOOP")
                .ok()
                .map(|_| truthy("CYPHES_CONTRIBUTE_LOOP"))
                .unwrap_or(defaults.loop_forever),
            provider: env_string("CYPHES_CONTRIBUTE_PROVIDER").unwrap_or(defaults.provider),
            model: env_string("CYPHES_CONTRIBUTE_MODEL"),
            max_runtime_seconds: env_string("CYPHES_CONTRIBUTE_MAX_RUNTIME_SECONDS")
                .and_then(|value| value.parse().ok())
                .or(defaults.max_runtime_seconds),
            max_daily_work_units: env_string("CYPHES_CONTRIBUTE_MAX_DAILY_UNITS")
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.max_daily_work_units),
        }
    }

    fn as_autonomous(&self) -> AutonomousConfig {
        AutonomousConfig {
            contribute: self.contribute && self.model.is_some(),
            provider: self.provider.clone(),
            model: self.model.clone().unwrap_or_default(),
            max_runtime_seconds: self.max_runtime_seconds,
            max_daily_work_units: self.max_daily_work_units,
        }
    }
}

/// Install a tracing subscriber honouring `RUST_LOG`.
///
/// Until now the app had no logging framework at all, so `RUST_LOG` was inert
/// and a headless operator had no way to observe the node. Defaults to `info`
/// for this crate when `RUST_LOG` is unset.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("cyphes_desktop_lib=info,cyphes_desktop=info"));
    // stderr, not stdout: stdout is block-buffered when redirected to a file or
    // captured by a supervisor, which swallows the log of a long-running daemon
    // until it exits. Operators need to see [HEADLESS] node started immediately.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true).with_writer(std::io::stderr))
        .try_init();
}

/// Run the node with no window and no Tauri runtime.
pub fn run() -> Result<(), String> {
    init_tracing();
    let config = HeadlessConfig::from_env();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to build async runtime: {error}"))?;

    runtime.block_on(async move {
        let store = AtpStore::open_default()?;
        let state = P2pState::default();
        let events = EventSink::headless();

        let keypair = load_or_create_identity()?;
        let local_agent_id = agent_id(&keypair.public());
        let (tx, rx) = mpsc::unbounded_channel();
        let (peer_id, listen_addrs) = spawn_swarm(
            events.clone(),
            state.clone(),
            store.clone(),
            keypair.clone(),
            rx,
        )
        .await?;

        {
            let mut inner = state.inner.lock().map_err(|error| error.to_string())?;
            inner.started = true;
            inner.local_peer_id = Some(peer_id.clone());
            inner.keypair = Some(keypair);
            inner.sender = Some(tx);
            inner.listen_addrs = listen_addrs.clone();
        }

        // The line headless operators watch for.
        tracing::info!(
            peer_id = %peer_id,
            agent_id = %local_agent_id,
            contribute = config.contribute,
            provider = %config.provider,
            model = config.model.as_deref().unwrap_or("(verifier only)"),
            "[HEADLESS] node started"
        );
        if config.contribute && config.model.is_none() {
            tracing::warn!(
                "CYPHES_CONTRIBUTE is set but CYPHES_CONTRIBUTE_MODEL is empty; running as verifier only"
            );
        }

        let autonomous_config = config.as_autonomous();
        let mut counters = AutonomousState::default();
        if !config.loop_forever {
            autonomous::tick(
                &events,
                &state,
                &store,
                &local_agent_id,
                &autonomous_config,
                &mut counters,
            )
            .await;
            return Ok(());
        }

        let mut ticker = tokio::time::interval(TICK_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    autonomous::tick(
                        &events,
                        &state,
                        &store,
                        &local_agent_id,
                        &autonomous_config,
                        &mut counters,
                    )
                    .await;
                }
                _ = shutdown_signal() => {
                    tracing::info!("[HEADLESS] shutdown signal received");
                    break;
                }
            }
        }
        Ok(())
    })
}

/// Resolve when the supervisor asks the process to stop, so systemd gets a
/// clean exit instead of a kill.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(stream) => stream,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_accepts_the_forms_operators_actually_write() {
        for value in ["1", "true", "TRUE", "yes", "on", " true "] {
            std::env::set_var("CYPHES_TEST_TRUTHY", value);
            assert!(truthy("CYPHES_TEST_TRUTHY"), "{value} should be truthy");
        }
        for value in ["0", "false", "no", "", "off"] {
            std::env::set_var("CYPHES_TEST_TRUTHY", value);
            assert!(!truthy("CYPHES_TEST_TRUTHY"), "{value} should be falsy");
        }
        std::env::remove_var("CYPHES_TEST_TRUTHY");
    }

    #[test]
    fn contribute_without_a_model_degrades_to_verifier_only() {
        // Claiming work with no model would burn claims the node cannot fulfil
        // and strand receipts, so the loop must refuse rather than try.
        let config = HeadlessConfig {
            contribute: true,
            model: None,
            ..HeadlessConfig::default()
        };
        assert!(!config.as_autonomous().contribute);

        let config = HeadlessConfig {
            contribute: true,
            model: Some("glm-5.2:cloud".to_string()),
            ..HeadlessConfig::default()
        };
        assert!(config.as_autonomous().contribute);
    }

    #[test]
    fn defaults_are_verifier_only_and_looping() {
        let config = HeadlessConfig::default();
        assert!(
            !config.contribute,
            "must not spend inference without being asked"
        );
        assert!(config.loop_forever);
    }
}
