//! Cross-platform URL opener.
//!
//! Single entry point used by clickable hyperlinks in the demo UI
//! (e.g. the "View on GitHub" link in the top bar).  Routes to the
//! browser via the OS shell on native targets, and to
//! `window.open(url, "_blank")` in the browser on WASM.

/// Open `url` in the user's default browser, in a new tab/window.
/// Failures (no browser available, popup blocked, etc.) are silently
/// ignored — the link is informational, not load-bearing.
pub fn open_url(url: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = webbrowser::open(url);
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            // `_blank` ⇒ new tab.  Match GitHub's normal "open in new tab"
            // convention so users don't lose the demo when they follow
            // the link.
            let _ = win.open_with_url_and_target(url, "_blank");
        }
    }
}
