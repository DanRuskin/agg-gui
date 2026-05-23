//! Builder methods that install host-side callbacks on
//! [`super::NodeEditor`] — extracted from `widget/mod.rs` to keep that
//! file under the 800-line guardrail. The callbacks let app shells
//! reroute editor events (overlay dialogs, dropped files) instead of
//! consuming them locally.

use std::cell::Cell;
use std::rc::Rc;

use agg_gui::Widget;

use super::NodeEditor;

impl NodeEditor {
    /// Install a host-side handler for file-drop events on the canvas.
    /// `handler` receives the dropped paths and the canvas-space
    /// position of the cursor at drop time (Y-up). Useful for hosts
    /// that want a drop on the canvas to spawn an asset-backed node.
    pub fn with_file_drop_handler<F>(mut self, handler: F) -> Self
    where
        F: FnMut(&[std::path::PathBuf], [f64; 2]) + 'static,
    {
        self.file_drop_handler = Some(Box::new(handler));
        self
    }

    /// Install a host-side sink that receives newly-constructed
    /// floating dialogs (today: the `ColorWheelPicker` dialog) along
    /// with their close-flag. When a sink is set the editor does NOT
    /// keep the dialog as `self.overlay` — the host takes ownership
    /// and is responsible for layout / paint / event dispatch.
    ///
    /// Designed for app shells (e.g. AtomArtist) that want the dialog
    /// reparented to the top of the widget tree so the user can drag
    /// it anywhere on screen rather than being confined to the
    /// editor's pane. Without a sink the editor preserves its
    /// historical "overlay-inside-the-editor" behaviour, which is
    /// still the right default for the gallery demo and the
    /// node-editor's own unit tests.
    pub fn with_overlay_sink<F>(mut self, sink: F) -> Self
    where
        F: FnMut(Box<dyn Widget>, Rc<Cell<bool>>) + 'static,
    {
        self.overlay_sink = Some(Box::new(sink));
        self
    }
}
