//! WASM clipboard and text-input focus exports.
//!
//! The JS harness reads/writes an in-process clipboard buffer to connect
//! Rust's copy/cut/paste logic to browser clipboard events.  It also asks
//! whether text input is focused so mobile browsers can show their software
//! keyboard through a hidden DOM textarea.

use wasm_bindgen::prelude::*;

/// Read the in-process clipboard buffer. Returns `None` when empty.
#[wasm_bindgen]
pub fn wasm_clipboard_get() -> Option<String> {
    agg_gui::wasm_clipboard::get()
}

/// Read the in-process HTML clipboard buffer. Returns `None` when empty.
#[wasm_bindgen]
pub fn wasm_clipboard_get_html() -> Option<String> {
    agg_gui::wasm_clipboard::get_html()
}

/// Write `text` into the in-process clipboard buffer.
#[wasm_bindgen]
pub fn wasm_clipboard_set(text: &str) {
    agg_gui::wasm_clipboard::set(text);
}

/// True when the focused widget is an editable text control.
#[wasm_bindgen]
pub fn text_input_focused() -> bool {
    crate::DEMO_APP.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|app| app.focused_widget_type_name())
            .map(|name| matches!(name, "TextField" | "TextArea"))
            .unwrap_or(false)
    })
}
