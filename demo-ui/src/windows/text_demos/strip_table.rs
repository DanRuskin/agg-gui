//! Text-related and layout demo windows: scrolling rows, strip layout, table,
//! text layout showcase, undo/redo, window options, modals, and multi-touch info.
//!
//! Most demos here are purely compositional — they build a widget tree from
//! `FlexColumn`, `FlexRow`, `Container`, `Label`, etc. without custom painting.

#![allow(unused_imports)]
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
