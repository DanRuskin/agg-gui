//! Startup timing helpers for the WASM demo shell.
//!
//! Browser startup has useful timing boundaries on both sides of the
//! JS/WASM boundary.  This module keeps the Rust-side console logging and
//! first-frame counters out of `lib.rs`, which already owns the platform
//! lifecycle and event exports.

use std::cell::Cell;

thread_local! {
    static RENDER_COUNT: Cell<u32> = Cell::new(0);
}

pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

pub fn log(label: &str, elapsed_ms: f64) {
    web_sys::console::info_1(
        &format!("[agg-gui wasm] {label}: {elapsed_ms:.1} ms").into(),
    );
}

pub fn next_render_index() -> u32 {
    RENDER_COUNT.with(|c| {
        let current = c.get();
        c.set(current.saturating_add(1));
        current
    })
}
