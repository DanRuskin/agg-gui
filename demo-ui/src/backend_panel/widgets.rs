use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widget::paint_subtree;
use agg_gui::{Color, DrawCtx, Event, EventResult, Font, Label, Rect, Size, Widget};

use super::{FrameHistory, RunMode};

// ── Sparkline widget ──────────────────────────────────────────────────────────

/// Renders a line chart of the last N frame times.  No text is drawn here —
/// the adjacent `FpsLabel` handles the textual display.
pub(super) struct Sparkline {
    pub(super) bounds: Rect,
    pub(super) children: Vec<Box<dyn Widget>>,
    pub(super) history: Rc<RefCell<FrameHistory>>,
}

impl Widget for Sparkline {
    fn type_name(&self) -> &'static str {
        "Sparkline"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
    fn layout(&mut self, available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, available.width, 48.0);
        Size::new(available.width, 48.0)
    }
    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let w = self.bounds.width;
        let h = self.bounds.height;
        let hist = self.history.borrow();

        // Background.
        ctx.set_fill_color(v.track_bg);
        ctx.begin_path();
        ctx.rounded_rect(0.0, 0.0, w, h, 4.0);
        ctx.fill();

        if hist.len < 2 {
            return;
        }
        let samples: Vec<f32> = hist.samples().collect();
        let max_ms = samples.iter().cloned().fold(0.1_f32, f32::max).max(16.7);

        // Draw line chart.
        ctx.set_stroke_color(v.accent);
        ctx.set_line_width(1.5);
        ctx.begin_path();
        let n = samples.len();
        for (i, &ms) in samples.iter().enumerate() {
            let x = i as f64 / (n - 1) as f64 * w;
            let y = (1.0 - ms as f64 / max_ms as f64) * (h - 4.0) + 2.0;
            if i == 0 {
                ctx.move_to(x, y);
            } else {
                ctx.line_to(x, y);
            }
        }
        ctx.stroke();

        // 16.7 ms reference line (60 fps target).
        let ref_y = (1.0 - 16.7 / max_ms as f64) * (h - 4.0) + 2.0;
        if ref_y >= 2.0 && ref_y <= h - 2.0 {
            ctx.set_stroke_color(Color::rgba(1.0, 0.6, 0.0, 0.7)); // orange 60fps reference line
            ctx.set_line_width(1.0);
            ctx.begin_path();
            ctx.move_to(0.0, ref_y);
            ctx.line_to(w, ref_y);
            ctx.stroke();
        }
    }
    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

// ── FPS label ─────────────────────────────────────────────────────────────────

/// Displays live frame-time statistics.  Uses `buffered = false` because
/// the text string changes every frame, so caching it to a backbuffer would
/// rebuild the cache every frame anyway — direct rasterization is cheaper.
pub(super) struct FpsLabel {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    history: Rc<RefCell<FrameHistory>>,
    /// Inner Label — not buffered (text changes every frame).
    label: Label,
}

impl FpsLabel {
    pub(super) fn new(font: Arc<Font>, history: Rc<RefCell<FrameHistory>>) -> Self {
        let mut label = Label::new("0.0 ms  (0 fps)", font).with_font_size(11.0);
        label.buffered = false; // live counter: no benefit to caching
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            history,
            label,
        }
    }
}

impl Widget for FpsLabel {
    fn type_name(&self) -> &'static str {
        "FpsLabel"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, available.width, 18.0);
        let s = self.label.layout(Size::new(available.width, 18.0));
        self.label
            .set_bounds(Rect::new(0.0, 0.0, s.width, s.height));
        Size::new(available.width, 18.0)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let hist = self.history.borrow();
        let text = format!("Mean CPU usage: {:.2} ms / frame", hist.mean_ms());
        drop(hist);

        // Update label text and color, then paint it.
        self.label.set_text(text);
        self.label.set_color(v.text_dim);

        let h = self.bounds.height;
        let lw = self.label.bounds().width;
        let lh = self.label.bounds().height;
        let ly = (h - lh) * 0.5;
        self.label.set_bounds(Rect::new(0.0, ly, lw, lh));

        ctx.save();
        ctx.translate(12.0, ly);
        paint_subtree(&mut self.label, ctx);
        ctx.restore();
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

// ── Screen size label ─────────────────────────────────────────────────────────

/// Displays the current screen dimensions.  Uses `buffered = false` because
/// the text changes on every resize event — direct rasterization is cheaper
/// than rebuilding the cache on each change.
pub(super) struct ScreenSizeLabel {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    screen_size: Rc<Cell<(u32, u32)>>,
    /// Inner Label — not buffered (value changes on resize).
    label: Label,
}

impl ScreenSizeLabel {
    pub(super) fn new(font: Arc<Font>, screen_size: Rc<Cell<(u32, u32)>>) -> Self {
        let mut label = Label::new("0 × 0", font).with_font_size(11.0);
        label.buffered = false;
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            screen_size,
            label,
        }
    }
}

impl Widget for ScreenSizeLabel {
    fn type_name(&self) -> &'static str {
        "ScreenSizeLabel"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, available.width, 18.0);
        let s = self.label.layout(Size::new(available.width, 18.0));
        self.label
            .set_bounds(Rect::new(0.0, 0.0, s.width, s.height));
        Size::new(available.width, 18.0)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let (w, h) = self.screen_size.get();
        let text = format!("{w} \u{00d7} {h}");

        self.label.set_text(text);
        self.label.set_color(v.text_dim);

        let height = self.bounds.height;
        let lw = self.label.bounds().width;
        let lh = self.label.bounds().height;
        let ly = (height - lh) * 0.5;
        self.label.set_bounds(Rect::new(0.0, ly, lw, lh));

        ctx.save();
        ctx.translate(12.0, ly);
        paint_subtree(&mut self.label, ctx);
        ctx.restore();
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

// ── Run mode row ─────────────────────────────────────────────────────────────

/// Reactive / Continuous toggle.  Two segmented buttons, each with a
/// backbuffered Label child.
pub(super) struct RunModeRow {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    run_mode: Rc<Cell<RunMode>>,
    hovered: Option<usize>,
    /// One Label per button.
    labels: Vec<Label>,
}

impl RunModeRow {
    const BTN_W: f64 = 96.0;
    const BTN_H: f64 = 24.0;
    const LABELS: &'static [&'static str] = &["Reactive", "Continuous"];

    pub(super) fn new(font: Arc<Font>, run_mode: Rc<Cell<RunMode>>) -> Self {
        let labels = Self::LABELS
            .iter()
            .map(|text| Label::new(*text, Arc::clone(&font)).with_font_size(12.0))
            .collect();
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            run_mode,
            hovered: None,
            labels,
        }
    }

    fn btn_rect(&self, i: usize) -> Rect {
        let gy = (self.bounds.height - Self::BTN_H) * 0.5;
        Rect::new(
            12.0 + i as f64 * (Self::BTN_W + 4.0),
            gy,
            Self::BTN_W,
            Self::BTN_H,
        )
    }
}

impl Widget for RunModeRow {
    fn type_name(&self) -> &'static str {
        "RunModeRow"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, available.width, Self::BTN_H + 8.0);
        for i in 0..2 {
            let r = self.btn_rect(i);
            let s = self.labels[i].layout(Size::new(r.width, r.height));
            self.labels[i].set_bounds(Rect::new(0.0, 0.0, s.width, s.height));
        }
        Size::new(available.width, Self::BTN_H + 8.0)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let current = self.run_mode.get();
        let modes = [RunMode::Reactive, RunMode::Continuous];

        for (i, (label_text, mode)) in Self::LABELS.iter().zip(modes.iter()).enumerate() {
            let r = self.btn_rect(i);
            let active = current == *mode;
            let hovered = self.hovered == Some(i);

            let bg = if active {
                v.accent
            } else if hovered {
                v.widget_bg_hovered
            } else {
                v.widget_bg
            };
            ctx.set_fill_color(bg);
            ctx.begin_path();
            ctx.rounded_rect(r.x, r.y, r.width, r.height, 4.0);
            ctx.fill();

            // Update label text + color.
            self.labels[i].set_text(*label_text);
            let text_color = if active { Color::white() } else { v.text_color };
            self.labels[i].set_color(text_color);

            // Center label within button.
            let lw = self.labels[i].bounds().width;
            let lh = self.labels[i].bounds().height;
            let lx = r.x + (r.width - lw) * 0.5;
            let ly = r.y + (r.height - lh) * 0.5;
            self.labels[i].set_bounds(Rect::new(lx, ly, lw, lh));

            ctx.save();
            ctx.translate(lx, ly);
            paint_subtree(&mut self.labels[i], ctx);
            ctx.restore();
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let hit = |p: agg_gui::Point| {
            (0..2).find(|&i| {
                let r = self.btn_rect(i);
                p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height
            })
        };
        match event {
            Event::MouseMove { pos } => {
                let was = self.hovered;
                self.hovered = hit(*pos);
                if was != self.hovered {
                    agg_gui::animation::request_draw();
                }
                EventResult::Ignored
            }
            Event::MouseDown {
                button: agg_gui::MouseButton::Left,
                pos,
                ..
            } => {
                if let Some(i) = hit(*pos) {
                    let next = [RunMode::Reactive, RunMode::Continuous][i];
                    if self.run_mode.get() != next {
                        self.run_mode.set(next);
                        agg_gui::animation::request_draw();
                    }
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }
}

// ── Toggle pill ──────────────────────────────────────────────────────────────

/// Sidebar button that toggles a bound `Rc<Cell<bool>>` on click — visually
/// matches the top-bar "Backend" button (solid rounded pill, accent-filled
/// when the cell is true, white label in the active state, dim hover fill
/// otherwise).  Used for the "System" and "Inspector" entries in the
/// Backend sidebar's "agg-gui windows" section so the sidebar's window
/// togglers share the same look as the rest of the app's chrome.
pub(super) struct TogglePill {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>, // always empty — label stored separately
    show: Rc<Cell<bool>>,
    hovered: bool,
    label: Label,
}

impl TogglePill {
    const H: f64 = 26.0;
    const LEFT_PAD: f64 = 12.0;
    const RIGHT_PAD: f64 = 12.0;

    pub(super) fn new(font: Arc<Font>, label_text: &'static str, show: Rc<Cell<bool>>) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            show,
            hovered: false,
            label: Label::new(label_text, font).with_font_size(12.0),
        }
    }
}

impl Widget for TogglePill {
    fn type_name(&self) -> &'static str {
        "TogglePill"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, available.width, Self::H + 4.0);
        let label_w = (available.width - Self::LEFT_PAD - Self::RIGHT_PAD).max(0.0);
        let s = self.label.layout(Size::new(label_w, Self::H));
        self.label
            .set_bounds(Rect::new(0.0, 0.0, s.width, s.height));
        Size::new(available.width, Self::H + 4.0)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let active = self.show.get();

        // Pill fills the full row width minus a small horizontal margin to
        // match the 12-px gutter used elsewhere in the sidebar.
        let gy = 2.0;
        let r = Rect::new(12.0, gy, (self.bounds.width - 24.0).max(0.0), Self::H);

        let bg = if active {
            v.accent
        } else if self.hovered {
            v.widget_bg_hovered
        } else {
            v.widget_bg
        };
        ctx.set_fill_color(bg);
        ctx.begin_path();
        ctx.rounded_rect(r.x, r.y, r.width, r.height, 4.0);
        ctx.fill();

        let text_color = if active { Color::white() } else { v.text_color };
        self.label.set_color(text_color);

        let lw = self.label.bounds().width;
        let lh = self.label.bounds().height;
        let lx = r.x + Self::LEFT_PAD;
        let ly = r.y + (r.height - lh) * 0.5;
        self.label.set_bounds(Rect::new(lx, ly, lw, lh));

        ctx.save();
        ctx.translate(lx, ly);
        paint_subtree(&mut self.label, ctx);
        ctx.restore();
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let gy = 2.0;
        let r = Rect::new(12.0, gy, (self.bounds.width - 24.0).max(0.0), Self::H);
        let hit = |p: agg_gui::Point| {
            p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height
        };
        match event {
            Event::MouseMove { pos } => {
                let was = self.hovered;
                self.hovered = hit(*pos);
                if was != self.hovered {
                    agg_gui::animation::request_draw();
                }
                EventResult::Ignored
            }
            Event::MouseDown {
                button: agg_gui::MouseButton::Left,
                pos,
                ..
            } => {
                if hit(*pos) {
                    self.show.set(!self.show.get());
                    agg_gui::animation::request_draw();
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }
}

// ── MSAA row ─────────────────────────────────────────────────────────────────

/// MSAA sample-count selector — five segmented buttons (Off / 2× / 4× / 8× /
/// 16×).  Writes to a shared `Rc<Cell<u8>>`; the platform harness reads that
/// value from the persisted state file at startup to configure the GL
/// surface.  Matches `RunModeRow`'s look and event model.
///
/// Exposed to other crate modules (the System window's Render tab uses the
/// same widget) via `pub(crate)`.
pub(crate) struct MsaaRow {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    samples: Rc<Cell<u8>>,
    hovered: Option<usize>,
    labels: Vec<Label>,
}

impl MsaaRow {
    const BTN_W: f64 = 44.0;
    const BTN_H: f64 = 24.0;
    const TEXT: &'static [&'static str] = &["Off", "2×", "4×", "8×", "16×"];
    const VALS: &'static [u8] = &[0, 2, 4, 8, 16];

    pub(crate) fn new(font: Arc<Font>, samples: Rc<Cell<u8>>) -> Self {
        let labels = Self::TEXT
            .iter()
            .map(|t| Label::new(*t, Arc::clone(&font)).with_font_size(12.0))
            .collect();
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            samples,
            hovered: None,
            labels,
        }
    }

    fn btn_rect(&self, i: usize) -> Rect {
        let gy = (self.bounds.height - Self::BTN_H) * 0.5;
        Rect::new(
            12.0 + i as f64 * (Self::BTN_W + 4.0),
            gy,
            Self::BTN_W,
            Self::BTN_H,
        )
    }
}

impl Widget for MsaaRow {
    fn type_name(&self) -> &'static str {
        "MsaaRow"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, available.width, Self::BTN_H + 8.0);
        for i in 0..Self::TEXT.len() {
            let r = self.btn_rect(i);
            let s = self.labels[i].layout(Size::new(r.width, r.height));
            self.labels[i].set_bounds(Rect::new(0.0, 0.0, s.width, s.height));
        }
        Size::new(available.width, Self::BTN_H + 8.0)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let current = self.samples.get();

        for i in 0..Self::TEXT.len() {
            let r = self.btn_rect(i);
            let active = current == Self::VALS[i];
            let hovered = self.hovered == Some(i);

            let bg = if active {
                v.accent
            } else if hovered {
                v.widget_bg_hovered
            } else {
                v.widget_bg
            };
            ctx.set_fill_color(bg);
            ctx.begin_path();
            ctx.rounded_rect(r.x, r.y, r.width, r.height, 4.0);
            ctx.fill();

            self.labels[i].set_text(Self::TEXT[i]);
            let text_color = if active { Color::white() } else { v.text_color };
            self.labels[i].set_color(text_color);

            let lw = self.labels[i].bounds().width;
            let lh = self.labels[i].bounds().height;
            let lx = r.x + (r.width - lw) * 0.5;
            let ly = r.y + (r.height - lh) * 0.5;
            self.labels[i].set_bounds(Rect::new(lx, ly, lw, lh));

            ctx.save();
            ctx.translate(lx, ly);
            paint_subtree(&mut self.labels[i], ctx);
            ctx.restore();
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let hit = |p: agg_gui::Point| {
            (0..Self::TEXT.len()).find(|&i| {
                let r = self.btn_rect(i);
                p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height
            })
        };
        match event {
            Event::MouseMove { pos } => {
                let was = self.hovered;
                self.hovered = hit(*pos);
                if was != self.hovered {
                    agg_gui::animation::request_draw();
                }
                EventResult::Ignored
            }
            Event::MouseDown {
                button: agg_gui::MouseButton::Left,
                pos,
                ..
            } => {
                if let Some(i) = hit(*pos) {
                    if self.samples.get() != Self::VALS[i] {
                        self.samples.set(Self::VALS[i]);
                        agg_gui::animation::request_draw();
                    }
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }
}

// ── Run mode description label ────────────────────────────────────────────────

/// Dynamic label beneath the run-mode buttons.
/// Reactive: "Only running UI code when there are animations or input."
/// Continuous: "Repainting the UI each frame. FPS: X.X"
pub(super) struct RunModeDesc {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    run_mode: Rc<Cell<RunMode>>,
    history: Rc<RefCell<FrameHistory>>,
    label: Label,
}

impl RunModeDesc {
    pub(super) fn new(
        font: Arc<Font>,
        run_mode: Rc<Cell<RunMode>>,
        history: Rc<RefCell<FrameHistory>>,
    ) -> Self {
        let mut label = Label::new("", Arc::clone(&font))
            .with_font_size(10.0)
            .with_wrap(true);
        label.buffered = false;
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            run_mode,
            history,
            label,
        }
    }
}

impl Widget for RunModeDesc {
    fn type_name(&self) -> &'static str {
        "RunModeDesc"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        // Set the text first so wrapped height is measured correctly for the
        // worst-case (reactive) string, then layout once within the available
        // width minus the 12-px horizontal padding used at paint time.
        self.label
            .set_text("Only running UI code when there are animations or input.".to_owned());
        let inner_w = (available.width - 24.0).max(1.0);
        let s = self.label.layout(Size::new(inner_w, f64::MAX / 2.0));
        self.label
            .set_bounds(Rect::new(0.0, 0.0, s.width, s.height));
        let h = (s.height + 8.0).max(18.0);
        self.bounds = Rect::new(0.0, 0.0, available.width, h);
        Size::new(available.width, h)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let text = match self.run_mode.get() {
            RunMode::Reactive => {
                "Only running UI code when there are animations or input.".to_owned()
            }
            RunMode::Continuous => {
                let hist = self.history.borrow();
                let fps = if hist.mean_ms() < 0.001 {
                    0.0
                } else {
                    1000.0 / hist.mean_ms()
                };
                format!("Running continuously as fast as possible. FPS: {fps:.1}")
            }
        };
        self.label.set_text(text);
        self.label.set_color(v.text_dim);

        let lh = self.label.bounds().height;
        let ly = ((self.bounds.height - lh) * 0.5).max(2.0);

        ctx.save();
        ctx.translate(12.0, ly);
        agg_gui::widget::paint_subtree(&mut self.label, ctx);
        ctx.restore();
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}
