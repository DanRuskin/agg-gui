//! WASM demo crate for agg-gui — Phase 2.
//!
//! Exports `render_frame(width, height) -> Vec<u8>` which renders the Phase 2
//! demo and returns pixels in **top-down (Y-down)** order, ready for JS
//! `CanvasRenderingContext2D.putImageData`.
//!
//! Internally the framebuffer uses bottom-up (Y-up) layout. The flip is
//! applied once via `Framebuffer::pixels_flipped()` before returning.

use wasm_bindgen::prelude::*;
use agg_gui::{Color, CompOp, Framebuffer, GfxCtx};

/// Render the agg-gui Phase 2 demo into an RGBA pixel buffer.
///
/// Returns `width * height * 4` bytes in top-down row order (ready for
/// `putImageData`). The internal framebuffer is Y-up; a Y-flip is applied
/// on output so the canvas displays correctly.
#[wasm_bindgen]
pub fn render_frame(width: u32, height: u32) -> Vec<u8> {
    let mut fb = Framebuffer::new(width, height);
    {
        let mut ctx = GfxCtx::new(&mut fb);
        draw_phase2_demo(&mut ctx, width, height);
    }
    fb.pixels_flipped()
}

/// Draw the Phase 2 demo scene in light mode.
///
/// Demonstrates:
/// 1. Rounded rectangles (rounded_rect API)
/// 2. Blend modes (CompOp — Multiply, Screen, Overlay on overlapping shapes)
/// 3. Clip rect (clipped scene in a panel)
/// 4. Transform state stack (save/restore with nested translate/rotate/scale)
pub fn draw_phase2_demo(ctx: &mut GfxCtx, width: u32, height: u32) {
    let w = width as f64;
    let h = height as f64;

    // Light background
    ctx.clear(Color::rgb(0.94, 0.94, 0.96));

    let pad = (w.min(h) * 0.03).max(10.0);
    let gap = pad * 0.6;
    let col_w = (w - pad * 2.0 - gap) / 2.0;
    let row_h = (h - pad * 2.0 - gap) / 2.0;

    // Four quadrant panels
    let panels = [
        (pad,             pad + row_h + gap, col_w, row_h),   // top-left
        (pad + col_w + gap, pad + row_h + gap, col_w, row_h), // top-right
        (pad,             pad,               col_w, row_h),   // bottom-left
        (pad + col_w + gap, pad,             col_w, row_h),   // bottom-right
    ];

    for &(px, py, pw, ph) in &panels {
        draw_card(ctx, px, py, pw, ph);
    }

    // --- Panel 1 (top-left): Rounded rectangles ---
    {
        let (px, py, pw, ph) = panels[0];
        draw_panel_title(ctx, px, py, pw, ph, "Rounded Rects");
        let inner_y = py + ph * 0.15;
        let inner_h = ph * 0.78;
        draw_rounded_rects_demo(ctx, px, inner_y, pw, inner_h);
    }

    // --- Panel 2 (top-right): Blend modes ---
    {
        let (px, py, pw, ph) = panels[1];
        draw_panel_title(ctx, px, py, pw, ph, "Blend Modes");
        let inner_y = py + ph * 0.15;
        let inner_h = ph * 0.78;
        draw_blend_modes_demo(ctx, px, inner_y, pw, inner_h);
    }

    // --- Panel 3 (bottom-left): Clip rect ---
    {
        let (px, py, pw, ph) = panels[2];
        draw_panel_title(ctx, px, py, pw, ph, "Clip Rect");
        let inner_y = py + ph * 0.15;
        let inner_h = ph * 0.78;
        draw_clip_demo(ctx, px, inner_y, pw, inner_h);
    }

    // --- Panel 4 (bottom-right): Transform stack ---
    {
        let (px, py, pw, ph) = panels[3];
        draw_panel_title(ctx, px, py, pw, ph, "Transform Stack");
        let inner_y = py + ph * 0.15;
        let inner_h = ph * 0.78;
        draw_transform_demo(ctx, px, inner_y, pw, inner_h);
    }

    // Footer label
    let label_size = (w * 0.012).clamp(9.0, 13.0);
    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.35));
    ctx.fill_text_gsv("agg-gui  Phase 2", pad, pad * 0.4, label_size);
}

// ---------------------------------------------------------------------------
// Card / panel helpers
// ---------------------------------------------------------------------------

fn draw_card(ctx: &mut GfxCtx, x: f64, y: f64, w: f64, h: f64) {
    // Shadow (slight offset, semi-transparent dark)
    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.08));
    ctx.set_blend_mode(CompOp::Multiply);
    ctx.begin_path();
    ctx.rounded_rect(x + 2.0, y - 2.0, w, h, 10.0);
    ctx.fill();

    // Card face (white)
    ctx.set_blend_mode(CompOp::SrcOver);
    ctx.set_fill_color(Color::rgb(1.0, 1.0, 1.0));
    ctx.begin_path();
    ctx.rounded_rect(x, y, w, h, 10.0);
    ctx.fill();
}

fn draw_panel_title(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64, title: &str) {
    let size = (pw * 0.055).clamp(10.0, 16.0);
    let _ = ph;
    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.55));
    ctx.fill_text_gsv(title, px + pw * 0.05, py + ph * 0.86, size);
}

// ---------------------------------------------------------------------------
// Panel content: Rounded rects
// ---------------------------------------------------------------------------

fn draw_rounded_rects_demo(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64) {
    ctx.set_blend_mode(CompOp::SrcOver);
    let margin = pw * 0.07;
    let inner_x = px + margin;
    let inner_w = pw - margin * 2.0;
    let row_h = (ph - margin) / 3.0 - margin * 0.3;
    let radii = [4.0_f64, 12.0, row_h * 0.5];
    let colors = [
        Color::rgb(0.27, 0.53, 0.91),
        Color::rgb(0.22, 0.76, 0.55),
        Color::rgb(0.88, 0.42, 0.27),
    ];

    for (i, (&r, &col)) in radii.iter().zip(colors.iter()).enumerate() {
        let iy = py + ph - (i + 1) as f64 * (row_h + margin * 0.5) - margin * 0.3;

        // Fill
        ctx.set_fill_color(col.with_alpha(0.18));
        ctx.begin_path();
        ctx.rounded_rect(inner_x, iy, inner_w, row_h, r);
        ctx.fill();

        // Stroke
        ctx.set_stroke_color(col);
        ctx.set_line_width(1.5);
        ctx.begin_path();
        ctx.rounded_rect(inner_x, iy, inner_w, row_h, r);
        ctx.stroke();

        // Radius label
        let label = format!("r = {}", r as i32);
        let lsize = (pw * 0.04).clamp(8.0, 12.0);
        ctx.set_fill_color(col);
        ctx.fill_text_gsv(&label, inner_x + inner_w * 0.03, iy + row_h * 0.28, lsize);
    }
}

// ---------------------------------------------------------------------------
// Panel content: Blend modes
// ---------------------------------------------------------------------------

fn draw_blend_modes_demo(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64) {
    let cx = px + pw * 0.5;
    let cy = py + ph * 0.5;
    let r = pw.min(ph) * 0.22;
    let offset = r * 0.65;

    struct Demo { label: &'static str, mode: CompOp, dx: f64, dy: f64 }

    let demos = [
        Demo { label: "Multiply", mode: CompOp::Multiply,  dx:  0.0, dy:  0.0 },
        Demo { label: "Screen",   mode: CompOp::Screen,    dx:  0.0, dy:  0.0 },
        Demo { label: "Overlay",  mode: CompOp::Overlay,   dx:  0.0, dy:  0.0 },
    ];

    let col_w = pw / demos.len() as f64;
    let lsize = (pw * 0.032).clamp(7.0, 10.0);

    for (i, demo) in demos.iter().enumerate() {
        let ccx = px + col_w * (i as f64 + 0.5);
        let ccy = cy;
        let small_r = r * 0.7;
        let _ = (demo.dx, demo.dy);

        // Base: solid blue
        ctx.set_blend_mode(CompOp::SrcOver);
        ctx.set_fill_color(Color::rgba(0.22, 0.45, 0.87, 0.9));
        ctx.begin_path();
        ctx.circle(ccx - small_r * 0.35, ccy - small_r * 0.2, small_r);
        ctx.fill();

        // Overlay: solid red with selected blend mode
        ctx.set_blend_mode(demo.mode);
        ctx.set_fill_color(Color::rgba(0.91, 0.28, 0.18, 0.9));
        ctx.begin_path();
        ctx.circle(ccx + small_r * 0.35, ccy + small_r * 0.2, small_r);
        ctx.fill();

        // Green third circle
        ctx.set_fill_color(Color::rgba(0.14, 0.76, 0.39, 0.85));
        ctx.begin_path();
        ctx.circle(ccx, ccy - small_r * 0.55, small_r);
        ctx.fill();

        ctx.set_blend_mode(CompOp::SrcOver);
        ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.5));
        let label_x = ccx - lsize * demo.label.len() as f64 * 0.35;
        ctx.fill_text_gsv(demo.label, label_x, py + ph * 0.08, lsize);
    }
    let _ = (r, offset);
}

// ---------------------------------------------------------------------------
// Panel content: Clip rect
// ---------------------------------------------------------------------------

fn draw_clip_demo(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64) {
    ctx.set_blend_mode(CompOp::SrcOver);
    let margin = pw * 0.08;
    let cx = px + pw * 0.5;
    let cy = py + ph * 0.5;

    // Clip window (shown as a dashed border)
    let clip_x = px + margin * 1.5;
    let clip_y = py + margin * 1.5;
    let clip_w = pw - margin * 3.0;
    let clip_h = ph - margin * 3.5;

    // Dim overlay outside the clip — shows what would be clipped
    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.06));
    ctx.begin_path();
    ctx.rounded_rect(px + margin * 0.3, py + margin * 0.3,
                     pw - margin * 0.6, ph - margin * 0.6, 6.0);
    ctx.fill();

    // Apply clip
    ctx.save();
    ctx.clip_rect(clip_x, clip_y, clip_w, clip_h);

    // Rotating ring of circles (mostly inside, some clipped)
    let n = 8;
    let ring_r = pw.min(ph) * 0.28;
    let dot_r  = pw.min(ph) * 0.09;
    let colors = [
        Color::rgb(0.27, 0.53, 0.91),
        Color::rgb(0.91, 0.35, 0.22),
        Color::rgb(0.22, 0.76, 0.42),
        Color::rgb(0.88, 0.65, 0.10),
        Color::rgb(0.62, 0.28, 0.88),
        Color::rgb(0.10, 0.72, 0.88),
        Color::rgb(0.95, 0.38, 0.62),
        Color::rgb(0.38, 0.82, 0.12),
    ];
    for i in 0..n {
        let angle = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
        let dx = angle.cos() * ring_r;
        let dy = angle.sin() * ring_r;
        ctx.set_fill_color(colors[i % colors.len()]);
        ctx.begin_path();
        ctx.circle(cx + dx, cy + dy, dot_r);
        ctx.fill();
    }

    // Large circle at center
    ctx.set_fill_color(Color::rgba(0.27, 0.53, 0.91, 0.25));
    ctx.begin_path();
    ctx.circle(cx, cy, ring_r * 0.55);
    ctx.fill();
    ctx.set_stroke_color(Color::rgba(0.27, 0.53, 0.91, 0.6));
    ctx.set_line_width(2.0);
    ctx.begin_path();
    ctx.circle(cx, cy, ring_r * 0.55);
    ctx.stroke();

    ctx.restore(); // removes clip

    // Clip border indicator
    ctx.set_stroke_color(Color::rgba(0.3, 0.3, 0.3, 0.4));
    ctx.set_line_width(1.5);
    ctx.begin_path();
    ctx.rounded_rect(clip_x, clip_y, clip_w, clip_h, 4.0);
    ctx.stroke();
}

// ---------------------------------------------------------------------------
// Panel content: Transform stack
// ---------------------------------------------------------------------------

fn draw_transform_demo(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64) {
    ctx.set_blend_mode(CompOp::SrcOver);
    let cx = px + pw * 0.5;
    let cy = py + ph * 0.5;
    let unit = pw.min(ph) * 0.12;

    // Three nested save/restore levels, each adding a rotation
    let levels = [
        (unit * 2.8, 0.0_f64,                              Color::rgba(0.27, 0.53, 0.91, 0.25), Color::rgba(0.27, 0.53, 0.91, 0.8)),
        (unit * 2.0, std::f64::consts::PI / 6.0,           Color::rgba(0.22, 0.76, 0.42, 0.25), Color::rgba(0.22, 0.76, 0.42, 0.8)),
        (unit * 1.2, std::f64::consts::PI / 4.0,           Color::rgba(0.91, 0.42, 0.22, 0.3),  Color::rgba(0.91, 0.42, 0.22, 0.9)),
    ];

    for &(size, rot, fill, stroke) in &levels {
        ctx.save();
        ctx.translate(cx, cy);
        ctx.rotate(rot);

        ctx.set_fill_color(fill);
        ctx.begin_path();
        ctx.rounded_rect(-size * 0.5, -size * 0.5, size, size, size * 0.12);
        ctx.fill();

        ctx.set_stroke_color(stroke);
        ctx.set_line_width(1.8);
        ctx.begin_path();
        ctx.rounded_rect(-size * 0.5, -size * 0.5, size, size, size * 0.12);
        ctx.stroke();

        ctx.restore();
    }

    // Center dot
    ctx.set_fill_color(Color::rgb(0.2, 0.2, 0.25));
    ctx.begin_path();
    ctx.circle(cx, cy, unit * 0.18);
    ctx.fill();

    // Axis arrows from center
    let ax_len = unit * 1.5;
    // +X arrow (red)
    ctx.set_stroke_color(Color::rgba(0.85, 0.2, 0.2, 0.7));
    ctx.set_line_width(1.5);
    ctx.begin_path();
    ctx.move_to(cx, cy);
    ctx.line_to(cx + ax_len, cy);
    ctx.stroke();
    // +Y arrow (green, goes UP in Y-up space)
    ctx.set_stroke_color(Color::rgba(0.1, 0.7, 0.2, 0.7));
    ctx.begin_path();
    ctx.move_to(cx, cy);
    ctx.line_to(cx, cy + ax_len);
    ctx.stroke();
}
