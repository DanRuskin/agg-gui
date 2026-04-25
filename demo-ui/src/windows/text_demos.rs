//! Text-related and layout demo windows: scrolling rows, strip layout, table,
//! text layout showcase, undo/redo, window options, modals, and multi-touch info.
//!
//! Most demos here are purely compositional — they build a widget tree from
//! `FlexColumn`, `FlexRow`, `Container`, `Label`, etc. without custom painting.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widget::paint_subtree;
use agg_gui::{
    measure_text_metrics, Button, Checkbox, Color, Container, DragValue, DrawCtx, Event,
    EventResult, FlexColumn, FlexRow, Font, Label, LabelAlign, MouseButton, Point, Rect,
    ScrollView, Separator, Size, SizedBox, TextField, Widget,
};

// ---------------------------------------------------------------------------
// Strip demo
// ---------------------------------------------------------------------------

/// A fixed-width labeled box used to visualise "strip" regions.
///
/// Text is rendered through a backbuffered Label child so the glyph rasterization
/// is cached to a framebuffer rather than repeated each frame.
struct StripCell {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    label_widget: Label,
    bg: Color,
    w: f64,
    h: f64,
}

impl StripCell {
    fn new(label: impl Into<String>, font: Arc<Font>, bg: Color, w: f64, h: f64) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            label_widget: Label::new(label, font).with_font_size(11.0),
            bg,
            w,
            h,
        }
    }
}

impl Widget for StripCell {
    fn type_name(&self) -> &'static str {
        "StripCell"
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

    fn layout(&mut self, _available: Size) -> Size {
        self.bounds = Rect::new(0.0, 0.0, self.w, self.h);
        // Position the label at 4px from the left, vertically centered.
        let ls = self.label_widget.layout(Size::new(self.w - 8.0, self.h));
        let ly = (self.h - ls.height) * 0.5;
        self.label_widget
            .set_bounds(Rect::new(4.0, ly, ls.width, ls.height));
        Size::new(self.w, self.h)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        ctx.set_fill_color(self.bg);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, self.w, self.h);
        ctx.fill();
        ctx.set_stroke_color(v.widget_stroke);
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, self.w, self.h);
        ctx.stroke();

        // Paint label via backbuffered child.
        self.label_widget.set_color(v.text_color);
        let lb = self.label_widget.bounds();
        ctx.save();
        ctx.translate(lb.x, lb.y);
        paint_subtree(&mut self.label_widget, ctx);
        ctx.restore();
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

/// Build the Strip demo — a horizontal row of fixed-width strips, then a
/// vertical column of fixed-height strips.
pub fn strip_demo(font: Arc<Font>) -> Box<dyn Widget> {
    let mut outer = FlexColumn::new()
        .with_gap(16.0)
        .with_padding(14.0)
        .with_panel_bg();

    outer.push(
        Box::new(Label::new("Horizontal strips", Arc::clone(&font)).with_font_size(12.0)),
        0.0,
    );

    let colors_h = [
        Color::rgba(0.22, 0.45, 0.88, 0.18),
        Color::rgba(0.18, 0.72, 0.42, 0.18),
        Color::rgba(0.88, 0.25, 0.18, 0.18),
        Color::rgba(0.86, 0.78, 0.40, 0.18),
        Color::rgba(0.60, 0.25, 0.88, 0.18),
    ];
    let mut h_row = FlexRow::new().with_gap(4.0);
    for (i, &bg) in colors_h.iter().enumerate() {
        h_row.push(
            Box::new(StripCell::new(
                format!("S{}", i + 1),
                Arc::clone(&font),
                bg,
                55.0,
                40.0,
            )),
            0.0,
        );
    }
    outer.push(Box::new(h_row), 0.0);

    outer.push(Box::new(Separator::horizontal()), 0.0);
    outer.push(
        Box::new(Label::new("Vertical strips", Arc::clone(&font)).with_font_size(12.0)),
        0.0,
    );

    let colors_v = [
        Color::rgba(0.22, 0.65, 0.88, 0.18),
        Color::rgba(0.88, 0.55, 0.15, 0.18),
        Color::rgba(0.88, 0.25, 0.65, 0.18),
        Color::rgba(0.50, 0.50, 0.50, 0.18),
    ];
    let mut v_col = FlexColumn::new().with_gap(4.0);
    for (i, &bg) in colors_v.iter().enumerate() {
        v_col.push(
            Box::new(StripCell::new(
                format!("Strip {}", i + 1),
                Arc::clone(&font),
                bg,
                200.0,
                32.0,
            )),
            0.0,
        );
    }
    outer.push(Box::new(v_col), 0.0);

    outer.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    Box::new(outer)
}

// ---------------------------------------------------------------------------
// Table demo
// ---------------------------------------------------------------------------

/// Build the Table demo — a header row and 8 data rows with alternating colors.
pub fn table_demo(font: Arc<Font>) -> Box<dyn Widget> {
    let mut outer = FlexColumn::new()
        .with_gap(10.0)
        .with_padding(12.0)
        .with_panel_bg();

    outer.push(
        Box::new(Label::new("Simple data table", Arc::clone(&font)).with_font_size(12.0)),
        0.0,
    );

    // Column widths.
    let col_w = [55.0_f64, 90.0, 70.0, 55.0];
    let headers = ["#", "Name", "Value", "Status"];

    // Header row.
    let mut header_row = FlexRow::new().with_gap(0.0);
    for (i, &hdr) in headers.iter().enumerate() {
        let cell = Container::new()
            .with_background(Color::rgba(0.0, 0.0, 0.0, 0.10))
            .with_border(Color::rgba(0.0, 0.0, 0.0, 0.15), 1.0)
            .with_padding(5.0)
            .add(Box::new(SizedBox::new().with_width(col_w[i]).with_child(
                Box::new(Label::new(hdr, Arc::clone(&font)).with_font_size(11.5)),
            )));
        header_row.push(Box::new(cell), 0.0);
    }
    outer.push(Box::new(header_row), 0.0);

    // Data rows.
    let data = [
        ("1", "Alpha", "0.92", "OK"),
        ("2", "Beta", "1.44", "OK"),
        ("3", "Gamma", "0.07", "Warn"),
        ("4", "Delta", "3.14", "OK"),
        ("5", "Epsilon", "2.72", "OK"),
        ("6", "Zeta", "0.00", "Error"),
        ("7", "Eta", "9.81", "OK"),
        ("8", "Theta", "1.618", "OK"),
    ];
    for (row_i, &(n, name, val, status)) in data.iter().enumerate() {
        let bg = if row_i % 2 == 0 {
            Color::rgba(0.0, 0.0, 0.0, 0.03)
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        };
        let cells_text = [n, name, val, status];
        let mut data_row = FlexRow::new().with_gap(0.0);
        for (ci, &text) in cells_text.iter().enumerate() {
            let cell = Container::new()
                .with_background(bg)
                .with_border(Color::rgba(0.0, 0.0, 0.0, 0.08), 1.0)
                .with_padding(5.0)
                .add(Box::new(SizedBox::new().with_width(col_w[ci]).with_child(
                    Box::new(Label::new(text, Arc::clone(&font)).with_font_size(12.0)),
                )));
            data_row.push(Box::new(cell), 0.0);
        }
        outer.push(Box::new(data_row), 0.0);
    }

    outer.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    Box::new(ScrollView::new(Box::new(outer)))
}

// ---------------------------------------------------------------------------
// Text Layout demo
// ---------------------------------------------------------------------------

const TEXT_LAYOUT_LOREM_IPSUM_LONG: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing \
elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure \
dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur \
sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

/// Excerpt from Dolores Ibarruri's farewell speech to the International Brigades.
const TEXT_LAYOUT_LA_PASIONARIA: &str = "Mothers! Women!\n\
\n\
When the years pass by and the wounds of war are stanched; when the memory of the sad and bloody \
days dissipates in a present of liberty, of peace and of wellbeing; when the rancor have died out \
and pride in a free country is felt equally by all Spaniards, speak to your children. Tell them of \
these men of the International Brigades.\n\
\n\
Recount for them how, coming over seas and mountains, crossing frontiers bristling with bayonets, \
sought by raving dogs thirsting to tear their flesh, these men reached our country as crusaders for \
freedom, to fight and die for Spain's liberty and independence threatened by German and Italian \
fascism. They gave up everything - their loves, their countries, home and fortune, fathers, mothers, \
wives, brothers, sisters and children - and they came and said to us: \"We are here. Your cause, \
Spain's cause, is ours. It is the cause of all advanced and progressive mankind.\"\n\
\n\
- Dolores Ibarruri, 1938";

struct TextLayoutDemoState {
    max_rows: Rc<Cell<usize>>,
    break_mode: Rc<Cell<usize>>,
    overflow: Rc<Cell<usize>>,
    extra_letter_spacing: Rc<Cell<f64>>,
    custom_line_height: Rc<Cell<bool>>,
    line_height_pixels: Rc<Cell<f64>>,
    halign: Rc<Cell<usize>>,
    justify: Rc<Cell<bool>>,
    text_source: Rc<Cell<usize>>,
}

impl TextLayoutDemoState {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            max_rows: Rc::new(Cell::new(1000)),
            break_mode: Rc::new(Cell::new(0)),
            overflow: Rc::new(Cell::new(1)),
            extra_letter_spacing: Rc::new(Cell::new(0.0)),
            custom_line_height: Rc::new(Cell::new(false)),
            line_height_pixels: Rc::new(Cell::new(20.0)),
            halign: Rc::new(Cell::new(0)),
            justify: Rc::new(Cell::new(false)),
            text_source: Rc::new(Cell::new(0)),
        })
    }

    fn overflow_char(&self) -> Option<char> {
        match self.overflow.get() {
            1 => Some('…'),
            2 => Some('—'),
            3 => Some('-'),
            _ => None,
        }
    }

    fn align(&self) -> LabelAlign {
        match self.halign.get() {
            1 => LabelAlign::Center,
            2 => LabelAlign::Right,
            _ => LabelAlign::Left,
        }
    }

    fn text(&self) -> &'static str {
        if self.text_source.get() == 0 {
            TEXT_LAYOUT_LOREM_IPSUM_LONG
        } else {
            TEXT_LAYOUT_LA_PASIONARIA
        }
    }
}

#[derive(Clone)]
struct TextLayoutLine {
    text: String,
    paragraph_end: bool,
}

struct TextLayoutPreview {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    font: Arc<Font>,
    state: Rc<TextLayoutDemoState>,
    lines: Vec<TextLayoutLine>,
    line_h: f64,
    content_w: f64,
}

impl TextLayoutPreview {
    fn new(font: Arc<Font>, state: Rc<TextLayoutDemoState>) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            font,
            state,
            lines: Vec::new(),
            line_h: 18.0,
            content_w: 0.0,
        }
    }

    fn font_size(&self) -> f64 {
        13.0
    }

    fn line_width(&self, text: &str, extra_spacing: f64) -> f64 {
        let gaps = text.chars().count().saturating_sub(1) as f64;
        measure_text_metrics(&self.font, text, self.font_size()).width + gaps * extra_spacing
    }

    fn push_word_wrapped(
        &self,
        paragraph: &str,
        max_width: f64,
        extra_spacing: f64,
        out: &mut Vec<TextLayoutLine>,
    ) {
        if paragraph.is_empty() {
            out.push(TextLayoutLine {
                text: String::new(),
                paragraph_end: true,
            });
            return;
        }

        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || self.line_width(&candidate, extra_spacing) <= max_width {
                current = candidate;
            } else {
                out.push(TextLayoutLine {
                    text: std::mem::replace(&mut current, word.to_string()),
                    paragraph_end: false,
                });
            }
        }

        out.push(TextLayoutLine {
            text: current,
            paragraph_end: true,
        });
    }

    fn push_anywhere_wrapped(
        &self,
        paragraph: &str,
        max_width: f64,
        extra_spacing: f64,
        out: &mut Vec<TextLayoutLine>,
    ) {
        if paragraph.is_empty() {
            out.push(TextLayoutLine {
                text: String::new(),
                paragraph_end: true,
            });
            return;
        }

        let mut current = String::new();
        for ch in paragraph.chars() {
            let candidate = format!("{current}{ch}");
            if !current.is_empty() && self.line_width(&candidate, extra_spacing) > max_width {
                out.push(TextLayoutLine {
                    text: std::mem::replace(&mut current, ch.to_string()),
                    paragraph_end: false,
                });
            } else {
                current = candidate;
            }
        }

        out.push(TextLayoutLine {
            text: current,
            paragraph_end: true,
        });
    }

    fn append_overflow(&self, line: &mut String, max_width: f64, extra_spacing: f64) {
        let Some(ch) = self.state.overflow_char() else {
            return;
        };
        let marker = ch.to_string();
        while !line.is_empty()
            && self.line_width(&format!("{line}{marker}"), extra_spacing) > max_width
        {
            line.pop();
        }
        line.push(ch);
    }

    fn rebuild_lines(&mut self, available_w: f64) {
        let extra_spacing = self.state.extra_letter_spacing.get();
        let max_width = available_w.max(1.0);
        let mut lines = Vec::new();
        for paragraph in self.state.text().split('\n') {
            if self.state.break_mode.get() == 1 {
                self.push_anywhere_wrapped(paragraph, max_width, extra_spacing, &mut lines);
            } else {
                self.push_word_wrapped(paragraph, max_width, extra_spacing, &mut lines);
            }
        }

        let max_rows = self.state.max_rows.get();
        if lines.len() > max_rows {
            lines.truncate(max_rows);
            if let Some(last) = lines.last_mut() {
                self.append_overflow(&mut last.text, max_width, extra_spacing);
                last.paragraph_end = true;
            }
        }

        self.lines = lines;
        self.content_w = max_width;
        self.line_h = if self.state.custom_line_height.get() {
            self.state.line_height_pixels.get().max(8.0)
        } else {
            self.font_size() * 1.35
        };
    }

    fn paint_spaced_line(
        &self,
        ctx: &mut dyn DrawCtx,
        line: &str,
        x: f64,
        y: f64,
        extra_spacing: f64,
        justify_spacing: f64,
    ) {
        let mut cursor_x = x;
        for ch in line.chars() {
            let s = ch.to_string();
            ctx.fill_text(&s, cursor_x, y);
            let w = ctx.measure_text(&s).map(|m| m.width).unwrap_or(0.0);
            cursor_x += w + extra_spacing;
            if ch.is_whitespace() {
                cursor_x += justify_spacing;
            }
        }
    }
}

impl Widget for TextLayoutPreview {
    fn type_name(&self) -> &'static str {
        "TextLayoutPreview"
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
        let content_w = (available.width - 24.0).max(1.0);
        self.rebuild_lines(content_w);
        let content_h = self.lines.len().max(1) as f64 * self.line_h;
        Size::new(available.width, content_h + 24.0)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let w = self.bounds.width;
        let h = self.bounds.height;
        let v = ctx.visuals();
        let pad = 12.0;

        ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.06));
        ctx.begin_path();
        ctx.rect(0.0, 0.0, w, h);
        ctx.fill();
        ctx.set_stroke_color(v.widget_stroke);
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.rect(0.5, 0.5, (w - 1.0).max(0.0), (h - 1.0).max(0.0));
        ctx.stroke();

        ctx.save();
        ctx.clip_rect(pad, pad, self.content_w, (h - pad * 2.0).max(0.0));
        ctx.set_font(Arc::clone(&self.font));
        ctx.set_font_size(self.font_size());
        ctx.set_fill_color(v.text_color);

        let extra_spacing = self.state.extra_letter_spacing.get();
        let align = self.state.align();
        let justify = self.state.justify.get();
        let total_text_h = self.lines.len() as f64 * self.line_h;
        let mut y = h - pad - self.line_h * 0.5 - (self.line_h - self.font_size()) * 0.35;

        for (i, line) in self.lines.iter().enumerate() {
            if !line.text.is_empty() {
                let line_w = self.line_width(&line.text, extra_spacing);
                let is_last = i + 1 == self.lines.len();
                let should_justify = justify && !line.paragraph_end && !is_last;
                let spaces = line.text.chars().filter(|c| c.is_whitespace()).count();
                let justify_spacing = if should_justify && spaces > 0 {
                    ((self.content_w - line_w) / spaces as f64).max(0.0)
                } else {
                    0.0
                };
                let draw_w = if should_justify {
                    self.content_w
                } else {
                    line_w
                };
                let x = match align {
                    LabelAlign::Center => pad + (self.content_w - draw_w) * 0.5,
                    LabelAlign::Right => pad + self.content_w - draw_w,
                    LabelAlign::Left => pad,
                };

                if extra_spacing.abs() > 0.01 || justify_spacing > 0.0 {
                    self.paint_spaced_line(ctx, &line.text, x, y, extra_spacing, justify_spacing);
                } else {
                    ctx.fill_text(&line.text, x, y);
                }
            }
            y -= self.line_h;
            if h - y > total_text_h + pad {
                break;
            }
        }

        ctx.restore();
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

struct SelectionButtons {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    labels: Vec<String>,
    selected: Rc<Cell<usize>>,
    hovered: Option<usize>,
    font_size: f64,
    label_widgets: Vec<Label>,
}

impl SelectionButtons {
    fn new(options: Vec<impl Into<String>>, selected: Rc<Cell<usize>>, font: Arc<Font>) -> Self {
        let labels: Vec<String> = options.into_iter().map(|s| s.into()).collect();
        let font_size = 12.0;
        let label_widgets = labels
            .iter()
            .map(|text| {
                Label::new(text.as_str(), Arc::clone(&font))
                    .with_font_size(font_size)
                    .with_align(LabelAlign::Center)
            })
            .collect();
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            labels,
            selected,
            hovered: None,
            font_size,
            label_widgets,
        }
    }

    fn button_h(&self) -> f64 {
        (self.font_size * 1.7).max(24.0)
    }

    fn index_at(&self, p: Point) -> Option<usize> {
        if self.labels.is_empty()
            || p.x < 0.0
            || p.y < 0.0
            || p.x > self.bounds.width
            || p.y > self.bounds.height
        {
            return None;
        }
        let cell_w = self.bounds.width / self.labels.len() as f64;
        Some(((p.x / cell_w).floor() as usize).min(self.labels.len() - 1))
    }
}

impl Widget for SelectionButtons {
    fn type_name(&self) -> &'static str {
        "SelectionButtons"
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        let h = self.button_h();
        let w = available.width;
        self.bounds = Rect::new(0.0, 0.0, w, h);
        if !self.labels.is_empty() {
            let cell_w = w / self.labels.len() as f64;
            for label in &mut self.label_widgets {
                label.layout(Size::new(cell_w, h));
                label.set_bounds(Rect::new(0.0, 0.0, cell_w, h));
            }
        }
        Size::new(w, h)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        if self.labels.is_empty() {
            return;
        }

        let v = ctx.visuals();
        let n = self.labels.len();
        let cell_w = self.bounds.width / n as f64;
        let h = self.bounds.height;
        let selected = self.selected.get().min(n - 1);

        for i in 0..n {
            let x = i as f64 * cell_w;
            let is_selected = i == selected;
            let is_hovered = self.hovered == Some(i);
            let bg = if is_selected {
                v.accent
            } else if is_hovered {
                v.widget_bg_hovered
            } else {
                v.widget_bg
            };
            let text = if is_selected {
                Color::white()
            } else {
                v.text_color
            };

            ctx.set_fill_color(bg);
            ctx.begin_path();
            ctx.rounded_rect(x, 0.0, cell_w, h, 4.0);
            ctx.fill();

            ctx.set_stroke_color(v.widget_stroke);
            ctx.set_line_width(1.0);
            ctx.begin_path();
            ctx.rounded_rect(
                x + 0.5,
                0.5,
                (cell_w - 1.0).max(0.0),
                (h - 1.0).max(0.0),
                4.0,
            );
            ctx.stroke();

            self.label_widgets[i].set_color(text);
            let lb = self.label_widgets[i].bounds();
            ctx.save();
            ctx.translate(x + (cell_w - lb.width) * 0.5, (h - lb.height) * 0.5);
            paint_subtree(&mut self.label_widgets[i], ctx);
            ctx.restore();
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { pos } => {
                let was = self.hovered;
                self.hovered = self.index_at(*pos);
                if was != self.hovered {
                    agg_gui::animation::request_tick();
                }
                EventResult::Ignored
            }
            Event::MouseDown {
                button: MouseButton::Left,
                pos,
                ..
            } => {
                if let Some(i) = self.index_at(*pos) {
                    if self.selected.get() != i {
                        self.selected.set(i);
                        agg_gui::animation::request_tick();
                    }
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }
}

fn text_layout_control_row(
    label: &'static str,
    control: Box<dyn Widget>,
    font: Arc<Font>,
) -> Box<dyn Widget> {
    Box::new(
        FlexRow::new()
            .with_gap(10.0)
            .add(Box::new(
                Label::new(label, Arc::clone(&font))
                    .with_font_size(12.0)
                    .with_max_size(Size::new(130.0, f64::MAX))
                    .with_min_size(Size::new(130.0, 0.0)),
            ))
            .add_flex(control, 1.0),
    )
}

/// Build the Text Layout demo — mirrors egui's LayoutJob playground with live
/// controls for wrapping, elision, spacing, line height, alignment, and text.
pub fn text_layout(font: Arc<Font>) -> Box<dyn Widget> {
    let state = TextLayoutDemoState::new();
    let mut col = FlexColumn::new()
        .with_gap(8.0)
        .with_padding(14.0)
        .with_panel_bg();

    col.push(
        Box::new(
            Label::new("Text layout", Arc::clone(&font))
                .with_font_size(18.0)
                .with_color(Color::rgb(0.22, 0.45, 0.88)),
        ),
        0.0,
    );
    col.push(
        Box::new(
            Label::new(
                "A live LayoutJob-style playground modeled on egui's Text Layout demo.",
                Arc::clone(&font),
            )
            .with_font_size(12.0)
            .with_wrap(true),
        ),
        0.0,
    );

    col.push(Box::new(Separator::horizontal()), 0.0);

    {
        let cell = Rc::clone(&state.max_rows);
        col.push(
            text_layout_control_row(
                "Max rows:",
                Box::new(
                    DragValue::new(cell.get() as f64, 0.0, 1000.0, Arc::clone(&font))
                        .with_decimals(0)
                        .with_speed(1.0)
                        .on_change(move |v| cell.set(v.round().max(0.0) as usize)),
                ),
                Arc::clone(&font),
            ),
            0.0,
        );
    }

    col.push(
        text_layout_control_row(
            "Line-break:",
            Box::new(SelectionButtons::new(
                vec!["Word boundaries", "Anywhere"],
                Rc::clone(&state.break_mode),
                Arc::clone(&font),
            )),
            Arc::clone(&font),
        ),
        0.0,
    );

    col.push(
        text_layout_control_row(
            "Overflow character:",
            Box::new(SelectionButtons::new(
                vec!["None", "…", "—", " - "],
                Rc::clone(&state.overflow),
                Arc::clone(&font),
            )),
            Arc::clone(&font),
        ),
        0.0,
    );

    {
        let cell = Rc::clone(&state.extra_letter_spacing);
        col.push(
            text_layout_control_row(
                "Extra letter spacing:",
                Box::new(
                    DragValue::new(cell.get(), -5.0, 20.0, Arc::clone(&font))
                        .with_decimals(1)
                        .with_speed(0.1)
                        .on_change(move |v| cell.set(v)),
                ),
                Arc::clone(&font),
            ),
            0.0,
        );
    }

    let mut line_height_row = FlexRow::new().with_gap(10.0);
    line_height_row.push(
        Box::new(
            Checkbox::new("Custom", Arc::clone(&font), state.custom_line_height.get())
                .with_font_size(12.0)
                .with_state_cell(Rc::clone(&state.custom_line_height)),
        ),
        0.0,
    );
    {
        let cell = Rc::clone(&state.line_height_pixels);
        line_height_row.push(
            Box::new(
                DragValue::new(cell.get(), 8.0, 64.0, Arc::clone(&font))
                    .with_decimals(0)
                    .with_speed(1.0)
                    .on_change(move |v| cell.set(v.round().max(8.0))),
            ),
            1.0,
        );
    }
    col.push(
        text_layout_control_row("Line height:", Box::new(line_height_row), Arc::clone(&font)),
        0.0,
    );

    col.push(
        text_layout_control_row(
            "Horizontal align:",
            Box::new(SelectionButtons::new(
                vec!["Left", "Center", "Right"],
                Rc::clone(&state.halign),
                Arc::clone(&font),
            )),
            Arc::clone(&font),
        ),
        0.0,
    );

    col.push(
        text_layout_control_row(
            "Justify:",
            Box::new(
                Checkbox::new("Fill row width", Arc::clone(&font), state.justify.get())
                    .with_font_size(12.0)
                    .with_state_cell(Rc::clone(&state.justify)),
            ),
            Arc::clone(&font),
        ),
        0.0,
    );

    col.push(
        text_layout_control_row(
            "Text:",
            Box::new(SelectionButtons::new(
                vec!["Lorem Ipsum", "La Pasionaria"],
                Rc::clone(&state.text_source),
                Arc::clone(&font),
            )),
            Arc::clone(&font),
        ),
        0.0,
    );

    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(
        Box::new(TextLayoutPreview::new(Arc::clone(&font), state)),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    Box::new(ScrollView::new(Box::new(col)))
}

// ---------------------------------------------------------------------------
// Undo Redo demo
// ---------------------------------------------------------------------------

/// Build the Undo Redo demo — a TextField plus usage instructions.
/// (TextField manages its own internal undo history via Ctrl+Z / Ctrl+Y.)
pub fn undo_redo(font: Arc<Font>) -> Box<dyn Widget> {
    let mut col = FlexColumn::new()
        .with_gap(14.0)
        .with_padding(16.0)
        .with_panel_bg();

    col.push(
        Box::new(Label::new("Text field with undo/redo", Arc::clone(&font)).with_font_size(12.0)),
        0.0,
    );

    col.push(
        Box::new(
            SizedBox::new().with_height(34.0).with_child(Box::new(
                TextField::new(Arc::clone(&font))
                    .with_font_size(13.0)
                    .with_text("Edit me — then Ctrl+Z to undo"),
            )),
        ),
        0.0,
    );

    col.push(Box::new(Separator::horizontal()), 0.0);

    col.push(
        Box::new(Label::new("Keyboard shortcuts:", Arc::clone(&font)).with_font_size(12.0)),
        0.0,
    );

    for line in [
        "Ctrl+Z         — undo last edit",
        "Ctrl+Y         — redo",
        "Ctrl+Shift+Z   — redo (alternate)",
        "Ctrl+A         — select all",
        "Ctrl+C / X / V — clipboard",
    ] {
        col.push(
            Box::new(Label::new(line, Arc::clone(&font)).with_font_size(12.0)),
            0.0,
        );
    }

    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(
        Box::new(
            Label::new(
                "Each character insertion/deletion is recorded in the TextField's internal \
         UndoBuffer. Undo collapses runs of single-character edits into a single step.",
                Arc::clone(&font),
            )
            .with_font_size(11.0),
        ),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    Box::new(col)
}

// ---------------------------------------------------------------------------
// Window Options demo
// ---------------------------------------------------------------------------

/// Build the Window Options demo — checkboxes reflecting window capabilities.
pub fn window_options(font: Arc<Font>) -> Box<dyn Widget> {
    let resizable = Rc::new(Cell::new(true));
    let collapsible = Rc::new(Cell::new(true));
    let auto_sized = Rc::new(Cell::new(false));
    let anchored = Rc::new(Cell::new(false));

    let mut col = FlexColumn::new()
        .with_gap(14.0)
        .with_padding(16.0)
        .with_panel_bg();

    col.push(
        Box::new(Label::new("Window options", Arc::clone(&font)).with_font_size(12.0)),
        0.0,
    );

    {
        let v = Rc::clone(&resizable);
        col.push(
            Box::new(
                Checkbox::new("Resizable", Arc::clone(&font), resizable.get())
                    .with_font_size(13.0)
                    .on_change(move |b| v.set(b)),
            ),
            0.0,
        );
    }
    {
        let v = Rc::clone(&collapsible);
        col.push(
            Box::new(
                Checkbox::new("Collapsible", Arc::clone(&font), collapsible.get())
                    .with_font_size(13.0)
                    .on_change(move |b| v.set(b)),
            ),
            0.0,
        );
    }
    {
        let v = Rc::clone(&auto_sized);
        col.push(
            Box::new(
                Checkbox::new("Auto-sized", Arc::clone(&font), auto_sized.get())
                    .with_font_size(13.0)
                    .on_change(move |b| v.set(b)),
            ),
            0.0,
        );
    }
    {
        let v = Rc::clone(&anchored);
        col.push(
            Box::new(
                Checkbox::new("Anchored", Arc::clone(&font), anchored.get())
                    .with_font_size(13.0)
                    .on_change(move |b| v.set(b)),
            ),
            0.0,
        );
    }

    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(
        Box::new(
            Label::new("Current window size: 360 \u{00d7} 290", Arc::clone(&font))
                .with_font_size(12.0),
        ),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    Box::new(col)
}

// ---------------------------------------------------------------------------
// Modals demo
// ---------------------------------------------------------------------------

/// Inline modal overlay: shown/hidden by the `open` cell.
///
/// Text is rendered through backbuffered Label children so glyph rasterization
/// is cached rather than repeated each frame.
struct ModalOverlay {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    open: Rc<Cell<bool>>,
    lbl_title: Label,
    lbl_body: Label,
    lbl_dismiss: Label,
}

impl ModalOverlay {
    fn new(font: Arc<Font>, open: Rc<Cell<bool>>) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            open,
            lbl_title: Label::new("Modal dialog", Arc::clone(&font)).with_font_size(13.0),
            lbl_body: Label::new(
                "This is a modal. Click anywhere to dismiss.",
                Arc::clone(&font),
            )
            .with_font_size(11.5),
            lbl_dismiss: Label::new("[ Dismiss ]", Arc::clone(&font)).with_font_size(11.0),
        }
    }
}

impl Widget for ModalOverlay {
    fn type_name(&self) -> &'static str {
        "ModalOverlay"
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
        if !self.open.get() {
            self.bounds = Rect::new(0.0, 0.0, 0.0, 0.0);
            return Size::new(0.0, 0.0);
        }
        let h = 120.0_f64;
        let w = available.width;
        self.bounds = Rect::new(0.0, 0.0, w, h);

        // Dialog dimensions (computed same as paint).
        let dw = w.min(280.0);
        let dh = 90.0_f64;
        let dx = (w - dw) * 0.5;
        let dy = (h - dh) * 0.5;
        let inner_w = dw - 20.0;

        let ts = self.lbl_title.layout(Size::new(inner_w, 20.0));
        self.lbl_title.set_bounds(Rect::new(
            dx + 10.0,
            dy + dh - ts.height - 10.0,
            ts.width,
            ts.height,
        ));

        let bs = self.lbl_body.layout(Size::new(inner_w, 18.0));
        self.lbl_body.set_bounds(Rect::new(
            dx + 10.0,
            dy + dh - ts.height - bs.height - 18.0,
            bs.width,
            bs.height,
        ));

        let ds = self.lbl_dismiss.layout(Size::new(inner_w, 18.0));
        self.lbl_dismiss.set_bounds(Rect::new(
            dx + 10.0,
            dy + dh - ts.height - bs.height - ds.height - 26.0,
            ds.width,
            ds.height,
        ));

        Size::new(w, h)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        if !self.open.get() {
            return;
        }
        let v = ctx.visuals();
        let w = self.bounds.width;
        let h = self.bounds.height;

        // Semi-transparent overlay.
        ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.35));
        ctx.begin_path();
        ctx.rect(0.0, 0.0, w, h);
        ctx.fill();

        // Dialog box.
        let dw = w.min(280.0);
        let dh = 90.0_f64;
        let dx = (w - dw) * 0.5;
        let dy = (h - dh) * 0.5;
        ctx.set_fill_color(v.window_fill);
        ctx.begin_path();
        ctx.rounded_rect(dx, dy, dw, dh, 8.0);
        ctx.fill();
        ctx.set_stroke_color(v.widget_stroke);
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.rounded_rect(dx, dy, dw, dh, 8.0);
        ctx.stroke();

        // Paint labels via backbuffered children.
        self.lbl_title.set_color(v.text_color);
        let tb = self.lbl_title.bounds();
        ctx.save();
        ctx.translate(tb.x, tb.y);
        paint_subtree(&mut self.lbl_title, ctx);
        ctx.restore();

        self.lbl_body.set_color(v.text_dim);
        let bb = self.lbl_body.bounds();
        ctx.save();
        ctx.translate(bb.x, bb.y);
        paint_subtree(&mut self.lbl_body, ctx);
        ctx.restore();

        self.lbl_dismiss.set_color(v.accent);
        let db = self.lbl_dismiss.bounds();
        ctx.save();
        ctx.translate(db.x, db.y);
        paint_subtree(&mut self.lbl_dismiss, ctx);
        ctx.restore();
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if !self.open.get() {
            return EventResult::Ignored;
        }
        // Click anywhere dismisses.
        if let Event::MouseDown {
            button: MouseButton::Left,
            ..
        } = event
        {
            self.open.set(false);
            return EventResult::Consumed;
        }
        EventResult::Ignored
    }

    fn hit_test(&self, p: Point) -> bool {
        self.open.get()
            && p.x >= 0.0
            && p.x <= self.bounds.width
            && p.y >= 0.0
            && p.y <= self.bounds.height
    }
}

/// Build the Modals demo — a button that shows an inline modal overlay.
pub fn modals_demo(font: Arc<Font>) -> Box<dyn Widget> {
    let open = Rc::new(Cell::new(false));

    let mut col = FlexColumn::new()
        .with_gap(12.0)
        .with_padding(14.0)
        .with_panel_bg();

    col.push(
        Box::new(Label::new("Modals demo", Arc::clone(&font)).with_font_size(12.0)),
        0.0,
    );

    {
        let open_for_btn = Rc::clone(&open);
        col.push(
            Box::new(
                SizedBox::new().with_height(30.0).with_child(Box::new(
                    Button::new("Open modal", Arc::clone(&font))
                        .with_font_size(13.0)
                        .on_click(move || {
                            open_for_btn.set(true);
                        }),
                )),
            ),
            0.0,
        );
    }

    col.push(
        Box::new(ModalOverlay::new(Arc::clone(&font), Rc::clone(&open))),
        0.0,
    );

    col.push(
        Box::new(
            Label::new(
                "Click 'Open modal' to show the dialog. Click anywhere in it to dismiss.",
                Arc::clone(&font),
            )
            .with_font_size(11.0),
        ),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    Box::new(col)
}

// ---------------------------------------------------------------------------
// Multi Touch demo
// ---------------------------------------------------------------------------
//
// Port of egui's `multi_touch.rs` demo.  Layout + interaction + the
// decaying-arrow trick all match the original as closely as the
// coordinate-system flip allows.  The big visible difference vs. egui
// is Y-up: egui draws the arrow from (-0.5, 0.5) to (0.5, -0.5) in its
// Y-down normalised space, which reads visually as bottom-left →
// top-right; in our Y-up space that same visual is (-0.5, -0.5) to
// (0.5, 0.5).  Everything else — normalised ±1 canvas with square
// proportions, zoom/rotate/translate accumulators, pressure-driven
// stroke width, and the half-life reset animation — is the same.

/// Accumulated zoom / rotation / translation state for the arrow.
/// Mirrors the fields on egui's `MultiTouch` struct.
struct MultiTouchView {
    bounds: agg_gui::Rect,
    children: Vec<Box<dyn Widget>>,
    /// Multiplicative zoom; starts at 1.0 and pinch deltas multiply in.
    zoom: f64,
    /// Rotation in radians (Y-up CCW).
    rotation: f64,
    /// Translation in NORMALISED units (i.e. `pixels / scale`), so the
    /// arrow tracks the pinch midpoint regardless of widget size — this
    /// is what egui does via `to_screen.inverse().scale() * delta`.
    translation_x: f64,
    translation_y: f64,
    /// Timestamp of the most recent frame that saw a touch gesture.
    /// The reset animation keys off `(now - last_touch_time)`.
    last_touch_time: Option<web_time::Instant>,
    /// Previous frame's instant — used to derive `dt` for the half-life
    /// decay.  `None` until after the first paint.
    prev_frame_time: Option<web_time::Instant>,
    /// Latest frame's force reading (0.0 when unsupported), used to
    /// thicken the stroke.
    force: f32,
    /// Latest frame's finger count.  Surfaced through the status label.
    num_touches: usize,
}

impl MultiTouchView {
    fn new() -> Self {
        Self {
            bounds: agg_gui::Rect::default(),
            children: Vec::new(),
            zoom: 1.0,
            rotation: 0.0,
            translation_x: 0.0,
            translation_y: 0.0,
            last_touch_time: None,
            prev_frame_time: None,
            force: 0.0,
            num_touches: 0,
        }
    }

    /// Uniform pixels-per-normalised-unit scale, matching egui's
    /// `to_screen.scale()`.  The shorter widget axis maps to ±1.
    fn unit_scale(&self) -> f64 {
        self.bounds.width.min(self.bounds.height) * 0.5
    }

    /// Smoothly drift zoom / rotation / translation back toward identity
    /// once the user lifts their fingers.  Same curve as egui: hold for
    /// 0.5 s, then an exponential half-life decay whose time-constant
    /// itself ramps down over the next 0.5 s.
    fn slowly_reset(&mut self, now: web_time::Instant, dt: f64) -> bool {
        let last = match self.last_touch_time {
            Some(t) => t,
            None => return false,
        };
        let time_since_last = now.duration_since(last).as_secs_f64();
        let delay = 0.5_f64;
        if time_since_last < delay {
            return true; // keep ticking, don't change values yet
        }
        // `remap_clamp(time_since_last, 0.5..=1.0, 1.0..=0.0)` from egui.
        let t = ((time_since_last - delay) / (1.0 - delay)).clamp(0.0, 1.0);
        let half_life = (1.0 - t).powi(4);
        if half_life <= 1e-3 {
            self.zoom = 1.0;
            self.rotation = 0.0;
            self.translation_x = 0.0;
            self.translation_y = 0.0;
            return false;
        }
        // dt is the wall-clock delta between frames.
        let factor = (-(2_f64.ln()) / half_life * dt).exp();
        self.zoom = 1.0 + (self.zoom - 1.0) * factor;
        self.rotation *= factor;
        self.translation_x *= factor;
        self.translation_y *= factor;
        true
    }
}

impl Widget for MultiTouchView {
    fn type_name(&self) -> &'static str {
        "MultiTouchView"
    }
    fn bounds(&self) -> agg_gui::Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: agg_gui::Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: agg_gui::Size) -> agg_gui::Size {
        self.bounds = agg_gui::Rect::new(0.0, 0.0, available.width, available.height);
        available
    }

    fn paint(&mut self, ctx: &mut dyn agg_gui::DrawCtx) {
        let now = web_time::Instant::now();
        let dt = match self.prev_frame_time {
            Some(t) => now.duration_since(t).as_secs_f64().clamp(0.0, 0.25),
            None => 1.0 / 60.0,
        };
        self.prev_frame_time = Some(now);

        // ── Integrate this frame's gesture deltas ────────────────────────
        let scale = self.unit_scale();
        let mut stroke_width = 1.0_f32;
        let had_gesture = if let Some(mt) = agg_gui::current_multi_touch() {
            self.zoom *= mt.zoom_delta as f64;
            self.rotation += mt.rotation_delta as f64;
            // Pan delta comes in widget pixels; store in normalised units
            // so the accumulator is resolution-independent.
            if scale > 0.0 {
                self.translation_x += mt.translation_delta.x / scale;
                self.translation_y += mt.translation_delta.y / scale;
            }
            self.force = mt.force;
            self.num_touches = mt.num_touches;
            self.last_touch_time = Some(now);
            stroke_width += 10.0 * mt.force;
            true
        } else {
            self.num_touches = 0;
            self.force = 0.0;
            self.slowly_reset(now, dt)
        };
        if had_gesture {
            agg_gui::animation::request_tick();
        }

        // ── Canvas background ────────────────────────────────────────────
        let v = ctx.visuals();
        ctx.set_fill_color(v.panel_fill);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, self.bounds.width, self.bounds.height);
        ctx.fill();

        // ── Arrow geometry ───────────────────────────────────────────────
        //
        // egui draws from (-0.5, 0.5) to (0.5, -0.5) in Y-down, meaning
        // bottom-left → top-right visually.  In Y-up that's
        // (-0.5, -0.5) → (0.5, 0.5).
        let cx = self.bounds.width * 0.5;
        let cy = self.bounds.height * 0.5;
        let zoom = self.zoom;
        let (sin_r, cos_r) = self.rotation.sin_cos();
        let rot_scale = |vx: f64, vy: f64| -> (f64, f64) {
            (
                zoom * (vx * cos_r - vy * sin_r),
                zoom * (vx * sin_r + vy * cos_r),
            )
        };
        let (tail_ox, tail_oy) = rot_scale(-0.5, -0.5);
        let (dir_x, dir_y) = rot_scale(1.0, 1.0);
        let tail_nx = self.translation_x + tail_ox;
        let tail_ny = self.translation_y + tail_oy;
        let tail_px = cx + tail_nx * scale;
        let tail_py = cy + tail_ny * scale;
        let tip_px = tail_px + dir_x * scale;
        let tip_py = tail_py + dir_y * scale;

        // ── Arrow stroke ─────────────────────────────────────────────────
        let color = v.text_color;
        ctx.set_stroke_color(color);
        ctx.set_line_width(stroke_width as f64);
        ctx.begin_path();
        ctx.move_to(tail_px, tail_py);
        ctx.line_to(tip_px, tip_py);
        ctx.stroke();

        // ── Arrow head (filled triangle at the tip) ──────────────────────
        let head_len = (dir_x * scale).hypot(dir_y * scale) * 0.12;
        let tip_len = (tip_px - tail_px).hypot(tip_py - tail_py);
        if tip_len > 1.0 && head_len > 0.5 {
            let ux = (tip_px - tail_px) / tip_len;
            let uy = (tip_py - tail_py) / tip_len;
            let head_half_angle = 0.45_f64;
            let (sa, ca) = head_half_angle.sin_cos();
            let lx = tip_px - head_len * (ux * ca - uy * sa);
            let ly = tip_py - head_len * (uy * ca + ux * sa);
            let rx = tip_px - head_len * (ux * ca + uy * sa);
            let ry = tip_py - head_len * (uy * ca - ux * sa);
            ctx.set_fill_color(color);
            ctx.begin_path();
            ctx.move_to(tip_px, tip_py);
            ctx.line_to(lx, ly);
            ctx.line_to(rx, ry);
            ctx.close_path();
            ctx.fill();
        }
    }

    fn on_event(&mut self, _event: &agg_gui::Event) -> agg_gui::EventResult {
        // Consume drag events so the host window doesn't move when the
        // user single-finger-drags over the canvas.  Matches the
        // `Sense::drag()` workaround egui uses for the same reason.
        match _event {
            agg_gui::Event::MouseDown { .. }
            | agg_gui::Event::MouseMove { .. }
            | agg_gui::Event::MouseUp { .. } => agg_gui::EventResult::Consumed,
            _ => agg_gui::EventResult::Ignored,
        }
    }

    fn needs_paint(&self) -> bool {
        true
    }
}

/// Build the Multi Touch demo window content.  Single-finger acts like
/// a mouse; two or more fingers produce pinch / rotate / pan gestures
/// that drive the rendered arrow.  Pressure (when the platform reports
/// it) thickens the stroke.
pub fn multi_touch(font: Arc<Font>) -> Box<dyn Widget> {
    let status_font = Arc::clone(&font);

    /// Live status label that re-reads `current_multi_touch` every
    /// layout and formats its text.  Matches egui's "Input source" line.
    struct StatusLabel {
        bounds: agg_gui::Rect,
        children: Vec<Box<dyn Widget>>,
        inner: Label,
    }
    impl Widget for StatusLabel {
        fn type_name(&self) -> &'static str {
            "MultiTouchStatus"
        }
        fn bounds(&self) -> agg_gui::Rect {
            self.bounds
        }
        fn set_bounds(&mut self, b: agg_gui::Rect) {
            self.bounds = b;
            self.inner.set_bounds(b);
        }
        fn children(&self) -> &[Box<dyn Widget>] {
            &self.children
        }
        fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
            &mut self.children
        }
        fn layout(&mut self, available: agg_gui::Size) -> agg_gui::Size {
            let txt = match agg_gui::current_multi_touch() {
                Some(mt) => format!(
                    "Input source: {}-finger touch   force: {:.2}",
                    mt.num_touches, mt.force,
                ),
                None => "Input source: none".to_string(),
            };
            self.inner.set_text(&txt);
            self.inner.layout(available)
        }
        fn paint(&mut self, ctx: &mut dyn agg_gui::DrawCtx) {
            self.inner.paint(ctx);
        }
        fn on_event(&mut self, _e: &agg_gui::Event) -> agg_gui::EventResult {
            agg_gui::EventResult::Ignored
        }
        fn needs_paint(&self) -> bool {
            true
        }
    }

    let status_label: Box<dyn Widget> = Box::new(StatusLabel {
        bounds: agg_gui::Rect::default(),
        children: Vec::new(),
        inner: Label::new(" ", Arc::clone(&status_font))
            .with_font_size(12.0)
            .with_wrap(true),
    });

    let heading = Label::new(
        "This demo only works on devices with multitouch support \
         (e.g. mobiles, tablets, and trackpads).",
        Arc::clone(&font),
    )
    .with_font_size(13.0)
    .with_wrap(true);

    let hint = Label::new(
        "Try touch gestures Pinch/Stretch, Rotation, and Pressure with 2+ fingers.",
        Arc::clone(&font),
    )
    .with_font_size(11.0)
    .with_wrap(true);

    let view: Box<dyn Widget> = Box::new(MultiTouchView::new());

    let col = FlexColumn::new()
        .with_gap(8.0)
        .with_padding(10.0)
        .with_panel_bg()
        .add(Box::new(heading))
        .add(Box::new(Separator::horizontal()))
        .add(Box::new(hint))
        .add(status_label)
        .add_flex(view, 1.0);

    Box::new(col)
}
