//! Screenshot capture handle for agg-gui apps.
//!
//! The GL rendering harness (`GlGfxCtx::read_screenshot` on the desktop GL
//! path + the equivalent WebGL2 read-back in the WASM harness) produces a
//! top-down RGBA8 buffer of the current back buffer.  This module supplies
//! the small shared-state handle that a button or hotkey uses to
//! **request** a capture and that a widget uses to **display** the result.
//!
//! # Threading / ownership
//!
//! All fields are `Rc<...>` — single-threaded, cheap to clone.  Never
//! transfer a [`ScreenshotHandle`] across threads.
//!
//! # Wiring on native (winit + glow)
//!
//! ```ignore
//! let shot = agg_gui::ScreenshotHandle::new();
//!
//! // In a button's on_click:
//! let req = shot.request.clone();
//! Button::new("📷 Capture", font).on_click(move || req.set(true))
//!
//! // In the event loop, AFTER render_frame but BEFORE swap_buffers:
//! if shot.request.get() {
//!     let (rgba, w, h) = gl_ctx.read_screenshot();
//!     *shot.image.borrow_mut() = Some((rgba, w, h));
//!     shot.request.set(false);
//! }
//!
//! // Display: pass `shot.image` to `ImageView`.
//! ```
//!
//! # Wiring on WASM
//!
//! Same Rust-side flow — the browser's WebGL2 context still provides
//! `glReadPixels`, so `GlGfxCtx::read_screenshot()` works unchanged.  The
//! JS side needs no special code beyond driving the animation loop:
//!
//! ```ignore
//! // In the WASM render export (called from JS requestAnimationFrame):
//! if shot.request.get() {
//!     let (rgba, w, h) = gl_ctx.read_screenshot();  // must be BEFORE presenting
//!     *shot.image.borrow_mut() = Some((rgba, w, h));
//!     shot.request.set(false);
//! }
//! ```
//!
//! Note for the LLM / future dev: on WASM, `read_screenshot` MUST be called
//! before the browser composites the canvas (i.e. within the same rAF
//! tick, before yielding).  Because WebGL uses a preserved-drawing-buffer
//! only when explicitly requested, calling it outside that window yields
//! a blank image.  The natural "after paint, before yield" position in the
//! render function is correct.
//!
//! If the app wants to TRIGGER a browser download instead of displaying
//! in-canvas, export a WASM function that calls `read_screenshot`, encode
//! with the `png` crate via `agg_gui::encode_png_rgba` (if available in
//! the surrounding app), and pass the bytes to a JS helper that creates a
//! `Blob` + `URL.createObjectURL` + synthetic `<a download>` click.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Shared capture state.  Clone freely; all inner fields are `Rc<...>`.
#[derive(Clone)]
pub struct ScreenshotHandle {
    /// Set to `true` to request a capture on the next rendered frame.  The
    /// platform harness reads this cell after painting, captures the
    /// framebuffer into `image`, and clears the flag.
    pub request: Rc<Cell<bool>>,
    /// Most recent captured image — top-down RGBA8, plus `(width, height)`.
    /// `None` until the first capture completes.
    pub image:   Rc<RefCell<Option<(Vec<u8>, u32, u32)>>>,
}

impl ScreenshotHandle {
    pub fn new() -> Self {
        Self {
            request: Rc::new(Cell::new(false)),
            image:   Rc::new(RefCell::new(None)),
        }
    }

    /// Convenience: request a capture.  Equivalent to `self.request.set(true)`.
    pub fn take(&self) { self.request.set(true); }

    /// `true` while the latest request has not yet been fulfilled.
    pub fn pending(&self) -> bool { self.request.get() }

    /// Access the most recent capture without consuming it.
    pub fn has_image(&self) -> bool { self.image.borrow().is_some() }
}

impl Default for ScreenshotHandle {
    fn default() -> Self { Self::new() }
}
