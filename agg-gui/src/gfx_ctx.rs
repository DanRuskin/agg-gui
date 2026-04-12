//! Graphics context — the primary drawing API for widget painting.
//!
//! `GfxCtx` is modeled after Cairo's `cairo_t`. All drawing goes through this
//! type. It owns a stateful transform + style stack and writes pixels into a
//! [`Framebuffer`] via AGG.
//!
//! # Coordinate system
//!
//! All coordinates are **first-quadrant (Y-up)**. Origin is the bottom-left
//! corner of the framebuffer. Positive X goes right, positive Y goes up.
//! Positive angles rotate counter-clockwise (mathematically standard).
//!
//! AGG is configured for bottom-up memory layout, so there is no Y-flip at
//! the rasterizer boundary.

use std::f64::consts::PI;

use agg_rust::arc::Arc as AggArc;
use agg_rust::basics::PATH_FLAGS_NONE;
use agg_rust::comp_op::{CompOp, PixfmtRgba32CompOp};
use agg_rust::conv_curve::ConvCurve;
use agg_rust::conv_stroke::ConvStroke;
use agg_rust::conv_transform::ConvTransform;
use agg_rust::gsv_text::GsvText;
use agg_rust::math_stroke::{LineCap, LineJoin};
use agg_rust::path_storage::PathStorage;
use agg_rust::rasterizer_scanline_aa::RasterizerScanlineAa;
use agg_rust::renderer_base::RendererBase;
use agg_rust::renderer_scanline::render_scanlines_aa_solid;
use agg_rust::rendering_buffer::RowAccessor;
use agg_rust::rounded_rect::RoundedRect;
use agg_rust::scanline_u::ScanlineU8;
use agg_rust::trans_affine::TransAffine;

use crate::color::Color;
use crate::framebuffer::Framebuffer;

// Re-export so callers don't need to import agg_rust directly.
pub use agg_rust::comp_op::CompOp as BlendMode;

/// Snapshot of drawing state, pushed/popped by `save()`/`restore()`.
#[derive(Clone)]
struct GfxState {
    transform: TransAffine,
    fill_color: Color,
    stroke_color: Color,
    line_width: f64,
    line_join: LineJoin,
    line_cap: LineCap,
    blend_mode: CompOp,
    /// Scissor clip in Y-up screen space: (x, y, width, height).
    /// Applied to RendererBase before each rasterization call.
    clip: Option<(f64, f64, f64, f64)>,
    /// Multiplied into fill and stroke alpha at draw time.
    global_alpha: f64,
}

impl Default for GfxState {
    fn default() -> Self {
        Self {
            transform: TransAffine::new(),
            fill_color: Color::black(),
            stroke_color: Color::black(),
            line_width: 1.0,
            line_join: LineJoin::Round,
            line_cap: LineCap::Round,
            blend_mode: CompOp::SrcOver,
            clip: None,
            global_alpha: 1.0,
        }
    }
}

/// Cairo-style stateful 2D graphics context.
///
/// All widget painting goes through `GfxCtx`. Create one per frame from a
/// [`Framebuffer`], draw into it, then let it drop — the framebuffer retains
/// the rendered pixels.
pub struct GfxCtx<'a> {
    fb: &'a mut Framebuffer,
    state: GfxState,
    state_stack: Vec<GfxState>,
    /// Accumulated path, reset by `begin_path()`.
    path: PathStorage,
}

impl<'a> GfxCtx<'a> {
    /// Create a new graphics context for the given framebuffer.
    pub fn new(fb: &'a mut Framebuffer) -> Self {
        Self {
            fb,
            state: GfxState::default(),
            state_stack: Vec::new(),
            path: PathStorage::new(),
        }
    }

    // -------------------------------------------------------------------------
    // State stack
    // -------------------------------------------------------------------------

    /// Push the current drawing state onto the stack.
    pub fn save(&mut self) {
        self.state_stack.push(self.state.clone());
    }

    /// Pop and restore the drawing state from the stack.
    pub fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.state = state;
        }
    }

    // -------------------------------------------------------------------------
    // Transform (Y-up, CCW-positive rotations)
    // -------------------------------------------------------------------------

    /// Append a translation to the current transform.
    ///
    /// Uses **pre-multiply** (Cairo semantics): `transform = T × transform`.
    /// This means `translate` + `rotate` rotates within the translated space,
    /// matching the behaviour of Cairo, HTML5 Canvas, and most GUI toolkits.
    pub fn translate(&mut self, tx: f64, ty: f64) {
        self.state.transform.premultiply(&TransAffine::new_translation(tx, ty));
    }

    /// Append a rotation (radians, counter-clockwise in Y-up space).
    ///
    /// Uses pre-multiply semantics — see [`translate`](Self::translate).
    pub fn rotate(&mut self, radians: f64) {
        self.state.transform.premultiply(&TransAffine::new_rotation(radians));
    }

    /// Append a uniform scale.
    ///
    /// Uses pre-multiply semantics — see [`translate`](Self::translate).
    pub fn scale(&mut self, sx: f64, sy: f64) {
        self.state.transform.premultiply(&TransAffine::new_scaling(sx, sy));
    }

    /// Replace the current transform entirely.
    pub fn set_transform(&mut self, m: TransAffine) {
        self.state.transform = m;
    }

    /// Reset the current transform to identity.
    pub fn reset_transform(&mut self) {
        self.state.transform = TransAffine::new();
    }

    // -------------------------------------------------------------------------
    // Style
    // -------------------------------------------------------------------------

    pub fn set_fill_color(&mut self, color: Color) {
        self.state.fill_color = color;
    }

    pub fn set_stroke_color(&mut self, color: Color) {
        self.state.stroke_color = color;
    }

    pub fn set_line_width(&mut self, w: f64) {
        self.state.line_width = w;
    }

    pub fn set_line_join(&mut self, join: LineJoin) {
        self.state.line_join = join;
    }

    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.state.line_cap = cap;
    }

    /// Set the Porter-Duff compositing mode for subsequent fill and stroke calls.
    ///
    /// Defaults to `CompOp::SrcOver` (standard alpha-over blending).
    pub fn set_blend_mode(&mut self, mode: CompOp) {
        self.state.blend_mode = mode;
    }

    /// Set a global alpha multiplier (0.0 = transparent, 1.0 = opaque).
    ///
    /// Multiplied into fill and stroke colors at draw time. Saved/restored
    /// with the state stack.
    pub fn set_global_alpha(&mut self, alpha: f64) {
        self.state.global_alpha = alpha.clamp(0.0, 1.0);
    }

    // -------------------------------------------------------------------------
    // Clipping
    // -------------------------------------------------------------------------

    /// Set a rectangular scissor clip in Y-up screen space.
    ///
    /// Only pixels inside `(x, y, x+w, y+h)` are affected by subsequent draws.
    /// The clip is pixel-aligned (AGG `RendererBase::clip_box_i`). It is
    /// saved and restored with the state stack.
    ///
    /// To combine clips (intersection), call `clip_rect` multiple times — each
    /// call intersects with the existing clip. Use `save`/`restore` to scope.
    pub fn clip_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        // Intersect with existing clip if present.
        if let Some((cx, cy, cw, ch)) = self.state.clip {
            let x1 = x.max(cx);
            let y1 = y.max(cy);
            let x2 = (x + w).min(cx + cw);
            let y2 = (y + h).min(cy + ch);
            self.state.clip = Some((x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0)));
        } else {
            self.state.clip = Some((x, y, w, h));
        }
    }

    /// Remove the clip region — subsequent draws cover the full framebuffer.
    pub fn reset_clip(&mut self) {
        self.state.clip = None;
    }

    // -------------------------------------------------------------------------
    // Clear
    // -------------------------------------------------------------------------

    /// Fill the entire framebuffer with `color`, ignoring any transform or clip.
    pub fn clear(&mut self, color: Color) {
        let rgba = color.to_rgba8();
        for chunk in self.fb.pixels_mut().chunks_exact_mut(4) {
            chunk[0] = rgba.r as u8;
            chunk[1] = rgba.g as u8;
            chunk[2] = rgba.b as u8;
            chunk[3] = rgba.a as u8;
        }
    }

    // -------------------------------------------------------------------------
    // Path construction
    // -------------------------------------------------------------------------

    /// Start a new path, discarding any previously accumulated path data.
    pub fn begin_path(&mut self) {
        self.path = PathStorage::new();
    }

    /// Move the current point without drawing.
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.path.move_to(x, y);
    }

    /// Add a straight line from the current point to `(x, y)`.
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.path.line_to(x, y);
    }

    /// Add a cubic Bézier curve to `(x, y)` with control points `(cx1,cy1)` and `(cx2,cy2)`.
    pub fn cubic_to(&mut self, cx1: f64, cy1: f64, cx2: f64, cy2: f64, x: f64, y: f64) {
        self.path.curve4(cx1, cy1, cx2, cy2, x, y);
    }

    /// Add a quadratic Bézier curve to `(x, y)` with control point `(cx, cy)`.
    pub fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.path.curve3(cx, cy, x, y);
    }

    /// Add an arc segment.
    ///
    /// Center `(cx, cy)`, radius `r`, from `start_angle` to `end_angle` in radians.
    /// Angles are measured CCW from the +X axis (standard mathematical convention).
    pub fn arc_to(&mut self, cx: f64, cy: f64, r: f64, start_angle: f64, end_angle: f64, ccw: bool) {
        let mut arc = AggArc::new(cx, cy, r, r, start_angle, end_angle, ccw);
        self.path.concat_path(&mut arc, 0);
    }

    /// Convenience: add a full circle at `(cx, cy)` with radius `r`.
    pub fn circle(&mut self, cx: f64, cy: f64, r: f64) {
        self.arc_to(cx, cy, r, 0.0, 2.0 * PI, true);
        self.path.close_polygon(PATH_FLAGS_NONE);
    }

    /// Add a rectangle (bottom-left corner `(x, y)`, size `w × h`).
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.path.move_to(x, y);
        self.path.line_to(x + w, y);
        self.path.line_to(x + w, y + h);
        self.path.line_to(x, y + h);
        self.path.close_polygon(PATH_FLAGS_NONE);
    }

    /// Add a rectangle with uniform corner radius `r`.
    ///
    /// Bottom-left corner at `(x, y)`, size `w × h`. Corners are smoothed
    /// with circular arcs via AGG's `RoundedRect`.
    pub fn rounded_rect(&mut self, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
        let mut rr = RoundedRect::new(x, y, x + w, y + h, r);
        rr.normalize_radius();
        self.path.concat_path(&mut rr, 0);
    }

    /// Close the current sub-path with a straight line back to its start.
    pub fn close_path(&mut self) {
        self.path.close_polygon(PATH_FLAGS_NONE);
    }

    // -------------------------------------------------------------------------
    // Drawing
    // -------------------------------------------------------------------------

    /// Fill the accumulated path with the current fill color.
    pub fn fill(&mut self) {
        let mut color = self.state.fill_color;
        color.a *= self.state.global_alpha as f32;
        let rgba = color.to_rgba8();
        let mode = self.state.blend_mode;
        let clip = self.state.clip;
        let transform = self.state.transform.clone();
        self.rasterize_fill(&rgba, mode, clip, &transform);
    }

    /// Stroke the accumulated path with the current stroke color and line width.
    pub fn stroke(&mut self) {
        let mut color = self.state.stroke_color;
        color.a *= self.state.global_alpha as f32;
        let rgba = color.to_rgba8();
        let width = self.state.line_width;
        let join = self.state.line_join;
        let cap = self.state.line_cap;
        let mode = self.state.blend_mode;
        let clip = self.state.clip;
        let transform = self.state.transform.clone();
        self.rasterize_stroke(&rgba, width, join, cap, mode, clip, &transform);
    }

    /// Fill then stroke the accumulated path in one call.
    pub fn fill_and_stroke(&mut self) {
        self.fill();
        self.stroke();
    }

    // -------------------------------------------------------------------------
    // Text (vector font via GsvText — Phase 1/2 only, full text in Phase 3)
    // -------------------------------------------------------------------------

    /// Draw a string at `(x, y)` using the built-in AGG vector font.
    ///
    /// `size` is the font height in pixels. The baseline is at Y = `y`.
    /// Ascenders go upward (higher Y) — correct in Y-up space.
    pub fn fill_text_gsv(&mut self, text: &str, x: f64, y: f64, size: f64) {
        let mut color = self.state.fill_color;
        color.a *= self.state.global_alpha as f32;
        let rgba = color.to_rgba8();
        let mode = self.state.blend_mode;
        let clip = self.state.clip;
        let transform = self.state.transform.clone();

        let w = self.fb.width();
        let h = self.fb.height();
        let stride = (w * 4) as i32;
        let mut ra = RowAccessor::new();
        unsafe { ra.attach(self.fb.pixels_mut().as_mut_ptr(), w, h, stride) };
        let mut pf = PixfmtRgba32CompOp::new_with_op(&mut ra, mode);
        pf.set_comp_op(mode);
        let mut rb = RendererBase::new(pf);
        apply_clip(&mut rb, clip);

        let mut ras = RasterizerScanlineAa::new();
        let mut sl = ScanlineU8::new();

        let mut gsv = GsvText::new();
        gsv.size(size, 0.0);
        gsv.start_point(x, y);
        gsv.text(text);

        let mut stroke = ConvStroke::new(&mut gsv);
        stroke.set_width(size * 0.1);
        let mut transformed = ConvTransform::new(&mut stroke, transform);
        ras.add_path(&mut transformed, 0);
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &rgba);
    }

    // -------------------------------------------------------------------------
    // Internal: AGG rasterization helpers
    // -------------------------------------------------------------------------

    fn rasterize_fill(
        &mut self,
        color: &agg_rust::color::Rgba8,
        mode: CompOp,
        clip: Option<(f64, f64, f64, f64)>,
        transform: &TransAffine,
    ) {
        let w = self.fb.width();
        let h = self.fb.height();
        let stride = (w * 4) as i32;
        let mut ra = RowAccessor::new();
        unsafe { ra.attach(self.fb.pixels_mut().as_mut_ptr(), w, h, stride) };
        let pf = PixfmtRgba32CompOp::new_with_op(&mut ra, mode);
        let mut rb = RendererBase::new(pf);
        apply_clip(&mut rb, clip);

        let mut ras = RasterizerScanlineAa::new();
        let mut sl = ScanlineU8::new();

        let mut curves = ConvCurve::new(&mut self.path);
        let mut transformed = ConvTransform::new(&mut curves, transform.clone());
        ras.add_path(&mut transformed, 0);
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, color);
    }

    fn rasterize_stroke(
        &mut self,
        color: &agg_rust::color::Rgba8,
        width: f64,
        join: LineJoin,
        cap: LineCap,
        mode: CompOp,
        clip: Option<(f64, f64, f64, f64)>,
        transform: &TransAffine,
    ) {
        let w = self.fb.width();
        let h = self.fb.height();
        let stride = (w * 4) as i32;
        let mut ra = RowAccessor::new();
        unsafe { ra.attach(self.fb.pixels_mut().as_mut_ptr(), w, h, stride) };
        let pf = PixfmtRgba32CompOp::new_with_op(&mut ra, mode);
        let mut rb = RendererBase::new(pf);
        apply_clip(&mut rb, clip);

        let mut ras = RasterizerScanlineAa::new();
        let mut sl = ScanlineU8::new();

        let mut curves = ConvCurve::new(&mut self.path);
        let mut stroke = ConvStroke::new(&mut curves);
        stroke.set_width(width);
        stroke.set_line_join(join);
        stroke.set_line_cap(cap);
        let mut transformed = ConvTransform::new(&mut stroke, transform.clone());
        ras.add_path(&mut transformed, 0);
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, color);
    }
}

/// Apply a Y-up scissor clip to the renderer, converting to inclusive pixel coordinates.
fn apply_clip<PF: agg_rust::pixfmt_rgba::PixelFormat>(
    rb: &mut RendererBase<PF>,
    clip: Option<(f64, f64, f64, f64)>,
) {
    if let Some((x, y, w, h)) = clip {
        let x1 = x.floor() as i32;
        let y1 = y.floor() as i32;
        let x2 = (x + w).ceil() as i32 - 1;
        let y2 = (y + h).ceil() as i32 - 1;
        rb.clip_box_i(x1, y1, x2, y2);
    }
}
