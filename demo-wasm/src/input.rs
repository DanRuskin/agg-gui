//! Wasm-bindgen input bindings — mouse, touch, keyboard, DPR, draw-need
//! polling.  Split out of `lib.rs` so the crate root stays under the
//! 800-line guardrail.
//!
//! Every `#[wasm_bindgen] pub fn` here is callable from JS by name
//! (see `demo/src/app.ts`).  They translate browser events into the
//! agg-gui input vocabulary and forward them to the singleton
//! [`DEMO_APP`].  As a submodule of the crate root, this file accesses
//! the thread-local state directly via `use crate::*`.

use agg_gui::{Key, Modifiers, MouseButton};
use wasm_bindgen::prelude::*;

use crate::{mark_dirty, DEMO_APP, MOUSE_BUTTONS_DOWN, NEEDS_DRAW, RUN_MODE};

#[wasm_bindgen]
pub fn set_device_pixel_ratio(dpr: f64) {
    agg_gui::set_device_scale(dpr.max(0.5));
    mark_dirty();
}

#[wasm_bindgen]
pub fn on_mouse_move(x: f64, y: f64) {
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.on_mouse_move(x, y);
        }
    });
    if let Some(window) = web_sys::window() {
        if let Some(doc) = window.document() {
            if let Some(el) = doc.get_element_by_id("canvas") {
                let style = agg_gui::web_adapter::cursor_style(agg_gui::current_cursor_icon());
                let _ = el.set_attribute("style", &style);
            }
        }
    }
}

#[wasm_bindgen]
pub fn on_mouse_down(x: f64, y: f64, button: u8) {
    MOUSE_BUTTONS_DOWN.set(MOUSE_BUTTONS_DOWN.get().saturating_add(1));
    let btn = match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        n => MouseButton::Other(n),
    };
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.on_mouse_down(x, y, btn, Modifiers::default());
        }
    });
}

#[wasm_bindgen]
pub fn on_mouse_up(x: f64, y: f64, button: u8) {
    MOUSE_BUTTONS_DOWN.set(MOUSE_BUTTONS_DOWN.get().saturating_sub(1));
    let btn = match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        n => MouseButton::Other(n),
    };
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.on_mouse_up(x, y, btn, Modifiers::default());
        }
    });
}

#[wasm_bindgen]
pub fn on_mouse_wheel(x: f64, y: f64, delta_y: f64) {
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.on_mouse_wheel(x, y, delta_y);
        }
    });
}

#[wasm_bindgen]
pub fn on_mouse_leave() {
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.on_mouse_leave();
        }
    });
}

#[wasm_bindgen]
pub fn on_touch_start(id: u32, x: f64, y: f64, force: f64) {
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            let f = if force > 0.0 {
                Some(force as f32)
            } else {
                None
            };
            app.on_touch_start(
                agg_gui::TouchDeviceId(0),
                agg_gui::TouchId(id as u64),
                x,
                y,
                f,
            );
        }
    });
    mark_dirty();
}

#[wasm_bindgen]
pub fn on_touch_move(id: u32, x: f64, y: f64, force: f64) {
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            let f = if force > 0.0 {
                Some(force as f32)
            } else {
                None
            };
            app.on_touch_move(
                agg_gui::TouchDeviceId(0),
                agg_gui::TouchId(id as u64),
                x,
                y,
                f,
            );
        }
    });
    mark_dirty();
}

#[wasm_bindgen]
pub fn on_touch_end(id: u32) {
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.on_touch_end(agg_gui::TouchDeviceId(0), agg_gui::TouchId(id as u64));
        }
    });
    mark_dirty();
}

#[wasm_bindgen]
pub fn on_touch_cancel(id: u32) {
    DEMO_APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.on_touch_cancel(agg_gui::TouchDeviceId(0), agg_gui::TouchId(id as u64));
        }
    });
    mark_dirty();
}

#[wasm_bindgen]
pub fn on_key_down(key_str: &str, shift: bool, ctrl: bool, alt: bool, meta: bool) {
    if let Some(key) = parse_js_key(key_str) {
        let mods = Modifiers {
            shift,
            ctrl,
            alt,
            meta,
        };
        DEMO_APP.with(|cell| {
            if let Some(app) = cell.borrow_mut().as_mut() {
                app.on_key_down(key, mods);
            }
        });
    }
}

#[wasm_bindgen]
pub fn needs_draw() -> bool {
    let continuous = RUN_MODE.with(|c| {
        c.borrow()
            .as_ref()
            .map(|rc| rc.get() == demo_ui::RunMode::Continuous)
            .unwrap_or(false)
    });
    if continuous {
        return true;
    }
    if NEEDS_DRAW.with(|c| c.get()) {
        return true;
    }
    let want = DEMO_APP.with(|c| c.borrow().as_ref().map(|a| a.wants_draw()).unwrap_or(false));
    want
}

fn parse_js_key(key: &str) -> Option<Key> {
    agg_gui::web_adapter::key(key)
}
