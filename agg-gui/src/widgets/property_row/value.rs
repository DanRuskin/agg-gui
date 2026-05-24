//! Value type carried into a row renderer.
//!
//! Independent of any particular host's value system (node-editor's
//! `PropertyValue`, atomartist's `PortValue`, etc.) — hosts translate
//! their own variants into this borrow-form before calling
//! [`paint_row`](super::render::paint_row).
//!
//! Borrow form keeps the dispatcher allocation-free: a host that
//! stores its current value as `Arc<str>` or owned `String` hands a
//! `&str` slice into `RowValue::Text(...)`. The renderer reads, never
//! retains.

/// Borrowed current value for a property row.
#[derive(Clone, Copy, Debug)]
pub enum RowValue<'a> {
    /// Numeric value — rendered by `Slider` / `NumberDrag` / default.
    Number(f64),
    /// Boolean — rendered by the toggle painter.
    Bool(bool),
    /// RGBA in 0..=1. Rendered by the color painter as a swatch.
    Color([f32; 4]),
    /// Editable string content. Multi-line painters break on `\n`.
    Text(&'a str),
    /// Opaque display string for non-editable values — used by the
    /// matrix renderer ("Identity"), image previews, geometry
    /// summaries, etc.
    Display(&'a str),
}

impl<'a> RowValue<'a> {
    /// Borrowed string view of any text-like value — convenience for
    /// renderers that just need to paint a short string regardless of
    /// the underlying variant.
    pub fn as_short_text(&self) -> Option<&str> {
        match self {
            RowValue::Text(s) | RowValue::Display(s) => Some(s),
            _ => None,
        }
    }
}
