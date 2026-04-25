#[cfg(feature = "clipboard")]
pub(super) fn clipboard_get() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}
/// Native non-clipboard build: silently no-ops (clipboard disabled at compile time).
#[cfg(all(not(feature = "clipboard"), not(target_arch = "wasm32")))]
pub(super) fn clipboard_get() -> Option<String> {
    None
}
/// WASM build: read from the in-process buffer bridged by the JS harness.
#[cfg(all(not(feature = "clipboard"), target_arch = "wasm32"))]
pub(super) fn clipboard_get() -> Option<String> {
    crate::wasm_clipboard::get()
}

#[cfg(feature = "clipboard")]
pub(super) fn clipboard_set(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_string());
    }
}
/// Native non-clipboard build: silently no-ops.
#[cfg(all(not(feature = "clipboard"), not(target_arch = "wasm32")))]
pub(super) fn clipboard_set(_: &str) {}
/// WASM build: write to the in-process buffer so the JS `copy`/`cut` handler
/// can forward it to the browser's system clipboard.
#[cfg(all(not(feature = "clipboard"), target_arch = "wasm32"))]
pub(super) fn clipboard_set(text: &str) {
    crate::wasm_clipboard::set(text);
}
