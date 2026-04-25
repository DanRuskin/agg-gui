//! Shared demo UI — identical widget tree for both native and WASM targets.

mod api;
mod app_builder;
mod backend_panel;
mod content;
mod font_picker;
mod rendering_test;
mod shell;
mod sidebar;
mod specs;
mod state;
mod top_bar;
mod url;
mod windows;

pub use api::{DemoHandles, PlatformHooks, PlatformKind};
pub use app_builder::build_demo_ui;
pub use backend_panel::FrameHistory;
pub use state::{SavedState, StateAccessor, WindowState};
pub use windows::{window_resize_sub_windows, ResizeTestWindow};

/// Encode a top-down RGBA8 buffer as a PNG.
pub fn encode_png_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    agg_gui::screenshot::encode_png_rgba(rgba, width, height).unwrap_or_else(|e| {
        eprintln!("encode_png_rgba failed: {e}");
        Vec::new()
    })
}
