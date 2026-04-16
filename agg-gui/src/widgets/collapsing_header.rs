//! `CollapsingHeader` — a clickable header that shows/hides child content.
//!
//! # Composition
//!
//! ```text
//! CollapsingHeader
//!   ├── Label (header text, drawn manually)
//!   └── child widget (shown when expanded, hidden when collapsed)
//! ```
//!
//! The triangle indicator is drawn as a path.  Clicking anywhere on the header
//! row toggles the collapsed/expanded state.

use std::sync::Arc;

use crate::color::Color;
use crate::draw_ctx::DrawCtx;
use crate::event::{Event, EventResult, MouseButton};
use crate::geometry::{Point, Rect, Size};
use crate::text::Font;
use crate::widget::{Widget, paint_subtree};
use crate::widgets::label::Label;

const HEADER_H: f64 = 22.0;
const TRIANGLE_SIZE: f64 = 6.0;
const INDENT: f64 = 12.0;

/// A collapsible section header.  When expanded, the child widget is visible
/// below the header row.  When collapsed, only the header row is shown.
pub struct CollapsingHeader {
    bounds:    Rect,
    children:  Vec<Box<dyn Widget>>,
    label:     Label,
    open:      bool,
    hovered:   bool,
    /// The content shown when expanded.
    content:   Option<Box<dyn Widget>>,
}

impl CollapsingHeader {
    /// Create a new header with the given text, using the provided font.
    /// Starts expanded by default.
    pub fn new(text: impl Into<String>, font: Arc<Font>) -> Self {
        let label = Label::new(text, Arc::clone(&font)).with_font_size(13.0);
        Self {
            bounds:   Rect::default(),
            children: Vec::new(),
            label,
            open:     true,
            hovered:  false,
            content:  None,
        }
    }

    /// Set whether the section is open (expanded) by default.
    pub fn default_open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Set the child content widget shown when expanded.
    pub fn with_content(mut self, content: Box<dyn Widget>) -> Self {
        self.content = Some(content);
        self
    }
}

impl Widget for CollapsingHeader {
    fn type_name(&self) -> &'static str { "CollapsingHeader" }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }

    fn layout(&mut self, available: Size) -> Size {
        let w = available.width;

        // Layout label inside the header row.
        let label_available = Size::new(w - INDENT - TRIANGLE_SIZE * 2.0, HEADER_H);
        let ls = self.label.layout(label_available);
        let ly = (HEADER_H - ls.height) * 0.5;
        self.label.set_bounds(Rect::new(INDENT + TRIANGLE_SIZE * 2.0 + 4.0, ly, ls.width, ls.height));

        // Layout content if open.
        let content_h = if self.open {
            if let Some(ref mut c) = self.content {
                let cs = c.layout(Size::new(w, available.height - HEADER_H));
                c.set_bounds(Rect::new(0.0, 0.0, cs.width, cs.height));
                cs.height
            } else {
                0.0
            }
        } else {
            0.0
        };

        let total_h = HEADER_H + content_h;
        self.bounds = Rect::new(0.0, 0.0, w, total_h);
        Size::new(w, total_h)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let w = self.bounds.width;
        let h = self.bounds.height;

        // Header row background on hover.
        if self.hovered {
            ctx.set_fill_color(Color::rgba(
                v.text_color.r, v.text_color.g, v.text_color.b, 0.06,
            ));
            ctx.begin_path();
            ctx.rounded_rect(0.0, h - HEADER_H, w, HEADER_H, 3.0);
            ctx.fill();
        }

        // Triangle indicator (▶ collapsed, ▼ expanded).
        // In Y-up: the header row occupies y = h - HEADER_H .. h.
        let center_y = h - HEADER_H * 0.5;
        let tx = INDENT;
        let ts = TRIANGLE_SIZE * 0.5;
        ctx.set_fill_color(v.text_dim);
        ctx.begin_path();
        if self.open {
            // Pointing down (▼): triangle with point at bottom.
            ctx.move_to(tx,          center_y + ts * 0.5);
            ctx.line_to(tx + ts * 2.0, center_y + ts * 0.5);
            ctx.line_to(tx + ts,       center_y - ts * 0.8);
        } else {
            // Pointing right (▶): triangle with point to the right.
            ctx.move_to(tx,            center_y + ts);
            ctx.line_to(tx,            center_y - ts);
            ctx.line_to(tx + ts * 1.6, center_y);
        }
        ctx.fill();

        // Label.
        self.label.set_color(v.text_color);
        let lb = self.label.bounds();
        // Label y is in header-local coords, but header is at top of our bounds (in Y-up).
        let label_offset_y = h - HEADER_H + lb.y;
        ctx.save();
        ctx.translate(lb.x, label_offset_y);
        paint_subtree(&mut self.label, ctx);
        ctx.restore();

        // Content (drawn at bottom of bounds = y=0 in Y-up).
        if self.open {
            if let Some(ref mut c) = self.content {
                let cb = c.bounds();
                // Content sits from y=0 to y=content_h.
                // Add a small left indent for visual hierarchy.
                ctx.save();
                ctx.translate(INDENT * 0.5, 0.0);
                paint_subtree(c.as_mut(), ctx);
                ctx.restore();
                let _ = cb;
            }
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let h = self.bounds.height;

        match event {
            Event::MouseMove { pos } => {
                // Header row: top portion in Y-up = y from (h - HEADER_H) to h.
                let in_header = pos.x >= 0.0 && pos.x <= self.bounds.width
                    && pos.y >= h - HEADER_H && pos.y <= h;
                let was = self.hovered;
                self.hovered = in_header;
                if self.hovered != was {
                    return EventResult::Consumed;
                }
                // Propagate to content if open.
                if self.open {
                    if let Some(ref mut c) = self.content {
                        let local = Point { x: pos.x - INDENT * 0.5, y: pos.y };
                        let ev = Event::MouseMove { pos: local };
                        return c.on_event(&ev);
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown { button: MouseButton::Left, pos, modifiers } => {
                let in_header = pos.x >= 0.0 && pos.x <= self.bounds.width
                    && pos.y >= h - HEADER_H && pos.y <= h;
                if in_header {
                    self.open = !self.open;
                    return EventResult::Consumed;
                }
                // Propagate to content.
                if self.open {
                    if let Some(ref mut c) = self.content {
                        let local = Point { x: pos.x - INDENT * 0.5, y: pos.y };
                        let ev = Event::MouseDown { button: MouseButton::Left, pos: local, modifiers: *modifiers };
                        return c.on_event(&ev);
                    }
                }
                EventResult::Ignored
            }
            Event::MouseUp { button: MouseButton::Left, pos, modifiers } => {
                if self.open {
                    if let Some(ref mut c) = self.content {
                        let local = Point { x: pos.x - INDENT * 0.5, y: pos.y };
                        let ev = Event::MouseUp { button: MouseButton::Left, pos: local, modifiers: *modifiers };
                        return c.on_event(&ev);
                    }
                }
                EventResult::Ignored
            }
            other => {
                // Forward to content when open.
                if self.open {
                    if let Some(ref mut c) = self.content {
                        return c.on_event(other);
                    }
                }
                EventResult::Ignored
            }
        }
    }

    fn hit_test(&self, local_pos: Point) -> bool {
        local_pos.x >= 0.0 && local_pos.x <= self.bounds.width
            && local_pos.y >= 0.0 && local_pos.y <= self.bounds.height
    }
}
