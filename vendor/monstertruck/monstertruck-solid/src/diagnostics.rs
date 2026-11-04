//! Opt-in instrumentation for the classic boolean pipeline.
//!
//! PATCH(BrepSCAD): all output is gated by environment variables so the normal
//! render path has no timing or formatting overhead. Set
//! `BREPSCAD_DEBUG_BOOLEAN_TIMING=1` for stage timings and
//! `BREPSCAD_DEBUG_SEW=1` for per-call sew counters.

use std::sync::OnceLock;
use std::time::Instant;

fn env_flag(name: &str) -> bool { std::env::var_os(name).is_some() }

/// Whether classic boolean stage timings are enabled.
pub(crate) fn boolean_timing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag("BREPSCAD_DEBUG_BOOLEAN_TIMING"))
}

/// Whether coplanar sew state dumps are enabled.
pub(crate) fn sew_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag("BREPSCAD_DEBUG_SEW"))
}

/// A small stage timer that is completely dormant unless explicitly enabled.
pub(crate) struct StageTimer {
    start: Option<Instant>,
}

impl StageTimer {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            start: enabled.then(Instant::now),
        }
    }

    pub(crate) fn enabled(&self) -> bool { self.start.is_some() }

    pub(crate) fn mark(&mut self, stage: &str) {
        if let Some(start) = self.start {
            let millis = start.elapsed().as_secs_f64() * 1000.0;
            eprintln!("[boolean] {stage}: {millis:.3}ms");
            self.start = Some(Instant::now());
        }
    }
}
