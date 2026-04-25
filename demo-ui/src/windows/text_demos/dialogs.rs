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
