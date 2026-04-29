//! Startup timing helpers for the native demo shell.
//!
//! The native host creates the OS window hidden, performs GL setup, builds the
//! shared demo UI, paints the first frame, then reveals the window.  These
//! timings make a slow launch visible in the `cargo dev` terminal without
//! pulling measurement concerns into the shared widget/demo crates.

use std::cell::Cell;
use std::time::Instant;

pub struct StartupTiming {
    started: Instant,
    last: Cell<Instant>,
}

impl StartupTiming {
    pub fn new() -> Self {
        let now = Instant::now();
        eprintln!("[agg-gui native startup] start");
        Self {
            started: now,
            last: Cell::new(now),
        }
    }

    pub fn mark(&self, label: &str) {
        let now = Instant::now();
        let phase = now.duration_since(self.last.get()).as_secs_f64() * 1000.0;
        let total = now.duration_since(self.started).as_secs_f64() * 1000.0;
        self.last.set(now);
        eprintln!("[agg-gui native startup] {label}: {total:.1} ms (+{phase:.1} ms)");
    }
}
