//! Inspector properties pane painter.
//!
//! Extracted from `inspector.rs` to keep both files within the project's
//! 800-line limit.  The single entry point is [`paint_properties`], which
//! renders the lower half of the inspector panel: section header, the static
//! geometry rows (x/y/width/height/depth), the widget's type-specific
//! properties, and the small box-model preview at the bottom.

use std::sync::Arc;

use crate::color::Color;
use crate::draw_ctx::DrawCtx;
use crate::text::Font;
use crate::widget::InspectorNode;

use super::inspector::{c_border, c_dim_text, c_text, FONT_SIZE};

pub(super) fn paint_properties(
    ctx: &mut dyn DrawCtx,
    available_h: f64,
    panel_w: f64,
    font: &Arc<Font>,
    selected: Option<usize>,
    nodes: &[InspectorNode],
) {
    if available_h < 4.0 {
        return;
    }
    let w = panel_w;
    let v = ctx.visuals().clone();

    ctx.set_font(Arc::clone(font));
    ctx.set_font_size(10.0);
    ctx.set_fill_color(c_dim_text(&v));
    ctx.fill_text("PROPERTIES", 10.0, available_h - 14.0);

    ctx.set_stroke_color(c_border(&v));
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(10.0 + 70.0, available_h - 10.0);
    ctx.line_to(w - 8.0, available_h - 10.0);
    ctx.stroke();

    let Some(sel_idx) = selected else {
        ctx.set_font_size(FONT_SIZE);
        ctx.set_fill_color(c_dim_text(&v));
        ctx.fill_text("(select a widget)", 10.0, available_h - 36.0);
        return;
    };

    let Some(node) = nodes.get(sel_idx) else {
        return;
    };

    ctx.set_font_size(14.0);
    ctx.set_fill_color(c_text(&v));
    ctx.fill_text(node.type_name, 10.0, available_h - 36.0);

    let b = &node.screen_bounds;
    let rows: &[(&str, String)] = &[
        ("x", format!("{:.1}", b.x)),
        ("y", format!("{:.1}", b.y)),
        ("width", format!("{:.1}", b.width)),
        ("height", format!("{:.1}", b.height)),
        ("depth", format!("{}", node.depth)),
    ];

    ctx.set_font_size(FONT_SIZE);
    let row_start_y = available_h - 56.0;
    for (i, (label, value)) in rows.iter().enumerate() {
        let ry = row_start_y - i as f64 * 18.0;
        if ry < 4.0 {
            break;
        }
        ctx.set_fill_color(c_dim_text(&v));
        ctx.fill_text(label, 12.0, ry);
        ctx.set_fill_color(c_text(&v));
        if let Some(m) = ctx.measure_text(value) {
            ctx.fill_text(value, w - m.width - 10.0, ry);
        }
        ctx.set_stroke_color(c_border(&v));
        ctx.set_line_width(0.5);
        ctx.begin_path();
        ctx.move_to(8.0, ry - 4.0);
        ctx.line_to(w - 8.0, ry - 4.0);
        ctx.stroke();
    }

    // Type-specific widget properties (from Widget::properties()).
    let prop_start_y = row_start_y - rows.len() as f64 * 18.0 - 4.0;
    for (j, (prop_label, prop_value)) in node.properties.iter().enumerate() {
        let ry = prop_start_y - j as f64 * 18.0;
        if ry < 4.0 {
            break;
        }
        ctx.set_fill_color(c_dim_text(&v));
        ctx.fill_text(prop_label, 12.0, ry);
        let is_bool = prop_value == "true" || prop_value == "false";
        if is_bool {
            let bool_color = if prop_value == "true" {
                Color::rgb(0.10, 0.52, 0.10)
            } else {
                Color::rgb(0.65, 0.18, 0.18)
            };
            ctx.set_fill_color(bool_color);
        } else {
            ctx.set_fill_color(c_text(&v));
        }
        if let Some(m) = ctx.measure_text(prop_value) {
            ctx.fill_text(prop_value, w - m.width - 10.0, ry);
        }
        ctx.set_stroke_color(c_border(&v));
        ctx.set_line_width(0.5);
        ctx.begin_path();
        ctx.move_to(8.0, ry - 4.0);
        ctx.line_to(w - 8.0, ry - 4.0);
        ctx.stroke();
    }

    // Box-model mini diagram.
    let total_rows = rows.len() + node.properties.len();
    let diag_h = (row_start_y - total_rows as f64 * 18.0 - 12.0).min(80.0);
    if diag_h > 30.0 {
        let diag_y_top = diag_h - 4.0;
        let diag_w = w - 20.0;
        let aspect = if b.height > 0.0 {
            b.width / b.height
        } else {
            1.0
        };
        let box_h = (diag_h * 0.6).min(50.0);
        let box_w = (box_h * aspect).min(diag_w * 0.8);
        let box_x = 10.0 + (diag_w - box_w) * 0.5;
        let box_y = diag_y_top - (diag_h + box_h) * 0.5;

        ctx.set_fill_color(Color::rgba(0.10, 0.50, 1.0, 0.10));
        ctx.begin_path();
        ctx.rect(box_x, box_y, box_w, box_h);
        ctx.fill();
        ctx.set_stroke_color(Color::rgba(0.10, 0.50, 1.0, 0.50));
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.rect(box_x, box_y, box_w, box_h);
        ctx.stroke();

        let dim = format!("{:.0} × {:.0}", b.width, b.height);
        ctx.set_font_size(10.0);
        ctx.set_fill_color(Color::rgba(0.10, 0.40, 0.90, 0.80));
        if let Some(m) = ctx.measure_text(&dim) {
            if m.width < box_w - 4.0 {
                ctx.fill_text(
                    &dim,
                    box_x + (box_w - m.width) * 0.5,
                    box_y + (box_h - m.ascent - m.descent) * 0.5 + m.descent,
                );
            }
        }
    }
}
