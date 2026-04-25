//! Layer-1 behaviour tests for the six Window Resize Test sub-windows.
//!
//! Each `#[test]` validates one specific behaviour that egui's
//! `window_resize_test.rs` demo is designed to demonstrate (see
//! `C:/Development/rust-apps/agg-gui/egui-reference/crates/egui_demo_lib/src/demo/tests/window_resize_test.rs`
//! for the source).  The tests drive a real `App` instance hosting the
//! relevant sub-window, synthesise mouse events, and assert on the
//! **measurable geometry** (outer window bounds, inner content bounds,
//! sub-widget bounds, ScrollView scroll offset, …) at the end of the
//! event sequence.
//!
//! Where agg-gui's current behaviour differs from egui's (e.g. a feature
//! planned for a later stage of the port), the test either marks the
//! shortfall with `#[ignore]` or asserts the *current* behaviour and
//! carries a comment flagging which stage will tighten the assertion.
//! That way the passing tests prove forward progress without hiding
//! known gaps.
//!
//! Coordinate-system note: `App`'s public event entry points accept
//! **physical-pixel Y-DOWN screen coordinates** (matching the contract
//! native and web hosts feed them).  Everything inside agg-gui is
//! Y-up.  The `drag` helper converts from Y-down to the widget-facing
//! Y-up reliably by letting `App::flip_y` do the work.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::{
    find_widget_by_id, find_widget_by_type, App, Event, FlexColumn, Font, Key, Modifiers,
    MouseButton, Point, Rect, Resize, Size, Stack, TextArea, Widget, Window,
};
use demo_ui::{window_resize_sub_windows, ResizeTestWindow};

// Canvas large enough that none of the initial sub-window rects get
// clipped to `MIN_W` / `MIN_H`; matches the actual demo's default
// 1280×720 layout so geometry lands in the same absolute coordinates.
const CANVAS_W: f64 = 1280.0;
const CANVAS_H: f64 = 720.0;

fn font() -> Arc<Font> {
    // Tests compile a fresh Font per invocation — cheap (TTF parse
    // handled by `ttf-parser`, no glyph rasterisation up front).
    const BYTES: &[u8] = include_bytes!("../../demo/assets/CascadiaCode.ttf");
    Arc::new(Font::from_slice(BYTES).expect("parse CascadiaCode.ttf"))
}

// ─── Test-setup helpers ──────────────────────────────────────────────────────

/// Build an `App` hosting exactly one of the six Window Resize Test
/// sub-windows, identified by the egui-source-order `index` (0 = auto-
/// sized, 1 = resizable + scroll, 2 = resizable + embedded scroll,
/// 3 = resizable without scroll, 4 = resizable with TextEdit,
/// 5 = freely resized).  Returns the App, the window title, and the
/// shared position cell that publishes current bounds each layout.
fn make_test_app(index: usize) -> (App, String, Rc<Cell<Rect>>) {
    let entries: Vec<ResizeTestWindow> = window_resize_sub_windows(font());
    let entry = entries
        .into_iter()
        .nth(index)
        .expect("index within the six sub-windows");
    let title = entry.title.clone();
    let pos_cell = Rc::new(Cell::new(entry.initial_rect));
    let visible = Rc::new(Cell::new(true));
    let mut win = Window::new(&title, font(), entry.content)
        .with_bounds(entry.initial_rect)
        .with_visible_cell(Rc::clone(&visible))
        .with_position_cell(Rc::clone(&pos_cell));
    // Match the application order used by `lib.rs::build_demo_ui`:
    // `with_vscroll` mutates children so it must precede any builder
    // that reads them.
    if entry.vscroll {
        win = win.with_vscroll(true);
    }
    if entry.auto_size {
        win = win.with_auto_size(true);
    } else {
        win = win.with_resizable_axes(entry.resizable_h, entry.resizable_v);
        if !entry.resizable {
            win = win.with_resizable(false);
        }
    }
    if entry.tight_fit {
        win = win.with_tight_content_fit(true);
    }
    if entry.floor_fit {
        win = win.with_height_floor_to_content(true);
    }
    let root = Stack::new().add(Box::new(win));
    let mut app = App::new(Box::new(root));
    app.layout(Size::new(CANVAS_W, CANVAS_H));
    (app, title, pos_cell)
}

/// Feed a full press / move / release drag through the App at the
/// given Y-DOWN screen coordinates.  Relayouts at the end so the next
/// assertion sees fully-propagated bounds (position cells are written
/// during `layout`).
fn drag(app: &mut App, start: (f64, f64), end: (f64, f64)) {
    app.on_mouse_move(start.0, start.1);
    app.on_mouse_down(start.0, start.1, MouseButton::Left, Modifiers::default());
    app.on_mouse_move(end.0, end.1);
    app.on_mouse_up(end.0, end.1, MouseButton::Left, Modifiers::default());
    app.layout(Size::new(CANVAS_W, CANVAS_H));
}

fn window_bounds(app: &App, title: &str) -> Rect {
    find_widget_by_id(app.root(), title)
        .expect("test window is in the tree")
        .bounds()
}

/// Convert a Y-up world coordinate to the Y-down screen coord an
/// `App` entry point expects.  Centralised so individual tests stay
/// readable: they compute where the edge *is* in widget terms, then
/// pass through `to_screen` instead of inlining the arithmetic.
fn to_screen(y_up: f64) -> f64 {
    CANVAS_H - y_up
}

#[path = "window_resize/w1.rs"]
mod w1;
#[path = "window_resize/w2_w4.rs"]
mod w2_w4;
#[path = "window_resize/w5_and_flags.rs"]
mod w5_and_flags;
