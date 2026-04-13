//! Text rendering — font loading, shaping, and glyph rasterization.
//!
//! # Pipeline
//!
//! ```text
//! Font bytes (TTF/OTF)
//!   │  ttf-parser  →  glyph outline curves
//!   │  rustybuzz   →  shaped glyph positions & advances
//!   │
//! GlyphPathBuilder  →  AGG PathStorage (Bézier curves)
//!   │
//! rasterize_fill_path  →  Framebuffer pixels
//! ```
//!
//! # Coordinate system
//!
//! TrueType fonts use Y-up coordinates (positive Y = above baseline).
//! This matches GfxCtx's first-quadrant convention exactly — no Y-flip
//! is needed at the glyph boundary.
//!
//! The baseline is placed at the Y coordinate passed to `GfxCtx::fill_text`.
//! Ascenders go to higher Y values (up), descenders to lower Y values (down),
//! which is correct for Y-up rendering.

use std::sync::Arc;

use agg_rust::basics::{is_end_poly, is_move_to, is_stop, PATH_CMD_LINE_TO, PATH_FLAGS_NONE, VertexSource};
use agg_rust::conv_curve::ConvCurve;
use agg_rust::path_storage::PathStorage;

/// Metrics describing a single line of shaped text.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextMetrics {
    /// Advance width of the text run in pixels.
    pub width: f64,
    /// Distance from baseline to top of tallest ascender, in pixels (positive).
    pub ascent: f64,
    /// Distance from baseline to bottom of deepest descender, in pixels (positive).
    pub descent: f64,
    /// Recommended line height (ascender + descender + line gap), in pixels.
    pub line_height: f64,
}

/// A loaded font, ready for shaping and rasterization.
///
/// Constructed from raw TTF/OTF bytes via [`Font::from_bytes`]. The data is
/// reference-counted so fonts can be cheaply shared and saved across frames.
pub struct Font {
    pub(crate) data: Arc<Vec<u8>>,
    index: u32,
    /// Cached at construction to avoid repeated parsing.
    units_per_em: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,
}

impl Font {
    /// Parse a font from raw TTF/OTF bytes.
    ///
    /// Returns `Err` if the data is not a valid font.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, &'static str> {
        let face = ttf_parser::Face::parse(&data, 0).map_err(|_| "failed to parse font")?;
        Ok(Self {
            units_per_em: face.units_per_em(),
            ascender: face.ascender(),
            descender: face.descender(),
            line_gap: face.line_gap(),
            data: Arc::new(data),
            index: 0,
        })
    }

    /// Parse a font from a borrowed byte slice (data is copied).
    pub fn from_slice(data: &[u8]) -> Result<Self, &'static str> {
        Self::from_bytes(data.to_vec())
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Ascender height in pixels at the given font size.
    pub fn ascender_px(&self, size: f64) -> f64 {
        self.ascender as f64 * size / self.units_per_em as f64
    }

    /// Descender depth in pixels at the given font size (positive value).
    pub fn descender_px(&self, size: f64) -> f64 {
        self.descender.unsigned_abs() as f64 * size / self.units_per_em as f64
    }

    /// Recommended line height in pixels at the given font size.
    pub fn line_height_px(&self, size: f64) -> f64 {
        let total = (self.ascender - self.descender + self.line_gap) as f64;
        total * size / self.units_per_em as f64
    }

    /// Run `f` with a `rustybuzz::Face` borrowed from the internal data.
    ///
    /// The face has the same lifetime as the closure invocation, so it cannot
    /// outlive this call. Use this for shaping + outline extraction.
    pub(crate) fn with_rb_face<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&rustybuzz::Face<'_>) -> R,
    {
        let face = rustybuzz::Face::from_slice(&self.data, self.index)
            .expect("font was validated at construction");
        f(&face)
    }
}

// ---------------------------------------------------------------------------
// Glyph outline → AGG PathStorage
// ---------------------------------------------------------------------------

/// Converts ttf-parser outline callbacks into an AGG `PathStorage`.
///
/// TTF fonts are Y-up; GfxCtx is Y-up — no axis flip is needed. Each glyph
/// is translated to its screen position `(ox, oy)` and scaled by `scale`.
pub(crate) struct GlyphPathBuilder {
    pub path: PathStorage,
    ox: f64,
    oy: f64,
    scale: f64,
    pub has_outline: bool,
}

impl GlyphPathBuilder {
    pub fn new(ox: f64, oy: f64, scale: f64) -> Self {
        Self {
            path: PathStorage::new(),
            ox,
            oy,
            scale,
            has_outline: false,
        }
    }

    #[inline]
    fn x(&self, v: f32) -> f64 { self.ox + v as f64 * self.scale }
    #[inline]
    fn y(&self, v: f32) -> f64 { self.oy + v as f64 * self.scale }
}

impl ttf_parser::OutlineBuilder for GlyphPathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.x(x), self.y(y));
        self.has_outline = true;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.x(x), self.y(y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.curve3(self.x(x1), self.y(y1), self.x(x), self.y(y));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.curve4(
            self.x(x1), self.y(y1),
            self.x(x2), self.y(y2),
            self.x(x),  self.y(y),
        );
    }
    fn close(&mut self) {
        self.path.close_polygon(PATH_FLAGS_NONE);
    }
}

// ---------------------------------------------------------------------------
// Shaping helper — shapes text and returns per-glyph paths
// ---------------------------------------------------------------------------

/// Shape `text` with `font` at `size` pixels, starting at screen position
/// `(x, y)` (baseline-left, Y-up). Returns one `PathStorage` per glyph that
/// has an outline (spaces and control chars yield no path).
pub(crate) fn shape_text(
    font: &Font,
    text: &str,
    size: f64,
    x: f64,
    y: f64,
) -> (Vec<PathStorage>, f64) {
    let scale = size / font.units_per_em() as f64;

    font.with_rb_face(|face| {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let output = rustybuzz::shape(face, &[], buffer);

        let mut paths = Vec::new();
        let mut pen_x = x;
        let mut total_advance = 0.0;

        for (info, pos) in output
            .glyph_infos()
            .iter()
            .zip(output.glyph_positions().iter())
        {
            let gid = ttf_parser::GlyphId(info.glyph_id as u16);
            let gx = pen_x + pos.x_offset as f64 * scale;
            let gy = y + pos.y_offset as f64 * scale;

            let mut builder = GlyphPathBuilder::new(gx, gy, scale);
            let has_outline = face.outline_glyph(gid, &mut builder).is_some();

            if has_outline && builder.has_outline {
                paths.push(builder.path);
            }

            let adv = pos.x_advance as f64 * scale;
            pen_x += adv;
            total_advance += adv;
        }

        (paths, total_advance)
    })
}

// ---------------------------------------------------------------------------
// Flattened contours for GL tessellation (tess2 input)
// ---------------------------------------------------------------------------

/// Shape `text` and return all glyph contours flattened to polylines.
///
/// Bézier curves are approximated with line segments at `flatness` pixels
/// tolerance (a good default is `0.5`).  The returned `Vec<Vec<[f32;2]>>`
/// has one inner `Vec` per closed contour; tess2 can tessellate the whole
/// list directly.
///
/// `(x, y)` is the baseline-left origin in Y-up pixel space.
pub fn shape_and_flatten_text(
    font: &Font,
    text: &str,
    size: f64,
    x: f64,
    y: f64,
    flatness: f64,
) -> Vec<Vec<[f32; 2]>> {
    let scale = size / font.units_per_em() as f64;
    font.with_rb_face(|face| {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let output = rustybuzz::shape(face, &[], buffer);
        let mut all_contours: Vec<Vec<[f32; 2]>> = Vec::new();
        let mut pen_x = x;

        for (info, pos) in output
            .glyph_infos()
            .iter()
            .zip(output.glyph_positions().iter())
        {
            let gid = ttf_parser::GlyphId(info.glyph_id as u16);
            let gx = pen_x + pos.x_offset as f64 * scale;
            let gy = y + pos.y_offset as f64 * scale;

            let mut builder = FlatContourBuilder::new(gx, gy, scale, flatness);
            face.outline_glyph(gid, &mut builder);
            builder.flush();
            all_contours.extend(builder.contours);

            pen_x += pos.x_advance as f64 * scale;
        }
        all_contours
    })
}

/// Shape `text` and return glyph contours flattened by AGG's own `ConvCurve`,
/// grouped **per glyph**.
///
/// Uses AGG's `ConvCurve` — the same Bézier flattener that `GfxCtx::fill` /
/// `rasterize_fill_path` use internally — so tess2 sees identical geometry to
/// the software rasterizer.
///
/// Returns `Vec<Vec<Vec<[f32; 2]>>>`:
/// - outer `Vec`: one entry per shaped glyph
/// - middle `Vec`: contours belonging to that glyph (e.g. 'O' has outer + inner)
/// - inner `Vec`: flattened polyline points for one contour
///
/// Keeping contours grouped per glyph lets the caller tessellate each glyph
/// with the EvenOdd rule so counters (holes in O, D, B, R …) are handled
/// correctly, while strokes from different glyphs never interact.
///
/// `(x, y)` is the baseline-left origin in Y-up pixel space.
pub fn shape_and_flatten_text_via_agg(
    font: &Font,
    text: &str,
    size: f64,
    x: f64,
    y: f64,
) -> Vec<Vec<Vec<[f32; 2]>>> {
    let scale = size / font.units_per_em() as f64;
    font.with_rb_face(|face| {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let output = rustybuzz::shape(face, &[], buffer);
        let mut all_glyphs: Vec<Vec<Vec<[f32; 2]>>> = Vec::new();
        let mut pen_x = x;

        for (info, pos) in output
            .glyph_infos()
            .iter()
            .zip(output.glyph_positions().iter())
        {
            let gid = ttf_parser::GlyphId(info.glyph_id as u16);
            let gx = pen_x + pos.x_offset as f64 * scale;
            let gy = y + pos.y_offset as f64 * scale;

            let mut builder = GlyphPathBuilder::new(gx, gy, scale);
            let has_outline = face.outline_glyph(gid, &mut builder).is_some();

            if has_outline && builder.has_outline {
                // Flatten via AGG's ConvCurve — same algorithm as the software path.
                let mut curves = ConvCurve::new(builder.path);
                curves.rewind(0);

                let mut glyph_contours: Vec<Vec<[f32; 2]>> = Vec::new();
                let mut current: Vec<[f32; 2]> = Vec::new();

                loop {
                    let (mut cx, mut cy) = (0.0_f64, 0.0_f64);
                    let cmd = curves.vertex(&mut cx, &mut cy);
                    if is_stop(cmd) { break; }
                    if is_move_to(cmd) {
                        if current.len() >= 3 {
                            glyph_contours.push(std::mem::take(&mut current));
                        } else {
                            current.clear();
                        }
                        current.push([cx as f32, cy as f32]);
                    } else if cmd == PATH_CMD_LINE_TO {
                        current.push([cx as f32, cy as f32]);
                    } else if is_end_poly(cmd) {
                        if current.len() >= 3 {
                            glyph_contours.push(std::mem::take(&mut current));
                        } else {
                            current.clear();
                        }
                    }
                }
                if current.len() >= 3 {
                    glyph_contours.push(current);
                }
                if !glyph_contours.is_empty() {
                    all_glyphs.push(glyph_contours);
                }
            }

            pen_x += pos.x_advance as f64 * scale;
        }
        all_glyphs
    })
}

/// Converts ttf-parser outline callbacks into flat polyline contours.
///
/// Bézier curves are subdivided until each segment is within `flatness` pixels.
struct FlatContourBuilder {
    pub contours: Vec<Vec<[f32; 2]>>,
    current: Vec<[f32; 2]>,
    ox: f64,
    oy: f64,
    scale: f64,
    flatness_sq: f64,
    /// Last pen position in font units (before origin/scale).
    pen: [f64; 2],
}

impl FlatContourBuilder {
    fn new(ox: f64, oy: f64, scale: f64, flatness: f64) -> Self {
        Self {
            contours: Vec::new(),
            current: Vec::new(),
            ox, oy, scale,
            flatness_sq: flatness * flatness,
            pen: [0.0, 0.0],
        }
    }

    #[inline]
    fn screen(&self, fx: f32, fy: f32) -> [f32; 2] {
        [(self.ox + fx as f64 * self.scale) as f32,
         (self.oy + fy as f64 * self.scale) as f32]
    }

    fn push(&mut self, fx: f32, fy: f32) {
        let pt = self.screen(fx, fy);
        self.current.push(pt);
        self.pen = [fx as f64, fy as f64];
    }

    fn flatten_quad(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p0 = [self.pen[0] as f32, self.pen[1] as f32];
        subdivide_quad(p0, [x1, y1], [x, y], self.flatness_sq, &mut self.current,
                       self.ox, self.oy, self.scale);
        let pt = self.screen(x, y);
        self.current.push(pt);
        self.pen = [x as f64, y as f64];
    }

    fn flatten_cubic(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p0 = [self.pen[0] as f32, self.pen[1] as f32];
        subdivide_cubic(p0, [x1, y1], [x2, y2], [x, y], self.flatness_sq, &mut self.current, self.ox, self.oy, self.scale);
        let pt = self.screen(x, y);
        self.current.push(pt);
        self.pen = [x as f64, y as f64];
    }

    fn flush(&mut self) {
        if self.current.len() >= 3 {
            let c = std::mem::take(&mut self.current);
            self.contours.push(c);
        } else {
            self.current.clear();
        }
    }
}

impl ttf_parser::OutlineBuilder for FlatContourBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        self.pen = [x as f64, y as f64];
        let pt = self.screen(x, y);
        self.current.push(pt);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.flatten_quad(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.flatten_cubic(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.flush();
    }
}

/// Recursively subdivide a quadratic Bézier until flat (in screen space).
///
/// Control points are in **font units**; flatness_sq and output are in
/// **screen pixels**.  Mirrors the same approach used in `subdivide_cubic`.
fn subdivide_quad(
    p0: [f32; 2], p1: [f32; 2], p2: [f32; 2],
    flatness_sq: f64,
    out: &mut Vec<[f32; 2]>,
    ox: f64, oy: f64, scale: f64,
) {
    // Convert to screen space for the flatness test.
    let s = |v: [f32; 2]| -> [f32; 2] {
        [(ox + v[0] as f64 * scale) as f32, (oy + v[1] as f64 * scale) as f32]
    };
    let sp0 = s(p0); let sp1 = s(p1); let sp2 = s(p2);
    let mx = (sp0[0] + 2.0 * sp1[0] + sp2[0]) * 0.25;
    let my = (sp0[1] + 2.0 * sp1[1] + sp2[1]) * 0.25;
    let mid_x = (sp0[0] + sp2[0]) * 0.5;
    let mid_y = (sp0[1] + sp2[1]) * 0.5;
    let dx = (mx - mid_x) as f64;
    let dy = (my - mid_y) as f64;
    if dx * dx + dy * dy <= flatness_sq {
        return; // flat enough
    }
    // Split in font units, push midpoint in screen pixels.
    let q0  = [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5];
    let q1  = [(p1[0] + p2[0]) * 0.5, (p1[1] + p2[1]) * 0.5];
    let mid = [(q0[0] + q1[0]) * 0.5, (q0[1] + q1[1]) * 0.5];
    subdivide_quad(p0,  q0, mid, flatness_sq, out, ox, oy, scale);
    out.push(s(mid));
    subdivide_quad(mid, q1, p2,  flatness_sq, out, ox, oy, scale);
}

/// Recursively subdivide a cubic Bézier until flat (in screen space).
fn subdivide_cubic(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
    flatness_sq: f64,
    out: &mut Vec<[f32; 2]>,
    ox: f64, oy: f64, scale: f64,
) {
    // Test flatness in screen space.
    let s = |v: [f32; 2]| -> [f32; 2] {
        [(ox + v[0] as f64 * scale) as f32, (oy + v[1] as f64 * scale) as f32]
    };
    let sp0 = s(p0); let sp3 = s(p3);
    let sp1 = s(p1); let sp2 = s(p2);
    let ux = 3.0 * sp1[0] - 2.0 * sp0[0] - sp3[0];
    let uy = 3.0 * sp1[1] - 2.0 * sp0[1] - sp3[1];
    let vx = 3.0 * sp2[0] - 2.0 * sp3[0] - sp0[0];
    let vy = 3.0 * sp2[1] - 2.0 * sp3[1] - sp0[1];
    let u = ux * ux + uy * uy;
    let v = vx * vx + vy * vy;
    let dist = if u > v { u } else { v } as f64;
    if dist <= flatness_sq * 16.0 {
        return;
    }
    // Split at t=0.5
    let q0 = [(p0[0]+p1[0])*0.5, (p0[1]+p1[1])*0.5];
    let q1 = [(p1[0]+p2[0])*0.5, (p1[1]+p2[1])*0.5];
    let q2 = [(p2[0]+p3[0])*0.5, (p2[1]+p3[1])*0.5];
    let r0 = [(q0[0]+q1[0])*0.5, (q0[1]+q1[1])*0.5];
    let r1 = [(q1[0]+q2[0])*0.5, (q1[1]+q2[1])*0.5];
    let mid = [(r0[0]+r1[0])*0.5, (r0[1]+r1[1])*0.5];
    subdivide_cubic(p0, q0, r0, mid, flatness_sq, out, ox, oy, scale);
    out.push(s(mid));
    subdivide_cubic(mid, r1, q2, p3, flatness_sq, out, ox, oy, scale);
}

/// Measure full text metrics (width, ascent, descent, line_height).
///
/// Useful for external rendering backends (e.g. `GlGfxCtx`) that need
/// text metrics without the `GfxCtx` wrapper.
pub fn measure_text_metrics(font: &Font, text: &str, size: f64) -> TextMetrics {
    TextMetrics {
        width:       measure_advance(font, text, size),
        ascent:      font.ascender_px(size),
        descent:     font.descender_px(size),
        line_height: font.line_height_px(size),
    }
}

/// Measure text advance width without rasterizing.
pub fn measure_advance(font: &Font, text: &str, size: f64) -> f64 {
    let scale = size / font.units_per_em() as f64;
    font.with_rb_face(|face| {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let output = rustybuzz::shape(face, &[], buffer);
        output
            .glyph_positions()
            .iter()
            .map(|p| p.x_advance as f64 * scale)
            .sum()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT_BYTES: &[u8] =
        include_bytes!("../../demo/assets/CascadiaCode.ttf");

    fn test_font() -> Arc<Font> {
        Arc::new(Font::from_slice(FONT_BYTES).expect("font ok"))
    }

    /// Verify that shape_and_flatten_text produces a sane number of
    /// contour points at typical UI font sizes.
    ///
    /// Before the fix, subdivide_quad tested flatness in font units
    /// (~2048 upm), producing ~1000 sub-divisions per Bézier segment
    /// instead of ~4 — this test would time-out or produce millions of
    /// points under the broken implementation.
    #[test]
    fn test_flatten_point_count_is_sane() {
        let font = test_font();
        let sizes: &[f64] = &[10.0, 13.0, 14.0, 24.0, 34.0];
        let texts: &[&str] = &[
            "Hello",
            "The quick brown fox",
            "Caption — 10px  The quick brown fox",
            "agg-gui",
            "Aa",
        ];

        for &size in sizes {
            for &text in texts {
                let contours =
                    shape_and_flatten_text(&font, text, size, 0.0, 0.0, 0.5);

                let total_pts: usize = contours.iter().map(|c| c.len()).sum();
                let char_count = text.chars().count().max(1);
                let pts_per_char = total_pts / char_count;

                // A well-formed glyph at any typical size should produce
                // between 4 and 300 points per character.  Anything above
                // ~500 means over-subdivision is happening again.
                assert!(
                    pts_per_char <= 500,
                    "size={size} text={text:?}: {pts_per_char} pts/char \
                     (total {total_pts}) — too many, subdivision loop likely"
                );
                assert!(
                    total_pts > 0 || text.trim().is_empty(),
                    "size={size} text={text:?}: zero points produced"
                );
            }
        }
    }

    /// Print raw contour coordinates for a single character.
    #[test]
    fn test_dump_single_char_coords() {
        use crate::gl_renderer::tessellate_fill;
        let font = test_font();
        for ch in ['W', 'i', 'd', 'g', 'e', 't', 's'] {
            let s = ch.to_string();
            let contours = shape_and_flatten_text(&font, &s, 13.0, 10.0, 50.0, 0.5);
            let total: usize = contours.iter().map(|c| c.len()).sum();
            eprintln!("{:?}: {} contours, {} pts", ch, contours.len(), total);
            // Print bounding box of each contour
            for (ci, c) in contours.iter().enumerate() {
                if c.is_empty() { continue; }
                let xs: Vec<f32> = c.iter().map(|p| p[0]).collect();
                let ys: Vec<f32> = c.iter().map(|p| p[1]).collect();
                let xmin = xs.iter().cloned().fold(f32::INFINITY, f32::min);
                let xmax = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                let ymin = ys.iter().cloned().fold(f32::INFINITY, f32::min);
                let ymax = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                eprintln!("  contour {ci}: {}/{} pts  x:[{xmin:.1},{xmax:.1}] y:[{ymin:.1},{ymax:.1}]",
                    c.len(), c.len());
            }
            let result = tessellate_fill(&contours);
            eprintln!("  tess: {:?}", result.as_ref().map(|(v,i)| (v.len()/2, i.len()/3)));
        }
    }

    /// Simulate the text draw calls that happen on the very first WASM
    /// render frame (Basics tab + window visible) and assert the full
    /// pipeline (shape → flatten → tessellate) completes in < 200 ms.
    ///
    /// This test catches both infinite-subdivision loops and algorithmic
    /// slowness that would cause a tab-kill dialog in the browser.
    /// WASM is ~5× slower than native, so 200 ms native ≈ 1 s WASM — fine.
    #[test]
    fn test_first_frame_text_pipeline_is_fast() {
        use crate::gl_renderer::tessellate_fill;
        use std::time::Instant;

        let font = test_font();
        let t0 = Instant::now();

        // All fill_text calls expected on the first rendered frame:
        //   tab bar (TabView), window title + label (Window),
        //   button labels (Button), text field placeholders (TextField).
        let calls: &[(&str, f64)] = &[
            // tab bar labels (13 pt)
            ("Basics",   13.0),
            ("Widgets",  13.0),
            ("Text",     13.0),
            ("Layout",   13.0),
            ("Tree",     13.0),
            // floating window
            ("3D Demo",                  16.0),
            ("WebGL2 — rotating cube",   11.0),
            // Basics tab buttons
            ("Primary Action",  14.0),
            ("Secondary",       14.0),
            ("Destructive",     14.0),
            // text field placeholders
            ("Type something\u{2026}",  14.0),
            ("Another field",           14.0),
        ];

        let mut total_pts  = 0usize;
        let mut total_tris = 0usize;

        for &(text, size) in calls {
            let contours = shape_and_flatten_text(&font, text, size, 10.0, 50.0, 0.5);
            total_pts += contours.iter().map(|c| c.len()).sum::<usize>();

            if let Some((verts, idx)) = tessellate_fill(&contours) {
                total_tris += idx.len() / 3;
                let _ = verts;
            }
        }

        let elapsed = t0.elapsed();

        // Sanity: we should have produced some geometry.
        assert!(total_pts  > 0,  "no contour points produced");
        assert!(total_tris > 0,  "no triangles tessellated");

        // Performance gate: must finish in under 200 ms natively.
        assert!(
            elapsed.as_millis() < 200,
            "first-frame text pipeline took {}ms (pts={total_pts} tris={total_tris}) — \
             too slow, would hang browser (WASM is ~5× slower)",
            elapsed.as_millis()
        );

        eprintln!(
            "first-frame text: {total_pts} pts, {total_tris} tris in {}ms",
            elapsed.as_millis()
        );
    }

    /// Verify that all contour points are in screen-pixel range for the
    /// given font size (not left in raw font units).
    #[test]
    fn test_flatten_output_is_in_screen_space() {
        let font = test_font();
        // Place text at (100, 200) at size 16.
        let contours =
            shape_and_flatten_text(&font, "Hello", 16.0, 100.0, 200.0, 0.5);

        assert!(!contours.is_empty(), "should produce contours for 'Hello'");

        for (ci, contour) in contours.iter().enumerate() {
            for &[x, y] in contour {
                // Screen-space points should be near (100±50, 200±30) at 16pt.
                // Font-unit coordinates would be in the hundreds–thousands.
                assert!(
                    x > 50.0 && x < 300.0,
                    "contour {ci}: x={x} looks like font units, not screen px"
                );
                assert!(
                    y > 150.0 && y < 280.0,
                    "contour {ci}: y={y} looks like font units, not screen px"
                );
            }
        }
    }
}
