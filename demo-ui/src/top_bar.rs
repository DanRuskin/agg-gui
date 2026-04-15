//! Top-bar widgets: theme toggle, app-tab bar, and backend toggle button.
//!
//! Exports:
//! - `AppTab` — selects which body pane is shown (Demos / 3D Cube / Rendering test)
//! - `build_top_bar_inner` — builds the FlexRow that fills the `TopMenuBar`

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::{
    DrawCtx, Event, EventResult,
    FlexRow, Font, Rect, Size, SizedBox, Widget,
    ThemePreference, Visuals, set_visuals,
};

/// Detect OS color scheme and return the matching `ThemePreference`.
pub fn detect_system_theme() -> ThemePreference {
    match dark_light::detect() {
        dark_light::Mode::Light | dark_light::Mode::Default => ThemePreference::Light,
        dark_light::Mode::Dark => ThemePreference::Dark,
    }
}

/// Apply visuals matching the current OS color scheme.
fn apply_system_visuals() {
    match detect_system_theme() {
        ThemePreference::Light  => set_visuals(Visuals::light()),
        ThemePreference::Dark   => set_visuals(Visuals::dark()),
        ThemePreference::System => {} // won't happen
    }
}

// ── App tab ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AppTab { Demos, Cube3D, RenderingTest }

// ── Theme toggle widget ────────────────────────────────────────────────────────

/// Three-button toggle: ☀ (Light) / 🌙 (Dark) / System.
/// Writes the chosen `Visuals` via `set_visuals()` when clicked.
struct ThemeToggle {
    bounds:  Rect,
    children: Vec<Box<dyn Widget>>,
    font:    Arc<Font>,
    pref:    Rc<Cell<ThemePreference>>,
    hovered: Option<usize>,
}

impl ThemeToggle {
    const BTN_W: f64 = 52.0;
    const BTN_H: f64 = 24.0;

    fn new(font: Arc<Font>, pref: Rc<Cell<ThemePreference>>) -> Self {
        Self { bounds: Rect::default(), children: Vec::new(), font, pref, hovered: None }
    }

    fn group_x(&self) -> f64 { 8.0 }

    fn btn_rect(&self, idx: usize) -> Rect {
        let gx = self.group_x();
        let gy = (self.bounds.height - Self::BTN_H) * 0.5;
        Rect::new(gx + idx as f64 * Self::BTN_W, gy, Self::BTN_W, Self::BTN_H)
    }

    fn hit_idx(&self, pos: agg_gui::Point) -> Option<usize> {
        for i in 0..3 {
            let r = self.btn_rect(i);
            if pos.x >= r.x && pos.x <= r.x + r.width
                && pos.y >= r.y && pos.y <= r.y + r.height
            { return Some(i); }
        }
        None
    }
}

impl Widget for ThemeToggle {
    fn type_name(&self) -> &'static str { "ThemeToggle" }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }

    fn layout(&mut self, available: Size) -> Size {
        let natural_w = (3.0 * Self::BTN_W + 16.0).min(available.width);
        self.bounds = Rect::new(0.0, 0.0, natural_w, available.height);
        Size::new(natural_w, available.height)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        ctx.set_font(Arc::clone(&self.font));
        let v = ctx.visuals();
        let current = self.pref.get();
        let labels = ["Light", "Dark", "System"];
        let prefs  = [ThemePreference::Light, ThemePreference::Dark, ThemePreference::System];

        for (i, (label, pref)) in labels.iter().zip(prefs.iter()).enumerate() {
            let r = self.btn_rect(i);
            let active  = std::mem::discriminant(&current) == std::mem::discriminant(pref);
            let hovered = self.hovered == Some(i);

            let bg = if active { v.accent }
                     else if hovered { v.widget_bg_hovered }
                     else { v.widget_bg };
            ctx.set_fill_color(bg);
            ctx.begin_path();
            let radius = if i == 0 || i == 2 { 4.0 } else { 0.0 };
            ctx.rounded_rect(r.x, r.y, r.width, r.height, radius);
            ctx.fill();

            if i < 2 {
                ctx.set_fill_color(v.widget_stroke);
                ctx.begin_path();
                ctx.rect(r.x + r.width - 1.0, r.y, 1.0, r.height);
                ctx.fill();
            }

            let text_color = if active { v.window_title_text } else { v.text_color };
            ctx.set_fill_color(text_color);
            ctx.set_font_size(11.0);
            if let Some(m) = ctx.measure_text(label) {
                let tx = r.x + (r.width - m.width) * 0.5;
                let ty = r.y + r.height * 0.5 - (m.ascent - m.descent) * 0.5 + m.descent;
                ctx.fill_text(label, tx, ty);
            }
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { pos } => {
                self.hovered = self.hit_idx(*pos);
                EventResult::Ignored
            }
            Event::MouseDown { button: agg_gui::MouseButton::Left, pos, .. } => {
                if let Some(idx) = self.hit_idx(*pos) {
                    let pref = [ThemePreference::Light, ThemePreference::Dark, ThemePreference::System][idx];
                    self.pref.set(pref);
                    match pref {
                        ThemePreference::Light  => set_visuals(Visuals::light()),
                        ThemePreference::Dark   => set_visuals(Visuals::dark()),
                        ThemePreference::System => apply_system_visuals(),
                    }
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }
}

// ── App tab bar widget ────────────────────────────────────────────────────────

/// Segmented tab selector: "Demos" | "3D Cube" | "Rendering test".
struct AppTabBar {
    bounds:   Rect,
    children: Vec<Box<dyn Widget>>,
    font:     Arc<Font>,
    tab:      Rc<Cell<AppTab>>,
    hovered:  Option<usize>,
}

impl AppTabBar {
    const LABELS: &'static [&'static str] = &["Demos", "3D Cube", "Rendering test"];
    const BTN_H:  f64 = 24.0;
    const PAD_X:  f64 = 12.0;

    fn new(font: Arc<Font>, tab: Rc<Cell<AppTab>>) -> Self {
        Self { bounds: Rect::default(), children: Vec::new(), font, tab, hovered: None }
    }

    fn tab_width(font: &Font, label: &str, fs: f64) -> f64 {
        agg_gui::text::measure_text_metrics(font, label, fs).width + Self::PAD_X * 2.0
    }

    fn natural_width(&self) -> f64 {
        Self::LABELS.iter().map(|l| Self::tab_width(&self.font, l, 12.0)).sum::<f64>()
    }

    fn tab_rects(&self) -> Vec<Rect> {
        let gy = (self.bounds.height - Self::BTN_H) * 0.5;
        let mut x = 0.0;
        Self::LABELS.iter().map(|l| {
            let w = Self::tab_width(&self.font, l, 12.0);
            let r = Rect::new(x, gy, w, Self::BTN_H);
            x += w;
            r
        }).collect()
    }

    fn hit_idx(&self, pos: agg_gui::Point) -> Option<usize> {
        for (i, r) in self.tab_rects().iter().enumerate() {
            if pos.x >= r.x && pos.x <= r.x + r.width
                && pos.y >= r.y && pos.y <= r.y + r.height
            { return Some(i); }
        }
        None
    }
}

impl Widget for AppTabBar {
    fn type_name(&self) -> &'static str { "AppTabBar" }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }

    fn layout(&mut self, available: Size) -> Size {
        let w = self.natural_width().min(available.width);
        self.bounds = Rect::new(0.0, 0.0, w, available.height);
        Size::new(w, available.height)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        ctx.set_font(Arc::clone(&self.font));
        ctx.set_font_size(12.0);
        let v = ctx.visuals();
        let current = self.tab.get();
        let tabs = [AppTab::Demos, AppTab::Cube3D, AppTab::RenderingTest];

        for (i, (rect, tab)) in self.tab_rects().iter().zip(tabs.iter()).enumerate() {
            let active  = current == *tab;
            let hovered = self.hovered == Some(i);

            let bg = if active { v.accent }
                     else if hovered { v.widget_bg_hovered }
                     else { v.widget_bg };
            ctx.set_fill_color(bg);
            ctx.begin_path();
            let r = if i == 0 || i == tabs.len() - 1 { 4.0 } else { 0.0 };
            ctx.rounded_rect(rect.x, rect.y, rect.width, rect.height, r);
            ctx.fill();

            if i < tabs.len() - 1 {
                ctx.set_fill_color(v.widget_stroke);
                ctx.begin_path();
                ctx.rect(rect.x + rect.width - 1.0, rect.y, 1.0, rect.height);
                ctx.fill();
            }

            let text_color = if active { v.window_title_text } else { v.text_color };
            ctx.set_fill_color(text_color);
            let label = Self::LABELS[i];
            if let Some(m) = ctx.measure_text(label) {
                let tx = rect.x + (rect.width - m.width) * 0.5;
                let ty = rect.y + rect.height * 0.5 - (m.ascent - m.descent) * 0.5 + m.descent;
                ctx.fill_text(label, tx, ty);
            }
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { pos } => { self.hovered = self.hit_idx(*pos); EventResult::Ignored }
            Event::MouseDown { button: agg_gui::MouseButton::Left, pos, .. } => {
                if let Some(i) = self.hit_idx(*pos) {
                    let tabs = [AppTab::Demos, AppTab::Cube3D, AppTab::RenderingTest];
                    self.tab.set(tabs[i]);
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }
}

// ── Backend toggle button ─────────────────────────────────────────────────────

/// "💻 Backend" button — toggles the left-side backend panel.
struct BackendButton {
    bounds:   Rect,
    children: Vec<Box<dyn Widget>>,
    font:     Arc<Font>,
    show:     Rc<Cell<bool>>,
    hovered:  bool,
}

impl BackendButton {
    const W: f64 = 96.0;
    const H: f64 = 24.0;

    fn new(font: Arc<Font>, show: Rc<Cell<bool>>) -> Self {
        Self { bounds: Rect::default(), children: Vec::new(), font, show, hovered: false }
    }

    fn btn_rect(&self) -> Rect {
        let gy = (self.bounds.height - Self::H) * 0.5;
        Rect::new(4.0, gy, Self::W, Self::H)
    }
}

impl Widget for BackendButton {
    fn type_name(&self) -> &'static str { "BackendButton" }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }

    fn layout(&mut self, available: Size) -> Size {
        let w = Self::W + 8.0;
        self.bounds = Rect::new(0.0, 0.0, w, available.height);
        Size::new(w, available.height)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        ctx.set_font(Arc::clone(&self.font));
        ctx.set_font_size(12.0);
        let v = ctx.visuals();
        let r = self.btn_rect();
        let active = self.show.get();

        let bg = if active { v.accent }
                 else if self.hovered { v.widget_bg_hovered }
                 else { v.widget_bg };
        ctx.set_fill_color(bg);
        ctx.begin_path();
        ctx.rounded_rect(r.x, r.y, r.width, r.height, 4.0);
        ctx.fill();

        let text_color = if active { v.window_title_text } else { v.text_color };
        ctx.set_fill_color(text_color);
        let label = "💻 Backend";
        if let Some(m) = ctx.measure_text(label) {
            let tx = r.x + (r.width - m.width) * 0.5;
            let ty = r.y + r.height * 0.5 - (m.ascent - m.descent) * 0.5 + m.descent;
            ctx.fill_text(label, tx, ty);
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let r = self.btn_rect();
        let in_btn = |p: agg_gui::Point| {
            p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height
        };
        match event {
            Event::MouseMove { pos } => { self.hovered = in_btn(*pos); EventResult::Ignored }
            Event::MouseDown { button: agg_gui::MouseButton::Left, pos, .. } => {
                if in_btn(*pos) {
                    self.show.set(!self.show.get());
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }
}

// ── Factory ───────────────────────────────────────────────────────────────────

/// Build the FlexRow child for `TopMenuBar`.
///
/// Layout: [Backend button] [spacer] [flex] [AppTabBar] [flex] [ThemeToggle]
pub fn build_top_bar_inner(
    font:         Arc<Font>,
    app_tab:      Rc<Cell<AppTab>>,
    show_backend: Rc<Cell<bool>>,
    theme_pref:   Rc<Cell<ThemePreference>>,
) -> Box<dyn Widget> {
    Box::new(FlexRow::new()
        .with_gap(0.0)
        .add(Box::new(BackendButton::new(Arc::clone(&font), show_backend)))
        .add(Box::new(SizedBox::new().with_width(8.0)))
        .add_flex(Box::new(SizedBox::new()), 1.0)
        .add(Box::new(AppTabBar::new(Arc::clone(&font), app_tab)))
        .add_flex(Box::new(SizedBox::new()), 1.0)
        .add(Box::new(ThemeToggle::new(font, theme_pref))))
}
