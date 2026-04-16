//! Shared demo UI — identical widget tree for both native and WASM targets.
//!
//! Implements the egui-style three-panel layout:
//! - **Top menu bar** (~36 px): "File" menu bar matching egui demo layout.
//! - **Central canvas**: floating `Window` widgets, one per demo.
//! - **Right sidebar** (~220 px): scrollable checkbox list grouped by Demos/Tests,
//!   with "Organize windows" button at the bottom — matching egui exactly.
//!
//! The only platform-specific piece is the 3D cube widget, passed by the caller.

mod backend_panel;
mod rendering_test;
mod sidebar;
mod state;
mod top_bar;
mod windows;

pub use state::{SavedState, StateAccessor, WindowState};
pub use backend_panel::FrameHistory;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::{
    App, DrawCtx, Event, EventResult, Key, Modifiers,
    FlexColumn, FlexRow, Font, InspectorNode, InspectorPanel,
    Rect, Size, SizedBox, Stack, Widget, Window,
    ThemePreference,
};

use backend_panel::{RunMode, build_backend_panel};
use sidebar::{SidebarEntry, build_sidebar};
use top_bar::build_top_bar_inner;

// ── Canvas background ──────────────────────────────────────────────────────────

struct CanvasBg { bounds: Rect, children: Vec<Box<dyn Widget>> }

impl CanvasBg {
    fn new() -> Self { Self { bounds: Rect::default(), children: Vec::new() } }
}

impl Widget for CanvasBg {
    fn type_name(&self) -> &'static str { "CanvasBg" }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }
    fn layout(&mut self, available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, available.width, available.height);
        available
    }
    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        ctx.set_fill_color(ctx.visuals().bg_color);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, self.bounds.width, self.bounds.height);
        ctx.fill();
    }
    fn on_event(&mut self, _: &Event) -> EventResult { EventResult::Ignored }
}

// ── Top menu bar ──────────────────────────────────────────────────────────────

/// Thin bar at the top of the window — mirrors egui's `Panel::top("menu_bar")`.
/// Contains a theme-toggle row on the right (☀ / 🌙 / System).
// Layout: a single FlexRow child fills the bar.
struct TopMenuBar {
    bounds:   Rect,
    children: Vec<Box<dyn Widget>>,
}

impl TopMenuBar {
    fn new(inner_row: Box<dyn Widget>) -> Self {
        Self {
            bounds:   Rect::default(),
            children: vec![inner_row],
        }
    }
}

impl Widget for TopMenuBar {
    fn type_name(&self) -> &'static str { "TopMenuBar" }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }

    fn layout(&mut self, available: Size) -> Size {
        let h = 36.0_f64;
        self.bounds = Rect::new(0.0, 0.0, available.width, h);
        if let Some(child) = self.children.first_mut() {
            child.layout(Size::new(available.width, h));
            child.set_bounds(Rect::new(0.0, 0.0, available.width, h));
        }
        Size::new(available.width, h)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        ctx.set_fill_color(v.top_bar_bg);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, self.bounds.width, self.bounds.height);
        ctx.fill();
        // Bottom separator line.
        ctx.set_fill_color(v.separator);
        ctx.begin_path();
        ctx.rect(0.0, self.bounds.height - 1.0, self.bounds.width, 1.0);
        ctx.fill();
    }

    fn on_event(&mut self, _: &Event) -> EventResult { EventResult::Ignored }
}

// ── Inspector overlay (right edge of canvas) ──────────────────────────────────

struct InspectorOverlay {
    bounds:         Rect,
    show:           Rc<Cell<bool>>,
    children:       Vec<Box<dyn Widget>>,
}

impl Widget for InspectorOverlay {
    fn type_name(&self) -> &'static str { "InspectorOverlay" }
    fn is_visible(&self) -> bool { self.show.get() }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }

    fn layout(&mut self, available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, available.width, available.height);
        let panel_w = 300.0_f64.min(available.width);
        let panel_x = available.width - panel_w;
        if let Some(child) = self.children.first_mut() {
            // Child positioned at the right edge in local coordinates.
            child.set_bounds(Rect::new(panel_x, 0.0, panel_w, available.height));
            child.layout(Size::new(panel_w, available.height));
        }
        available
    }

    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {}

    fn on_event(&mut self, _: &Event) -> EventResult { EventResult::Ignored }

    fn hit_test(&self, local_pos: agg_gui::Point) -> bool {
        if !self.show.get() { return false; }
        let panel_w = 300.0_f64.min(self.bounds.width);
        let panel_x = self.bounds.width - panel_w;
        local_pos.x >= panel_x && local_pos.x <= self.bounds.width
            && local_pos.y >= 0.0 && local_pos.y <= self.bounds.height
    }
}

// ── Backend panel pane ────────────────────────────────────────────────────────

/// Wraps the backend panel; returns zero width when hidden so FlexRow collapses it.
struct BackendPane {
    bounds:   Rect,
    children: Vec<Box<dyn Widget>>,
    show:     Rc<Cell<bool>>,
}

impl BackendPane {
    const PANEL_W: f64 = 240.0;
}

impl Widget for BackendPane {
    fn type_name(&self) -> &'static str { "BackendPane" }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }

    fn layout(&mut self, available: Size) -> Size {
        if !self.show.get() {
            self.bounds = Rect::new(0.0, 0.0, 0.0, available.height);
            return Size::new(0.0, available.height);
        }
        let w = Self::PANEL_W.min(available.width);
        self.bounds = Rect::new(0.0, 0.0, w, available.height);
        if let Some(child) = self.children.first_mut() {
            child.layout(Size::new(w, available.height));
            child.set_bounds(Rect::new(0.0, 0.0, w, available.height));
        }
        Size::new(w, available.height)
    }

    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {}
    fn on_event(&mut self, _: &Event) -> EventResult { EventResult::Ignored }
}

// ── Window tiling ──────────────────────────────────────────────────────────────

const WIN_COLS:     usize = 4;
const WIN_W:        f64   = 360.0;
const WIN_H:        f64   = 290.0;
const WIN_GAP_X:    f64   = 20.0;
const WIN_GAP_Y:    f64   = 20.0;
const WIN_ORIGIN_X: f64   = 20.0;
const WIN_ORIGIN_Y: f64   = 20.0; // from the TOP of the canvas (Y-down thinking)

/// Compute the tiled rect for demo index `i` given canvas `height` (Y-up space).
fn tile_rect(i: usize, canvas_height: f64, win_w: f64, win_h: f64) -> Rect {
    let col = i % WIN_COLS;
    let row = i / WIN_COLS;
    let x        = WIN_ORIGIN_X + col as f64 * (WIN_W + WIN_GAP_X);
    let y_down   = WIN_ORIGIN_Y + row as f64 * (WIN_H + WIN_GAP_Y);
    let y        = (canvas_height - y_down - win_h).max(4.0);
    Rect::new(x, y, win_w, win_h)
}

// ── Demo window list ───────────────────────────────────────────────────────────

struct DemoSpec {
    title:  &'static str,
    label:  &'static str,
    open:   bool,
    win_w:  f64,
    win_h:  f64,
}

// Exact egui demo list (alphabetical) + our 3D Cube extra at the end.
// Default open matches egui: Code Example + Widget Gallery.  3D Cube is our
// addition and is open by default as the showcase feature.
const DEMOS: &[DemoSpec] = &[
    DemoSpec { title: "Bézier Curve",          label: "Bézier Curve",          open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Code Editor",           label: "Code Editor",           open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Code Example",          label: "Code Example",          open: true,  win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Dancing Strings",       label: "Dancing Strings",       open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Drag and Drop",         label: "Drag and Drop",         open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Extra Viewport",        label: "Extra Viewport",        open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Font Book",             label: "Font Book",             open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Frame",                 label: "Frame",                 open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Highlighting",          label: "Highlighting",          open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Interactive Container", label: "Interactive Container", open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Misc Demos",            label: "Misc Demos",            open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Modals",                label: "Modals",                open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Multi Touch",           label: "Multi Touch",           open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Painting",              label: "Painting",              open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Panels",                label: "Panels",                open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Popups",                label: "Popups",                open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Rendering Test",        label: "Rendering Test",        open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Scene",                 label: "Scene",                 open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Screenshot",            label: "Screenshot",            open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Scrolling",             label: "Scrolling",             open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Sliders",               label: "Sliders",               open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Strip",                 label: "Strip",                 open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Table",                 label: "Table",                 open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "TextEdit",              label: "TextEdit",              open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Text Layout",           label: "Text Layout",           open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Tooltips",              label: "Tooltips",              open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Undo Redo",             label: "Undo Redo",             open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Widget Gallery",        label: "Widget Gallery",        open: true,  win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Window Options",        label: "Window Options",        open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "3D Cube",               label: "3D Cube",               open: false, win_w: 300.0, win_h: 260.0 },
];

// All 11 egui test windows — matching egui's Tests section exactly.
const TESTS: &[DemoSpec] = &[
    DemoSpec { title: "Clipboard Test",      label: "Clipboard Test",      open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Cursor Test",         label: "Cursor Test",         open: false, win_w: 296.0, win_h: 560.0 },
    DemoSpec { title: "Grid Test",           label: "Grid Test",           open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Id Test",             label: "Id Test",             open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Input Event History", label: "Input Event History", open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Input Test",          label: "Input Test",          open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Layout Test",         label: "Layout Test",         open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Manual Layout Test",  label: "Manual Layout Test",  open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "SVG Test",            label: "SVG Test",            open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "Tessellation Test",   label: "Tessellation Test",   open: false, win_w: WIN_W, win_h: WIN_H },
    DemoSpec { title: "↔ auto-sized",        label: "Window Resize Test",  open: false, win_w: WIN_W, win_h: WIN_H },
];

// ── Index of the 3D Cube in DEMOS (computed once) ─────────────────────────────
const CUBE_IDX: usize = 29; // must match position of "3D Cube" in DEMOS (shifted by "Rendering Test")

// ── Public API ─────────────────────────────────────────────────────────────────

/// Handles returned by `build_demo_ui` — shared cells used by the platform harness.
pub struct DemoHandles {
    pub show_inspector:  Rc<Cell<bool>>,
    pub inspector_nodes: Rc<RefCell<Vec<InspectorNode>>>,
    pub hovered_bounds:  Rc<RefCell<Option<Rect>>>,
    pub cube_visible:    Rc<Cell<bool>>,
    pub screen_size:     Rc<Cell<(u32, u32)>>,
    pub frame_history:   Rc<RefCell<FrameHistory>>,
    pub state:           StateAccessor,
}

/// Build the full demo `App`.
///
/// Returns `(App, DemoHandles)`. `initial_state` restores window positions and
/// open flags from a previous session; pass `None` on first run.
pub fn build_demo_ui(
    font:           Arc<Font>,
    cube_widget:    Box<dyn Widget>,
    renderer_name:  &'static str,
    backend_name:   &'static str,
    initial_state:  Option<SavedState>,
) -> (App, DemoHandles) {
    let show_inspector  = Rc::new(Cell::new(false));
    let inspector_nodes = Rc::new(RefCell::new(Vec::<InspectorNode>::new()));
    let hovered_bounds  = Rc::new(RefCell::new(None::<Rect>));
    let screen_size     = Rc::new(Cell::new((0u32, 0u32)));

    // Theme preference — detect OS color scheme so we start in the right mode.
    let initial_theme = top_bar::detect_system_theme();
    match initial_theme {
        ThemePreference::Light => agg_gui::set_visuals(agg_gui::Visuals::light()),
        _                      => agg_gui::set_visuals(agg_gui::Visuals::dark()),
    }
    let theme_pref = Rc::new(Cell::new(initial_theme));

    // ── Backend panel visibility ───────────────────────────────────────────────
    let show_backend = Rc::new(Cell::new(false));

    // ── Backend panel state ────────────────────────────────────────────────────
    let run_mode      = Rc::new(Cell::new(RunMode::Reactive));
    let frame_history = Rc::new(RefCell::new(FrameHistory::new()));

    // ── About window open-state cell ──────────────────────────────────────────
    let about_initially_open = initial_state.as_ref()
        .map(|st| st.about.open)
        .unwrap_or(true);
    let about_open = Rc::new(Cell::new(about_initially_open));

    // ── Sidebar entries ────────────────────────────────────────────────────────
    let demo_entries: Vec<SidebarEntry> = DEMOS.iter().enumerate()
        .map(|(i, s)| {
            let open = initial_state.as_ref()
                .and_then(|st| st.demos.get(i))
                .map(|ws| ws.open)
                .unwrap_or(s.open);
            SidebarEntry::new(s.label, open)
        })
        .collect();
    let test_entries: Vec<SidebarEntry> = TESTS.iter().enumerate()
        .map(|(i, s)| {
            let open = initial_state.as_ref()
                .and_then(|st| st.tests.get(i))
                .map(|ws| ws.open)
                .unwrap_or(s.open);
            SidebarEntry::new(s.label, open)
        })
        .collect();

    // cube_visible shares the same cell as the 3D Cube sidebar entry.
    let cube_visible = Rc::clone(&demo_entries[CUBE_IDX].open);

    // ── Reset cells — one per window ───────────────────────────────────────────
    let all_specs_count = DEMOS.len() + TESTS.len();
    let reset_cells: Vec<Rc<Cell<Option<Rect>>>> = (0..all_specs_count)
        .map(|_| Rc::new(Cell::new(None)))
        .collect();

    // ── Position output cells — written each layout pass for persistence ───────
    let demo_pos_cells: Vec<Rc<Cell<Rect>>> = (0..DEMOS.len())
        .map(|_| Rc::new(Cell::new(Rect::default())))
        .collect();
    let test_pos_cells: Vec<Rc<Cell<Rect>>> = (0..TESTS.len())
        .map(|_| Rc::new(Cell::new(Rect::default())))
        .collect();
    let about_pos_cell: Rc<Cell<Rect>> = Rc::new(Cell::new(Rect::default()));

    // Default canvas height used by tile_rect. 720px is a reasonable fallback;
    // it will look correct on most 1080p+ screens after accounting for the OS bar.
    let default_canvas_h = 720.0_f64;

    // ── Organize Windows callback ──────────────────────────────────────────────
    // Two separate clones: one for the sidebar button, one for Ctrl+Shift+O shortcut.
    let rc_for_cb: Vec<_>  = reset_cells.iter().map(Rc::clone).collect();
    let rc_for_key: Vec<_> = reset_cells.iter().map(Rc::clone).collect();

    let specs_w: Vec<f64> = DEMOS.iter().map(|s| s.win_w)
        .chain(TESTS.iter().map(|s| s.win_w))
        .collect();
    let specs_h: Vec<f64> = DEMOS.iter().map(|s| s.win_h)
        .chain(TESTS.iter().map(|s| s.win_h))
        .collect();

    let on_organize = {
        let sw = specs_w.clone();
        let sh = specs_h.clone();
        move || {
            for (i, cell) in rc_for_cb.iter().enumerate() {
                let r = tile_rect(i, default_canvas_h, sw[i], sh[i]);
                cell.set(Some(r));
            }
        }
    };

    // ── Sidebar ────────────────────────────────────────────────────────────────
    let sidebar_widget = build_sidebar(
        Arc::clone(&font),
        Rc::clone(&about_open),
        &demo_entries,
        &test_entries,
        on_organize,
    );
    let sidebar_panel = SizedBox::new()
        .with_width(220.0)
        .with_child(sidebar_widget);

    // ── Canvas stack (floating windows) ───────────────────────────────────────
    let mut canvas = Stack::new().add(Box::new(CanvasBg::new()));

    // Add DEMO windows.
    for (i, spec) in DEMOS.iter().enumerate() {
        let open_cell  = Rc::clone(&demo_entries[i].open);
        let reset_cell = Rc::clone(&reset_cells[i]);
        let initial = initial_state.as_ref()
            .and_then(|st| st.demos.get(i))
            .map(|ws| ws.to_rect())
            .unwrap_or_else(|| tile_rect(i, default_canvas_h, spec.win_w, spec.win_h));

        let content: Box<dyn Widget> = if i == CUBE_IDX {
            // Cube content requires the platform-provided cube_widget.
            // Use a placeholder here; replaced immediately after the loop.
            windows::coming_soon()
        } else {
            build_demo_content(spec.title, Arc::clone(&font))
        };

        let win = Window::new(spec.title, Arc::clone(&font), content)
            .with_bounds(Rect::new(initial.x, initial.y, initial.width, initial.height))
            .with_visible_cell(open_cell)
            .with_reset_cell(reset_cell)
            .with_position_cell(Rc::clone(&demo_pos_cells[i]));
        canvas = canvas.add(Box::new(win));
    }

    // Replace the placeholder cube window with the real GL cube content.
    // Children layout: [0] = CanvasBg, [1..=30] = DEMOS windows in order.
    {
        let open_cell  = Rc::clone(&demo_entries[CUBE_IDX].open);
        let reset_cell = Rc::clone(&reset_cells[CUBE_IDX]);
        let spec       = &DEMOS[CUBE_IDX];
        let initial = initial_state.as_ref()
            .and_then(|st| st.demos.get(CUBE_IDX))
            .map(|ws| ws.to_rect())
            .unwrap_or_else(|| tile_rect(CUBE_IDX, default_canvas_h, spec.win_w, spec.win_h));
        let content    = windows::cube_content(Arc::clone(&font), cube_widget);
        let win = Window::new(spec.title, Arc::clone(&font), content)
            .with_bounds(Rect::new(initial.x, initial.y, initial.width, initial.height))
            .with_visible_cell(open_cell)
            .with_reset_cell(reset_cell)
            .with_position_cell(Rc::clone(&demo_pos_cells[CUBE_IDX]));
        // Replace index 1 + CUBE_IDX (offset by the CanvasBg at [0]).
        canvas.children_mut()[1 + CUBE_IDX] = Box::new(win);
    }

    // Add TEST windows.
    for (i, spec) in TESTS.iter().enumerate() {
        let total_i    = DEMOS.len() + i;
        let open_cell  = Rc::clone(&test_entries[i].open);
        let reset_cell = Rc::clone(&reset_cells[total_i]);
        let initial = initial_state.as_ref()
            .and_then(|st| st.tests.get(i))
            .map(|ws| ws.to_rect())
            .unwrap_or_else(|| tile_rect(total_i, default_canvas_h, spec.win_w, spec.win_h));
        let content: Box<dyn Widget> = match spec.title {
            "Clipboard Test"      => windows::clipboard_test(Arc::clone(&font)),
            "Cursor Test"         => windows::cursor_test(Arc::clone(&font)),
            "Grid Test"           => windows::grid_test(Arc::clone(&font)),
            "Id Test"             => windows::id_test(Arc::clone(&font)),
            "Input Event History" => windows::input_event_history(Arc::clone(&font)),
            "Input Test"          => windows::input_test(Arc::clone(&font)),
            "Layout Test"         => windows::layout_test(Arc::clone(&font)),
            "Manual Layout Test"  => windows::manual_layout_test(Arc::clone(&font)),
            "SVG Test"            => windows::svg_test(Arc::clone(&font)),
            "Tessellation Test"   => windows::tessellation_test(Arc::clone(&font)),
            "↔ auto-sized"        => windows::window_resize_test(Arc::clone(&font)),
            _                     => windows::coming_soon(),
        };
        let win = Window::new(spec.title, Arc::clone(&font), content)
            .with_bounds(Rect::new(initial.x, initial.y, initial.width, initial.height))
            .with_visible_cell(open_cell)
            .with_reset_cell(reset_cell)
            .with_position_cell(Rc::clone(&test_pos_cells[i]));
        canvas = canvas.add(Box::new(win));
    }

    // ── Window Resize Test — 5 additional sub-windows (all share test_entries[10].open) ──
    // The sidebar checkbox "Window Resize Test" shows/hides all 6 windows together,
    // matching the egui reference where a single `open: &mut bool` controls all.
    {
        let wrt_open = Rc::clone(&test_entries[10].open);
        for (title, content, initial_rect) in
            windows::window_resize_sub_windows(Arc::clone(&font))
        {
            let win = Window::new(&title, Arc::clone(&font), content)
                .with_bounds(initial_rect)
                .with_visible_cell(Rc::clone(&wrt_open));
            canvas = canvas.add(Box::new(win));
        }
    }

    // ── About window ──────────────────────────────────────────────────────────
    {
        let about_initial = initial_state.as_ref()
            .map(|st| st.about.to_rect())
            .unwrap_or_else(|| Rect::new(80.0, 80.0, 440.0, 500.0));
        let about_win = Window::new("About agg-gui", Arc::clone(&font), windows::about(Arc::clone(&font)))
            .with_bounds(about_initial)
            .with_visible_cell(Rc::clone(&about_open))
            .with_position_cell(Rc::clone(&about_pos_cell));
        canvas = canvas.add(Box::new(about_win));
    }

    // ── Inspector overlay ──────────────────────────────────────────────────────
    let inspector = InspectorPanel::new(
        Arc::clone(&font),
        Rc::clone(&inspector_nodes),
        Rc::clone(&hovered_bounds),
    );
    let inspector_overlay = InspectorOverlay {
        bounds:   Rect::default(),
        show:     Rc::clone(&show_inspector),
        children: vec![Box::new(inspector)],
    };

    // ── Main area: canvas + inspector overlay ──────────────────────────────────
    let main_area = Stack::new()
        .add(Box::new(canvas))
        .add(Box::new(inspector_overlay));

    // ── Backend panel (left side, visible only when show_backend is true) ────────
    let backend_panel_widget = build_backend_panel(
        Arc::clone(&font),
        Rc::clone(&run_mode),
        Rc::clone(&frame_history),
        Rc::clone(&screen_size),
        Rc::clone(&show_inspector),
        renderer_name,
        backend_name,
        || {},
    );
    let backend_pane = BackendPane {
        bounds:   Rect::default(),
        children: vec![backend_panel_widget],
        show:     Rc::clone(&show_backend),
    };

    // ── Demos body: [backend panel] [canvas] [sidebar] — sidebar on RIGHT ─────
    let demos_body = FlexRow::new()
        .with_gap(0.0)
        .add(Box::new(backend_pane))
        .add_flex(Box::new(main_area), 1.0)
        .add(Box::new(sidebar_panel));

    // ── Top bar inner row ─────────────────────────────────────────────────────
    let top_bar_inner = build_top_bar_inner(
        Arc::clone(&font),
        Rc::clone(&show_backend),
        Rc::clone(&theme_pref),
    );

    // ── Root: top menu bar above the demos body ────────────────────────────────
    let root = FlexColumn::new()
        .with_gap(0.0)
        .add(Box::new(TopMenuBar::new(top_bar_inner)))
        .add_flex(Box::new(demos_body), 1.0);

    let mut app = App::new(Box::new(root));

    // ── Global keyboard shortcuts ─────────────────────────────────────────────
    // Ctrl+Shift+O — Organize Windows (tile all visible windows).
    // Ctrl+Shift+R — Reset Memory (resets all open/collapsed window states).
    let on_organize_key = {
        move || {
            for (i, cell) in rc_for_key.iter().enumerate() {
                let r = tile_rect(i, default_canvas_h, specs_w[i], specs_h[i]);
                cell.set(Some(r));
            }
        }
    };
    let demo_open_cells: Vec<Rc<Cell<bool>>> = demo_entries.iter()
        .map(|e| Rc::clone(&e.open))
        .collect();
    let test_open_cells: Vec<Rc<Cell<bool>>> = test_entries.iter()
        .map(|e| Rc::clone(&e.open))
        .collect();

    app.set_global_key_handler({
        let on_org = on_organize_key;
        move |key: Key, mods: Modifiers| {
            if mods.ctrl && mods.shift {
                match key {
                    Key::Char('O') | Key::Char('o') => { on_org(); return true; }
                    Key::Char('R') | Key::Char('r') => {
                        // Reset Memory: close all demo/test windows.
                        for c in &demo_open_cells  { c.set(false); }
                        for c in &test_open_cells  { c.set(false); }
                        return true;
                    }
                    _ => {}
                }
            }
            false
        }
    });

    // ── StateAccessor — collect all shared cells for the platform harness ─────
    let state_accessor = StateAccessor {
        demo_open: demo_entries.iter().map(|e| Rc::clone(&e.open)).collect(),
        demo_pos:  demo_pos_cells,
        test_open: test_entries.iter().map(|e| Rc::clone(&e.open)).collect(),
        test_pos:  test_pos_cells,
        about_open: Rc::clone(&about_open),
        about_pos:  about_pos_cell,
    };

    let handles = DemoHandles {
        show_inspector,
        inspector_nodes,
        hovered_bounds,
        cube_visible,
        screen_size,
        frame_history,
        state: state_accessor,
    };
    (app, handles)
}

// ── Demo content dispatcher ────────────────────────────────────────────────────

fn build_demo_content(title: &str, font: Arc<Font>) -> Box<dyn Widget> {
    match title {
        // basic.rs
        "Code Editor"           => windows::code_editor(font),
        "Sliders"               => windows::sliders(font),
        "TextEdit"              => windows::text_edit(font),
        "Tooltips"              => windows::tooltips(font),
        // code_example.rs
        "Code Example"          => windows::code_example(font),
        // gallery.rs
        "Widget Gallery"        => windows::widget_gallery(font),
        // animation.rs
        "Bézier Curve"          => windows::bezier_curve(font),
        "Dancing Strings"       => windows::dancing_strings(font),
        "Painting"              => windows::painting(font),
        // misc.rs
        "Frame"                 => windows::frame_demo(font),
        "Extra Viewport"        => windows::extra_viewport(font),
        "Highlighting"          => windows::highlighting(font),
        "Interactive Container" => windows::interactive_container(font),
        "Font Book"             => windows::font_book(font),
        "Misc Demos"            => windows::misc_demos(font),
        // interaction.rs
        "Drag and Drop"         => windows::drag_and_drop(font),
        "Scrolling"             => windows::scrolling_demo(font),
        "Panels"                => windows::panels_demo(font),
        "Popups"                => windows::popups_demo(font),
        "Rendering Test"        => rendering_test::rendering_test_view(font),
        "Scene"                 => windows::scene_demo(font),
        "Screenshot"            => windows::screenshot_demo(font),
        // text_demos.rs
        "Strip"                 => windows::strip_demo(font),
        "Table"                 => windows::table_demo(font),
        "Text Layout"           => windows::text_layout(font),
        "Undo Redo"             => windows::undo_redo(font),
        "Window Options"        => windows::window_options(font),
        "Modals"                => windows::modals_demo(font),
        "Multi Touch"           => windows::multi_touch(font),
        _                       => windows::coming_soon(),
    }
}
