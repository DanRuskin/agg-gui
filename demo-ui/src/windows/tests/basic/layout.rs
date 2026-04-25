#![allow(unused_imports)]
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widget::paint_subtree;
use agg_gui::{
    set_cursor_icon, Color, Container, CursorIcon, DrawCtx, Event, EventResult, FlexColumn,
    FlexRow, Font, Label, Point, Rect, Separator, Size, SizedBox, TextField, Widget,
};

// ---------------------------------------------------------------------------
// Input Test
// ---------------------------------------------------------------------------

/// Records the last-pressed key name and mouse position.
struct InputStateWidget {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    font: Arc<Font>,
    last_key: Option<String>,
    mouse_pos: Point,
}

impl Widget for InputStateWidget {
    fn type_name(&self) -> &'static str {
        "InputStateWidget"
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
        let h = 100.0_f64.min(available.height);
        self.bounds = Rect::new(0.0, 0.0, available.width, h);
        Size::new(available.width, h)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let w = self.bounds.width;
        let h = self.bounds.height;

        ctx.set_fill_color(v.widget_bg);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, w, h);
        ctx.fill();
        ctx.set_stroke_color(v.widget_stroke);
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, w, h);
        ctx.stroke();

        ctx.set_font(Arc::clone(&self.font));
        ctx.set_font_size(12.0);
        ctx.set_fill_color(v.text_color);

        let key_str = self.last_key.as_deref().unwrap_or("—");
        ctx.fill_text(&format!("Last key:   {}", key_str), 10.0, h - 20.0);
        ctx.fill_text(
            &format!(
                "Mouse pos:  ({:.0}, {:.0})",
                self.mouse_pos.x, self.mouse_pos.y
            ),
            10.0,
            h - 44.0,
        );
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { pos } => {
                self.mouse_pos = *pos;
                EventResult::Consumed
            }
            Event::KeyDown { key, .. } => {
                self.last_key = Some(format!("{:?}", key));
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn hit_test(&self, p: Point) -> bool {
        p.x >= 0.0 && p.x <= self.bounds.width && p.y >= 0.0 && p.y <= self.bounds.height
    }
}

/// Build the Input Test — shows last key pressed and current mouse position.
pub fn input_test(font: Arc<Font>) -> Box<dyn Widget> {
    let mut col = FlexColumn::new()
        .with_gap(12.0)
        .with_padding(12.0)
        .with_panel_bg();

    col.push(
        Box::new(
            Label::new(
                "Move the mouse or press keys inside the status box",
                Arc::clone(&font),
            )
            .with_font_size(11.5)
            .with_wrap(true),
        ),
        0.0,
    );

    col.push(
        Box::new(InputStateWidget {
            bounds: Rect::default(),
            children: Vec::new(),
            font: Arc::clone(&font),
            last_key: None,
            mouse_pos: Point { x: 0.0, y: 0.0 },
        }),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    Box::new(col)
}

// ---------------------------------------------------------------------------
// Layout Test
// ---------------------------------------------------------------------------

/// Build the Layout Test — colored boxes with alignment labels.
pub fn layout_test(font: Arc<Font>) -> Box<dyn Widget> {
    let labels = ["Left", "Center", "Right", "Stretch"];
    let colors = [
        Color::rgba(0.22, 0.45, 0.88, 0.25),
        Color::rgba(0.18, 0.72, 0.42, 0.25),
        Color::rgba(0.88, 0.25, 0.18, 0.25),
        Color::rgba(0.86, 0.78, 0.40, 0.25),
    ];

    let mut col = FlexColumn::new()
        .with_gap(12.0)
        .with_padding(14.0)
        .with_panel_bg();

    col.push(
        Box::new(Label::new("Alignment examples", Arc::clone(&font)).with_font_size(12.0)),
        0.0,
    );

    for (i, (&lbl, &bg)) in labels.iter().zip(colors.iter()).enumerate() {
        let box_w = match i {
            0 => 80.0,
            1 => 120.0,
            2 => 100.0,
            _ => 0.0, // stretch — use flex
        };

        let cell = Container::new()
            .with_background(bg)
            .with_border(Color::rgba(0.0, 0.0, 0.0, 0.15), 1.0)
            .with_padding(6.0)
            .add(Box::new(
                Label::new(lbl, Arc::clone(&font)).with_font_size(12.0),
            ));

        if i == 3 {
            // Stretch row.
            let row = FlexRow::new().add_flex(Box::new(cell), 1.0);
            col.push(Box::new(row), 0.0);
        } else {
            let row = FlexRow::new().add(Box::new(
                SizedBox::new().with_width(box_w).with_child(Box::new(cell)),
            ));
            col.push(Box::new(row), 0.0);
        }
    }

    col.push(Box::new(Separator::horizontal()), 0.0);
    col.push(
        Box::new(
            Label::new(
                "FlexRow / FlexColumn control alignment.\n\
         add() = fixed-size child, add_flex() = fills remaining space.",
                Arc::clone(&font),
            )
            .with_font_size(11.0)
            .with_wrap(true),
        ),
        0.0,
    );

    col.push(Box::new(SizedBox::new().with_height(8.0)), 0.0);
    Box::new(col)
}

// ---------------------------------------------------------------------------
// Manual Layout Test
// ---------------------------------------------------------------------------

/// A custom-painted widget showing absolutely-positioned boxes with corner labels.
struct ManualLayoutWidget {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    font: Arc<Font>,
}

impl Widget for ManualLayoutWidget {
    fn type_name(&self) -> &'static str {
        "ManualLayoutWidget"
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
        self.bounds = Rect::new(0.0, 0.0, available.width, available.height);
        available
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let w = self.bounds.width;
        let h = self.bounds.height;

        // Background.
        ctx.set_fill_color(v.bg_color);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, w, h);
        ctx.fill();

        ctx.set_font(Arc::clone(&self.font));

        // Absolutely-positioned boxes.
        let boxes: &[(f64, f64, f64, f64, Color, &str)] = &[
            (
                10.0,
                h - 60.0,
                80.0,
                40.0,
                Color::rgba(0.22, 0.45, 0.88, 0.25),
                "TL",
            ),
            (
                w - 90.0,
                h - 60.0,
                80.0,
                40.0,
                Color::rgba(0.18, 0.72, 0.42, 0.25),
                "TR",
            ),
            (
                10.0,
                20.0,
                80.0,
                40.0,
                Color::rgba(0.88, 0.25, 0.18, 0.25),
                "BL",
            ),
            (
                w - 90.0,
                20.0,
                80.0,
                40.0,
                Color::rgba(0.86, 0.78, 0.40, 0.25),
                "BR",
            ),
            (
                (w - 100.0) * 0.5,
                (h - 50.0) * 0.5,
                100.0,
                50.0,
                Color::rgba(0.60, 0.25, 0.88, 0.20),
                "Center",
            ),
        ];

        for &(bx, by, bw, bh, bg, label) in boxes {
            ctx.set_fill_color(bg);
            ctx.begin_path();
            ctx.rounded_rect(bx, by, bw, bh, 4.0);
            ctx.fill();
            ctx.set_stroke_color(v.widget_stroke);
            ctx.set_line_width(1.0);
            ctx.begin_path();
            ctx.rounded_rect(bx, by, bw, bh, 4.0);
            ctx.stroke();
            ctx.set_font_size(11.0);
            ctx.set_fill_color(v.text_color);
            ctx.fill_text(label, bx + 5.0, by + bh * 0.4 + 4.0);
            // Coordinate label.
            ctx.set_font_size(8.5);
            ctx.set_fill_color(v.text_dim);
            ctx.fill_text(&format!("({:.0},{:.0})", bx, by), bx + 5.0, by + 9.0);
        }
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

/// Build the Manual Layout Test — five absolutely positioned boxes.
pub fn manual_layout_test(font: Arc<Font>) -> Box<dyn Widget> {
    let mut col = FlexColumn::new()
        .with_gap(8.0)
        .with_padding(8.0)
        .with_panel_bg();

    col.push(
        Box::new(
            Label::new(
                "Absolutely-positioned boxes with coordinate labels",
                Arc::clone(&font),
            )
            .with_font_size(11.5)
            .with_wrap(true),
        ),
        0.0,
    );

    col.push(
        Box::new(ManualLayoutWidget {
            bounds: Rect::default(),
            children: Vec::new(),
            font,
        }),
        1.0,
    );

    Box::new(col)
}
