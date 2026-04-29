//! Startup timing helpers for shared demo UI construction.
//!
//! The native and WASM shells both delegate widget-tree construction to
//! `build_demo_ui`.  Keeping the timing helper here lets us measure which
//! shared window/content builder owns startup regressions without mixing these
//! diagnostics into individual widgets.

use std::cell::Cell;
use std::time::Instant;

pub struct StartupTiming {
    prefix: &'static str,
    started: Instant,
    last: Cell<Instant>,
}

impl StartupTiming {
    pub fn new(prefix: &'static str) -> Self {
        let now = Instant::now();
        eprintln!("{prefix} start");
        Self {
            prefix,
            started: now,
            last: Cell::new(now),
        }
    }

    pub fn mark(&self, label: &str) {
        let now = Instant::now();
        let phase = now.duration_since(self.last.get()).as_secs_f64() * 1000.0;
        let total = now.duration_since(self.started).as_secs_f64() * 1000.0;
        self.last.set(now);
        eprintln!("{} {label}: {total:.1} ms (+{phase:.1} ms)", self.prefix);
    }

    pub fn mark_if_slow(&self, label: &str, started: Instant, threshold_ms: f64) {
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        if elapsed >= threshold_ms {
            eprintln!("{} {label}: +{elapsed:.1} ms", self.prefix);
        }
    }
}
