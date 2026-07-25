//! UI event sink.
//!
//! Every `app.emit(...)` in this codebase is a fire-and-forget notification to
//! the cockpit. In headless mode there is no webview to notify, and — more
//! importantly — there is no Tauri runtime at all, because initialising one
//! requires a display that a WSL2 or server node does not have.
//!
//! `EventSink` is the seam. It exposes the same `emit(event, payload)` shape as
//! `tauri::Emitter`, so call sites are unchanged; only the type in the signature
//! moves from `&AppHandle` to `&EventSink`. In GUI mode it forwards to the real
//! handle. In headless mode it records the event to the tracing log, which is
//! how a headless operator observes the node at all.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Where UI events go. Cheap to clone; the Tauri variant clones an `AppHandle`,
/// which is itself a handle rather than the app.
#[derive(Clone, Default)]
pub struct EventSink {
    handle: Option<AppHandle>,
}

impl EventSink {
    /// Forward events to the cockpit webview.
    pub fn tauri(app: AppHandle) -> Self {
        Self { handle: Some(app) }
    }

    /// Drop events into the log. Used by the headless worker, which has no
    /// webview and no Tauri runtime.
    pub fn headless() -> Self {
        Self { handle: None }
    }

    /// Mirrors `tauri::Emitter::emit` so existing call sites need no edit.
    ///
    /// A failed UI notification must never fail the labor operation that
    /// produced it; every caller already discards the result with `let _ =`.
    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), tauri::Error> {
        match &self.handle {
            Some(app) => app.emit(event, payload),
            None => {
                tracing::debug!(event, "ui event (headless)");
                Ok(())
            }
        }
    }

    /// True when this sink is backed by a real cockpit.
    pub fn is_interactive(&self) -> bool {
        self.handle.is_some()
    }
}

impl std::fmt::Debug for EventSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventSink")
            .field("interactive", &self.is_interactive())
            .finish()
    }
}
