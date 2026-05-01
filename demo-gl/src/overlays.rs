use std::sync::Arc;

use agg_gui::color::Color;
use agg_gui::draw_ctx::DrawCtx;
use agg_gui::text::Font;
use agg_gui::InspectorOverlay;

use crate::GlGfxCtx;

/// Draw the inspector hover overlay in the Chrome F12 box-model style:
/// orange margin band outside the widget bounds, teal/blue widget bounds
/// fill + outline, and a green padding band inset from the bounds.
///
/// Called after every `App::paint` on both native and WASM so the highlight
/// is identical on both platforms.
pub fn draw_hover_overlay(ctx: &mut GlGfxCtx, overlay: InspectorOverlay) {
    let r = overlay.bounds;

    // Degenerate widget (zero width or zero height — most often a SizedBox
    // used as a flex spacer).  Draw a 2-px magenta bar on the live axis so
    // the spacer is still visible and labelled, then bail before the regular
    // margin/padding bands (which would be meaningless on a 0-extent box).
    if r.width < 1.0 || r.height < 1.0 {
        let bar = 2.0;
        let half = bar * 0.5;
        let marker = Color::rgba(0.85, 0.20, 0.75, 0.85);
        ctx.set_fill_color(marker);
        ctx.begin_path();
        if r.height < 1.0 && r.width >= 1.0 {
            // Horizontal spacer in a row: bar runs along x, centred on y.
            ctx.rect(r.x, r.y - half, r.width, bar);
        } else if r.width < 1.0 && r.height >= 1.0 {
            // Vertical spacer in a column: bar runs along y, centred on x.
            ctx.rect(r.x - half, r.y, bar, r.height);
        } else {
            // 0×0: a small square marker.
            ctx.rect(r.x - half, r.y - half, bar, bar);
        }
        ctx.fill();

        let label = format!("{:.0} × {:.0}", r.width, r.height);
        ctx.set_fill_color(marker);
        ctx.fill_text_gsv(&label, r.x + 2.0, r.y + r.height + 2.0, 9.0);
        return;
    }

    let m = overlay.margin;
    let p = overlay.padding;

    // ── Margin band (orange, drawn OUTSIDE the bounds) ──────────────────────
    // Y-up: `margin.bottom` extends below y=r.y (smaller Y); `margin.top`
    // extends above y=r.y+r.height.
    if m.left > 0.0 || m.right > 0.0 || m.top > 0.0 || m.bottom > 0.0 {
        let outer_x = r.x - m.left;
        let outer_y = r.y - m.bottom;
        let outer_w = r.width + m.left + m.right;
        let margin_fill = Color::rgba(0.96, 0.65, 0.18, 0.28);
        ctx.set_fill_color(margin_fill);
        // Bottom band
        if m.bottom > 0.0 {
            ctx.begin_path();
            ctx.rect(outer_x, outer_y, outer_w, m.bottom);
            ctx.fill();
        }
        // Top band
        if m.top > 0.0 {
            ctx.begin_path();
            ctx.rect(outer_x, r.y + r.height, outer_w, m.top);
            ctx.fill();
        }
        // Left band (only the slice that aligns with the widget's vertical span)
        if m.left > 0.0 {
            ctx.begin_path();
            ctx.rect(outer_x, r.y, m.left, r.height);
            ctx.fill();
        }
        // Right band
        if m.right > 0.0 {
            ctx.begin_path();
            ctx.rect(r.x + r.width, r.y, m.right, r.height);
            ctx.fill();
        }
    }

    // ── Widget bounds (teal fill + inset outline) ───────────────────────────
    let sw = 1.5_f64;
    let half = sw * 0.5;
    ctx.set_fill_color(Color::rgba(0.05, 0.65, 0.85, 0.18));
    ctx.begin_path();
    ctx.rect(r.x, r.y, r.width, r.height);
    ctx.fill();
    ctx.set_stroke_color(Color::rgba(0.05, 0.65, 0.85, 0.80));
    ctx.set_line_width(sw);
    ctx.begin_path();
    ctx.rect(
        r.x + half,
        r.y + half,
        (r.width - sw).max(0.0),
        (r.height - sw).max(0.0),
    );
    ctx.stroke();

    // ── Padding band (green, INSIDE the bounds) ─────────────────────────────
    // Drawn as four strips covering the area between the widget edge and the
    // inner content rect, matching how Chrome paints `padding`.
    if p.left > 0.0 || p.right > 0.0 || p.top > 0.0 || p.bottom > 0.0 {
        let pl = p.left.min(r.width);
        let pr = p.right.min((r.width - pl).max(0.0));
        let pb = p.bottom.min(r.height);
        let pt = p.top.min((r.height - pb).max(0.0));
        let pad_fill = Color::rgba(0.30, 0.70, 0.35, 0.30);
        ctx.set_fill_color(pad_fill);
        if pb > 0.0 {
            ctx.begin_path();
            ctx.rect(r.x, r.y, r.width, pb);
            ctx.fill();
        }
        if pt > 0.0 {
            ctx.begin_path();
            ctx.rect(r.x, r.y + r.height - pt, r.width, pt);
            ctx.fill();
        }
        if pl > 0.0 {
            ctx.begin_path();
            ctx.rect(r.x, r.y + pb, pl, (r.height - pb - pt).max(0.0));
            ctx.fill();
        }
        if pr > 0.0 {
            ctx.begin_path();
            ctx.rect(
                r.x + r.width - pr,
                r.y + pb,
                pr,
                (r.height - pb - pt).max(0.0),
            );
            ctx.fill();
        }
    }

    // ── Size label above the widget ─────────────────────────────────────────
    let label = format!("{:.0} × {:.0}", r.width, r.height);
    ctx.set_fill_color(Color::rgba(0.05, 0.65, 0.85, 1.00));
    ctx.fill_text_gsv(&label, r.x + 2.0, r.y + r.height + 2.0, 9.0);
}

/// Draw a "WxH  X.Xms" status bar in the bottom-left corner of the viewport.
///
/// `frame_ms` is the render time of the *previous* frame (so the display does
/// not include its own drawing cost).  Both native and WASM use this function
/// to keep the status overlay visually identical.
pub fn draw_status_overlay(ctx: &mut GlGfxCtx, font: Arc<Font>, w: u32, h: u32, frame_ms: f64) {
    let status = format!("{}×{}   {:.1}ms", w, h, frame_ms);
    ctx.set_font(font);
    ctx.set_font_size(11.0);
    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.30));
    ctx.fill_text(&status, 12.0, 6.0);
}
