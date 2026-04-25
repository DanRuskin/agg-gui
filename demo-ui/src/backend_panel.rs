//! Backend panel — left-side collapsible panel shown when the "Backend" button
//! is active in the top bar.
//!
//! All text is rendered through `Label` children so that glyph rasterization
//! is cached to offscreen framebuffers (backbuffer path).  For the live FPS
//! display and screen-size label (which change every frame) the labels use
//! `buffered = false` since caching a value that changes every render cycle
//! adds overhead with no benefit.
//!
//! Contents mirror egui's backend panel:
//! - Renderer / backend info
//! - Screen size (live)
//! - Run mode (Reactive / Continuous)
//! - Frame rate sparkline + mean CPU usage label
//! - Inspector checkbox toggle
//! - "Reset all state" button

#![allow(unused_imports)]
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widget::paint_subtree;
use agg_gui::widgets::button::Button;
use agg_gui::{
    Color, DrawCtx, Event, EventResult, FlexColumn, Font, Insets, Label, Rect, Separator, Size,
    SizedBox, Widget,
};

// ── Run mode ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RunMode {
    Reactive,
    Continuous,
}

// ── Frame history (simple ring buffer) ────────────────────────────────────────

/// Rolling FPS / frame-time display — stores the last N frame times in ms.
pub struct FrameHistory {
    times: Vec<f32>,
    head: usize,
    len: usize,
}

impl FrameHistory {
    const CAP: usize = 60;

    pub fn new() -> Self {
        Self {
            times: vec![0.0; Self::CAP],
            head: 0,
            len: 0,
        }
    }

    pub fn push(&mut self, frame_ms: f32) {
        self.times[self.head] = frame_ms;
        self.head = (self.head + 1) % Self::CAP;
        if self.len < Self::CAP {
            self.len += 1;
        }
    }

    pub fn mean_ms(&self) -> f32 {
        if self.len == 0 {
            return 0.0;
        }
        self.times[..self.len].iter().sum::<f32>() / self.len as f32
    }

    #[allow(dead_code)]
    pub fn fps(&self) -> f32 {
        let m = self.mean_ms();
        if m < 0.001 {
            0.0
        } else {
            1000.0 / m
        }
    }

    /// Samples as a slice from oldest to newest (for sparkline rendering).
    pub fn samples(&self) -> impl Iterator<Item = f32> + '_ {
        let cap = Self::CAP;
        (0..self.len).map(move |i| {
            let idx = (self.head + cap - self.len + i) % cap;
            self.times[idx]
        })
    }
}

mod widgets;
pub(crate) use widgets::MsaaRow;
use widgets::{FpsLabel, RunModeDesc, RunModeRow, ScreenSizeLabel, Sparkline, TogglePill};

// ── Backend panel ─────────────────────────────────────────────────────────────

/// Build the backend panel widget (240 px wide).
///
/// Mirrors egui's Backend panel layout: renderer/backend info, screen size,
/// run mode selector, FPS sparkline + mean CPU usage, inspector checkbox,
/// and a reset button.
pub fn build_backend_panel(
    font: Arc<Font>,
    run_mode: Rc<Cell<RunMode>>,
    history: Rc<RefCell<FrameHistory>>,
    screen_size: Rc<Cell<(u32, u32)>>,
    show_inspector: Rc<Cell<bool>>,
    show_system: Rc<Cell<bool>>,
    renderer_name: &'static str,
    backend_name: &'static str,
    on_reset: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    let mut col = FlexColumn::new()
        .with_gap(0.0)
        .with_padding(0.0)
        .with_panel_bg();

    // ── Heading ────────────────────────────────────────────────────────────── (FA4 "laptop")
    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    col.push(
        Box::new(
            Label::new("\u{F109} Backend", Arc::clone(&font))
                .with_font_size(14.0)
                .with_margin(Insets::from_sides(12.0, 12.0, 4.0, 4.0)),
        ),
        0.0,
    );
    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(Box::new(SizedBox::new().with_height(4.0)), 0.0);

    // ── Renderer / backend info ────────────────────────────────────────────────
    let running_text = format!("agg-gui running inside {backend_name}.");
    col.push(
        Box::new(
            Label::new(running_text, Arc::clone(&font))
                .with_font_size(11.0)
                .with_wrap(true)
                .with_margin(Insets::from_sides(12.0, 12.0, 2.0, 2.0)),
        ),
        0.0,
    );
    let renderer_text = format!("Renderer: {renderer_name}");
    col.push(
        Box::new(
            Label::new(renderer_text, Arc::clone(&font))
                .with_font_size(11.0)
                .with_wrap(true)
                .with_margin(Insets::from_sides(12.0, 12.0, 2.0, 2.0)),
        ),
        0.0,
    );
    let backend_text = format!("Backend: {backend_name}");
    col.push(
        Box::new(
            Label::new(backend_text, Arc::clone(&font))
                .with_font_size(11.0)
                .with_wrap(true)
                .with_margin(Insets::from_sides(12.0, 12.0, 2.0, 2.0)),
        ),
        0.0,
    );

    // ── Screen size (live) ─────────────────────────────────────────────────────
    col.push(
        Box::new(ScreenSizeLabel::new(Arc::clone(&font), screen_size)),
        0.0,
    );
    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);

    // ── Run mode toggle ───────────────────────────────────────────────────────
    col.push(
        Box::new(
            Label::new("Mode", Arc::clone(&font))
                .with_font_size(11.0)
                .with_margin(Insets::from_sides(12.0, 12.0, 2.0, 0.0)),
        ),
        0.0,
    );

    col.push(
        Box::new(RunModeRow::new(Arc::clone(&font), Rc::clone(&run_mode))),
        0.0,
    );

    // Dynamic description: "Only running UI code..." (Reactive) or "FPS: X.X" (Continuous).
    col.push(
        Box::new(RunModeDesc::new(
            Arc::clone(&font),
            Rc::clone(&run_mode),
            Rc::clone(&history),
        )),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(4.0)), 0.0);
    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);

    // ── Mean CPU usage label (primary display, matches egui reference) ────────
    col.push(
        Box::new(FpsLabel::new(Arc::clone(&font), Rc::clone(&history))),
        0.0,
    );

    // ── FPS sparkline (CPU history graph) ────────────────────────────────────
    col.push(
        Box::new(
            SizedBox::new()
                .with_margin(Insets::from_sides(12.0, 12.0, 4.0, 8.0))
                .with_child(Box::new(Sparkline {
                    bounds: Rect::default(),
                    children: Vec::new(),
                    history: Rc::clone(&history),
                })),
        ),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);

    // ── agg-gui windows section (System + Inspector toggle pills) ─────────────
    //
    // Styled like the top-bar "Backend" button: solid pill, accent-filled
    // when the bound cell is true, label re-coloured for contrast.  Shared
    // look across the top bar + this sidebar means hit-testing and visual
    // affordance are consistent — checkboxes looked out of place next to
    // the Mode segmented control above.  MSAA moved to the System window's
    // "Render" tab (see `windows/system.rs`), so the sidebar stays focused
    // on runtime-togglable state.
    col.push(
        Box::new(
            Label::new("agg-gui windows:", Arc::clone(&font))
                .with_font_size(11.0)
                .with_margin(Insets::from_sides(12.0, 12.0, 2.0, 0.0)),
        ),
        0.0,
    );

    col.push(
        Box::new(TogglePill::new(
            Arc::clone(&font),
            "\u{F013} System",
            Rc::clone(&show_system),
        )),
        0.0,
    );
    col.push(Box::new(SizedBox::new().with_height(4.0)), 0.0);
    col.push(
        Box::new(TogglePill::new(
            Arc::clone(&font),
            "\u{F002} Inspector",
            Rc::clone(&show_inspector),
        )),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);

    // ── Reset button ──────────────────────────────────────────────────────────
    col.push(
        Box::new(
            SizedBox::new()
                .with_height(28.0)
                .with_margin(Insets::from_sides(12.0, 12.0, 4.0, 4.0))
                .with_child(Box::new(
                    Button::new("Reset all state", Arc::clone(&font))
                        .with_font_size(12.0)
                        .on_click(on_reset),
                )),
        ),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(12.0)), 0.0);

    // Flex spacer fills any remaining vertical space so the FlexColumn always
    // occupies the full panel height — this ensures with_panel_bg() paints
    // panel_fill over the entire panel area rather than stopping at content height.
    col.push(Box::new(SizedBox::new()), 1.0);

    Box::new(col)
}
