//! LCD subpixel text as a **per-channel coverage mask** that composites
//! onto arbitrary backgrounds — no bg pre-fill, no destination-color
//! knowledge required at rasterization time.
//!
//! # Why this replaces the pre-fill approach
//!
//! The older `PixfmtRgba32Lcd` path baked the caller's background colour
//! into the rasterised output via a per-channel src-over against the
//! pre-filled framebuffer.  That coupled the LCD glyphs to one specific
//! destination and forced us to know that destination everywhere text is
//! drawn — driving the walk / sample / push / pop complexity.
//!
//! Instead, we keep the **three subpixel coverage values independent**:
//! the output of the rasteriser is three 8-bit channels per pixel
//! `(cov_r, cov_g, cov_b)` describing how much of each subpixel the glyph
//! covered.  At composite time a per-channel Porter-Duff `over` blend
//! mixes the TEXT COLOUR into the live destination:
//!
//! ```text
//! dst.r = src.r * cov.r + dst.r * (1 - cov.r)
//! dst.g = src.g * cov.g + dst.g * (1 - cov.g)
//! dst.b = src.b * cov.b + dst.b * (1 - cov.b)
//! ```
//!
//! The coverage mask is the same regardless of where it lands; the blend
//! naturally produces the correct LCD chroma against any background.
//!
//! See `lcd-subpixel-compositing.md` at the repository root for the full
//! derivation.
//!
//! # Pipeline
//!
//! ```text
//! shape_text (rustybuzz kerning + fallback chain — unchanged)
//!   │
//! per-glyph PathStorage → ConvTransform(scale_x_3) → PixfmtGray8
//!   (8-bit grayscale coverage at 3× horizontal resolution)
//!   │
//! 5-tap low-pass filter per output channel
//!   │
//! packed (cov_r, cov_g, cov_b) 3-byte mask
//! ```

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use agg_rust::color::Gray8;
use agg_rust::conv_curve::ConvCurve;
use agg_rust::conv_transform::ConvTransform;

// ---------------------------------------------------------------------------
// LcdBuffer — opaque 3-byte-per-pixel RGB render target
// ---------------------------------------------------------------------------
//
// Analogue of `Framebuffer` for widgets that opt into
// [`crate::widget::BackbufferMode::LcdCoverage`].  Every fill into an
// `LcdBuffer` goes through the 3× horizontal supersample + 5-tap filter
// pipeline and composites per-channel via Porter-Duff src-over.  The
// buffer has no alpha channel — it's intended to be fully covered by
// opaque fills and blitted as an opaque RGB texture.

/// RGB framebuffer, row 0 = bottom (matches `Framebuffer` convention).
/// 3 bytes per pixel: `(R, G, B)` composited result of every fill so far.
pub struct LcdBuffer {
    pixels: Vec<u8>,
    width:  u32,
    height: u32,
}

impl LcdBuffer {
    /// Allocate a zeroed buffer (all pixels black = `(0, 0, 0)`).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            pixels: vec![0u8; (width as usize) * (height as usize) * 3],
            width,
            height,
        }
    }

    #[inline] pub fn width(&self)  -> u32     { self.width }
    #[inline] pub fn height(&self) -> u32     { self.height }
    #[inline] pub fn pixels(&self) -> &[u8]   { &self.pixels }
    #[inline] pub fn pixels_mut(&mut self) -> &mut [u8] { &mut self.pixels }

    /// Consume the buffer and hand ownership of the underlying
    /// `Vec<u8>` — used when moving the rendered pixels into an
    /// `Arc` for the widget's backbuffer cache.
    pub fn into_pixels(self) -> Vec<u8> { self.pixels }

    /// Top-row-first copy of the pixels — matches the convention used
    /// by `draw_image_rgba_arc` (images uploaded as textures expect
    /// row 0 at top).  One-time flip on cache build.
    pub fn pixels_flipped(&self) -> Vec<u8> {
        let row_bytes = (self.width * 3) as usize;
        let mut out = vec![0u8; self.pixels.len()];
        for y in 0..self.height as usize {
            let src = &self.pixels[y * row_bytes .. (y + 1) * row_bytes];
            let dst_y = self.height as usize - 1 - y;
            out[dst_y * row_bytes .. (dst_y + 1) * row_bytes].copy_from_slice(src);
        }
        out
    }

    // ── Paint primitives ────────────────────────────────────────────────────
    //
    // These are the foundation operations every higher layer (LcdGfxCtx,
    // path-fill helpers, image blit) eventually composes into.  They write
    // directly into the 3-byte-per-pixel coverage store with no intermediate
    // allocation.

    /// Fill the entire buffer with a solid colour.  Treats `color` as
    /// fully covering every subpixel of every pixel — the natural
    /// interpretation for a bg fill on an opaque LCD surface.  Alpha is
    /// applied to all three channels equally; partial alpha produces a
    /// uniformly attenuated colour, not a translucent pixel (LcdBuffer
    /// has no alpha channel — partial alpha against a black initial
    /// buffer simply darkens the result).
    pub fn clear(&mut self, color: Color) {
        let a  = color.a.clamp(0.0, 1.0);
        let r8 = ((color.r.clamp(0.0, 1.0) * a) * 255.0 + 0.5) as u8;
        let g8 = ((color.g.clamp(0.0, 1.0) * a) * 255.0 + 0.5) as u8;
        let b8 = ((color.b.clamp(0.0, 1.0) * a) * 255.0 + 0.5) as u8;
        for px in self.pixels.chunks_exact_mut(3) {
            px[0] = r8;
            px[1] = g8;
            px[2] = b8;
        }
    }

    /// Fill an AGG path through the LCD pipeline: rasterize at 3× X
    /// resolution → 5-tap filter → per-channel src-over composite into
    /// this buffer.  `transform` is applied to `path` before the 3× X
    /// scale (typically the caller's CTM); the path's coordinates are
    /// in the buffer's pixel space (Y-up, origin = bottom-left).
    /// Optional `clip` is a screen-space rect (post-CTM, in mask pixel
    /// coords) — pixels outside it are unaffected.
    ///
    /// First non-text primitive on the buffer.  Future fill / stroke /
    /// image-blit entry points either call this directly (for solid
    /// fills / outlines) or open their own `LcdMaskBuilder` scope when
    /// they need to batch many paths into one mask.
    ///
    /// First-cut implementation: rasterizes at the buffer's full size.
    /// A later optimization can compute the path's bbox and size the
    /// scratch tightly — measurable win for small paths in large
    /// buffers, but architecturally identical and not required for
    /// correctness.
    pub fn fill_path(
        &mut self,
        path:      &mut PathStorage,
        color:     Color,
        transform: &TransAffine,
        clip:      Option<(f64, f64, f64, f64)>,
    ) {
        if self.width == 0 || self.height == 0 { return; }
        let mut builder = LcdMaskBuilder::new(self.width, self.height).with_clip(clip);
        builder.with_paths(transform, |add| { add(path); });
        let mask = builder.finalize();
        // Convert clip → integer pixel rect for composite-time enforcement.
        // The gray-buffer raster clip should already have zeroed coverage
        // outside, but the 5-tap filter can leak ±2 subpixels at clip
        // edges; composite-time clip catches that.
        let clip_i = clip.map(rect_to_pixel_clip);
        self.composite_mask(&mask, color, 0, 0, clip_i);
    }

    /// Composite an [`LcdMask`] into this buffer using per-channel
    /// Porter-Duff src-over.  Each subpixel mixes `src` colour into the
    /// stored value by its own coverage:
    ///
    /// ```text
    /// dst.r = src.r * cov.r + dst.r * (1 - cov.r)
    /// dst.g = src.g * cov.g + dst.g * (1 - cov.g)
    /// dst.b = src.b * cov.b + dst.b * (1 - cov.b)
    /// ```
    ///
    /// `(dst_x, dst_y)` is the mask's bottom-left in this buffer's Y-up
    /// pixel grid; mask row `my` writes to buffer row `dst_y + my`.
    /// Optional `clip` (in this buffer's integer pixel coords:
    /// `(x1, y1, x2, y2)`, half-open) suppresses writes outside its
    /// bounds — used by widgets that paint inside a clipping parent.
    /// Mirrors [`composite_lcd_mask`] but writes into the 3-byte/pixel
    /// LcdBuffer instead of a 4-byte/pixel RGBA destination.
    pub fn composite_mask(
        &mut self,
        mask:  &LcdMask,
        src:   Color,
        dst_x: i32,
        dst_y: i32,
        clip:  Option<(i32, i32, i32, i32)>,
    ) {
        if mask.width == 0 || mask.height == 0 { return; }
        let sa = src.a.clamp(0.0, 1.0);
        let sr = src.r.clamp(0.0, 1.0);
        let sg = src.g.clamp(0.0, 1.0);
        let sb = src.b.clamp(0.0, 1.0);
        let dst_w_i = self.width  as i32;
        let dst_h_i = self.height as i32;
        let dst_w_u = self.width as usize;
        let mw = mask.width  as i32;
        let mh = mask.height as i32;
        // Intersect clip with the buffer's bounds up-front so the inner
        // loop can do a single range check per pixel.
        let (cx1, cy1, cx2, cy2) = match clip {
            Some((cx1, cy1, cx2, cy2)) =>
                (cx1.max(0), cy1.max(0), cx2.min(dst_w_i), cy2.min(dst_h_i)),
            None => (0, 0, dst_w_i, dst_h_i),
        };
        if cx1 >= cx2 || cy1 >= cy2 { return; }

        for my in 0..mh {
            let dy = dst_y + my;
            if dy < cy1 || dy >= cy2 { continue; }
            let dy_u = dy as usize;
            for mx in 0..mw {
                let dx = dst_x + mx;
                if dx < cx1 || dx >= cx2 { continue; }
                let mi = ((my * mw + mx) * 3) as usize;
                // Source colour modulates coverage by its own alpha, then
                // composites per channel.  Alpha-zero src → no-op; alpha-one
                // src reproduces the original mask formula exactly.
                let cr = (mask.data[mi]     as f32 / 255.0) * sa;
                let cg = (mask.data[mi + 1] as f32 / 255.0) * sa;
                let cb = (mask.data[mi + 2] as f32 / 255.0) * sa;
                if cr == 0.0 && cg == 0.0 && cb == 0.0 { continue; }
                let di = (dy_u * dst_w_u + (dx as usize)) * 3;
                let dr = self.pixels[di]     as f32 / 255.0;
                let dg = self.pixels[di + 1] as f32 / 255.0;
                let db = self.pixels[di + 2] as f32 / 255.0;
                let rr = sr * cr + dr * (1.0 - cr);
                let rg = sg * cg + dg * (1.0 - cg);
                let rb = sb * cb + db * (1.0 - cb);
                self.pixels[di]     = (rr * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                self.pixels[di + 1] = (rg * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                self.pixels[di + 2] = (rb * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
            }
        }
    }

    /// Copy `src` onto this buffer at offset `(dst_x, dst_y)`, replacing
    /// every destination pixel with the corresponding source pixel.
    /// Optional `clip` (in this buffer's integer pixel coords, half-open)
    /// suppresses writes outside it.
    ///
    /// Used by [`crate::lcd_gfx_ctx::LcdGfxCtx::pop_layer`] to flatten a
    /// pushed sub-layer back into its parent.  Semantics differ from
    /// RGBA layer compositing (which uses SrcOver alpha): an `LcdBuffer`
    /// has no alpha channel, so we can't distinguish "untouched" from
    /// "intentionally black".  Layers in LCD coverage mode therefore
    /// **fully replace** the destination region — matching the
    /// `LcdCoverage` widget contract that mandates opaque coverage of
    /// the full bounds.  Widgets that need translucent overlays should
    /// not opt into LCD coverage mode in the first place.
    pub fn composite_buffer(
        &mut self,
        src:   &LcdBuffer,
        dst_x: i32,
        dst_y: i32,
        clip:  Option<(i32, i32, i32, i32)>,
    ) {
        if src.width == 0 || src.height == 0 { return; }
        let dst_w_i = self.width  as i32;
        let dst_h_i = self.height as i32;
        let dst_w_u = self.width as usize;
        let src_w_u = src.width  as usize;
        let sw = src.width  as i32;
        let sh = src.height as i32;
        let (cx1, cy1, cx2, cy2) = match clip {
            Some((x1, y1, x2, y2)) =>
                (x1.max(0), y1.max(0), x2.min(dst_w_i), y2.min(dst_h_i)),
            None => (0, 0, dst_w_i, dst_h_i),
        };
        if cx1 >= cx2 || cy1 >= cy2 { return; }

        for sy in 0..sh {
            let dy = dst_y + sy;
            if dy < cy1 || dy >= cy2 { continue; }
            let dy_u = dy as usize;
            let sy_u = sy as usize;
            for sx in 0..sw {
                let dx = dst_x + sx;
                if dx < cx1 || dx >= cx2 { continue; }
                let si = (sy_u * src_w_u + sx as usize) * 3;
                let di = (dy_u * dst_w_u + dx as usize) * 3;
                self.pixels[di]     = src.pixels[si];
                self.pixels[di + 1] = src.pixels[si + 1];
                self.pixels[di + 2] = src.pixels[si + 2];
            }
        }
    }
}
use agg_rust::path_storage::PathStorage;
use agg_rust::pixfmt_gray::PixfmtGray8;
use agg_rust::rasterizer_scanline_aa::RasterizerScanlineAa;
use agg_rust::renderer_base::RendererBase;
use agg_rust::renderer_scanline::render_scanlines_aa_solid;
use agg_rust::rendering_buffer::RowAccessor;
use agg_rust::scanline_u::ScanlineU8;
use agg_rust::trans_affine::TransAffine;

use crate::color::Color;
use crate::text::{measure_text_metrics, shape_text, Font};

/// Identity transform — exposed so call sites that don't otherwise
/// depend on `agg_rust::trans_affine::TransAffine` can pass one.
pub fn identity_xform() -> TransAffine { TransAffine::new() }

// ---------------------------------------------------------------------------
// Cached LCD text raster
// ---------------------------------------------------------------------------
//
// The mask is fully determined by `(text, font_ptr, font_size)` — colour is
// applied at composite time, and placement coordinates are just translations
// the caller handles.  Caching keeps `fill_text` roughly as fast as the old
// grayscale path: AGG rasterisation runs once per unique text string, and
// GL backends can further cache the uploaded texture keyed on the returned
// `Arc`'s pointer identity (see `demo-gl`'s `arc_texture_cache` pattern).

/// Result of [`rasterize_text_lcd_cached`].  Callers composite the mask
/// at `(x - baseline_x_in_mask, y - baseline_y_in_mask)` where `(x, y)`
/// is the target baseline position in local / screen coordinates.
pub struct CachedLcdText {
    /// 3-byte-per-pixel coverage mask, Y-up (row 0 = bottom).  Shared
    /// `Arc` so GL backends can key a texture cache on its pointer
    /// identity — one upload per unique raster result.
    pub pixels:              Arc<Vec<u8>>,
    pub width:               u32,
    pub height:              u32,
    /// Mask-local x of the glyph origin (= padding inset).
    pub baseline_x_in_mask:  f64,
    /// Mask-local Y-up y of the glyph baseline.
    pub baseline_y_in_mask:  f64,
}

const MASK_PAD: f64 = 2.0;

#[derive(Clone, PartialEq, Eq, Hash)]
struct LcdMaskKey {
    text:      String,
    font_ptr:  usize,
    size_bits: u64,
}

struct LcdMaskEntry {
    pixels:              Arc<Vec<u8>>,
    width:               u32,
    height:              u32,
    baseline_x_in_mask:  f64,
    baseline_y_in_mask:  f64,
}

thread_local! {
    static MASK_CACHE: RefCell<HashMap<LcdMaskKey, LcdMaskEntry>>
        = RefCell::new(HashMap::new());
    static MASK_LRU: RefCell<VecDeque<LcdMaskKey>>
        = RefCell::new(VecDeque::new());
}

const MASK_CACHE_MAX: usize = 1024;

/// Rasterise `text` in `font` at `size` into a 3-channel LCD coverage mask,
/// caching the result so subsequent calls with the same `(text, font, size)`
/// return the shared `Arc` without re-running AGG.
pub fn rasterize_text_lcd_cached(
    font: &Arc<Font>,
    text: &str,
    size: f64,
) -> CachedLcdText {
    let key = LcdMaskKey {
        text:      text.to_string(),
        font_ptr:  Arc::as_ptr(font) as *const () as usize,
        size_bits: size.to_bits(),
    };
    // Cache hit path — bump LRU, return shared Arc.
    let hit = MASK_CACHE.with(|m| {
        m.borrow().get(&key).map(|e| CachedLcdText {
            pixels:             Arc::clone(&e.pixels),
            width:              e.width,
            height:             e.height,
            baseline_x_in_mask: e.baseline_x_in_mask,
            baseline_y_in_mask: e.baseline_y_in_mask,
        })
    });
    if let Some(got) = hit {
        MASK_LRU.with(|lru| {
            let mut lru = lru.borrow_mut();
            // Move key to back (most recently used).
            if let Some(pos) = lru.iter().position(|k| k == &key) {
                lru.remove(pos);
            }
            lru.push_back(key);
        });
        return got;
    }

    // Cache miss — run the rasteriser.
    let m   = measure_text_metrics(font, text, size);
    let bw  = (m.width  + MASK_PAD * 2.0).ceil().max(1.0) as u32;
    let bh  = (m.ascent + m.descent + MASK_PAD * 2.0).ceil().max(1.0) as u32;
    let bx  = MASK_PAD;
    let by  = MASK_PAD + m.descent;
    let mask = rasterize_lcd_mask(
        font, text, size, bx, by, bw, bh, &TransAffine::new(),
    );
    let pixels = Arc::new(mask.data);
    let entry = LcdMaskEntry {
        pixels:              Arc::clone(&pixels),
        width:               bw,
        height:              bh,
        baseline_x_in_mask:  bx,
        baseline_y_in_mask:  by,
    };

    MASK_CACHE.with(|m| m.borrow_mut().insert(key.clone(), entry));
    MASK_LRU.with(|lru| {
        let mut lru = lru.borrow_mut();
        lru.push_back(key.clone());
        // LRU evict to cap — drop the oldest Arc strong refs so GL
        // texture caches holding a Weak will see them expire and
        // release their textures.
        while lru.len() > MASK_CACHE_MAX {
            if let Some(old) = lru.pop_front() {
                MASK_CACHE.with(|m| m.borrow_mut().remove(&old));
            }
        }
    });

    CachedLcdText {
        pixels,
        width:              bw,
        height:             bh,
        baseline_x_in_mask: bx,
        baseline_y_in_mask: by,
    }
}

/// 3-byte-per-pixel LCD coverage mask.  Callers composite via
/// [`composite_lcd_mask`].  The distinction from a normal RGBA image is
/// crucial: the three channels are **independent coverage values**, not
/// an RGB colour — they drive a per-channel blend where each subpixel
/// mixes the source colour with the destination colour by its own amount.
pub struct LcdMask {
    pub data:   Vec<u8>, // len = width * height * 3, stride = width * 3
    pub width:  u32,
    pub height: u32,
}

/// FreeType-default 5-tap weights; sum = 9.  Heavier filter weights reduce
/// colour fringing at the cost of sharpness; tuning against this table is
/// the standard knob for "darker / lighter" LCD text.
const FILTER_WEIGHTS: [u32; 5] = [1, 2, 3, 2, 1];
const FILTER_SUM:     u32       = 9;

/// Rasterize `text` at baseline `(x, y)` into a 3-channel coverage mask
/// of size `mask_w × mask_h`.  `transform` is applied before the 3× X
/// scale that puts the path into the high-resolution grayscale buffer.
///
/// The returned mask has **no colour**; at composite time `composite_lcd_mask`
/// mixes the caller's desired text colour into the destination through the
/// per-channel coverage.
pub fn rasterize_lcd_mask(
    font:      &Font,
    text:      &str,
    size:      f64,
    x:         f64,
    y:         f64,
    mask_w:    u32,
    mask_h:    u32,
    transform: &TransAffine,
) -> LcdMask {
    rasterize_lcd_mask_multi(font, &[(text, x, y)], size, mask_w, mask_h, transform)
}

/// Multi-span variant: raster several `(text, x, y)` tuples into a
/// single mask.  Used by wrapped-text `Label` so every line shares one
/// 3×-wide gray buffer and one filter pass.  The gray buffer is written
/// cumulatively by AGG (glyphs in different pixels don't interact, so
/// non-overlapping lines just occupy disjoint rows).
///
/// Now a thin wrapper over [`LcdMaskBuilder`] — kept as a free function
/// because the cached text path keys on `(text, font, size)` and never
/// needs to interleave non-text paths.  Generic callers should reach
/// for the builder directly.
pub fn rasterize_lcd_mask_multi(
    font:      &Font,
    spans:     &[(&str, f64, f64)],
    size:      f64,
    mask_w:    u32,
    mask_h:    u32,
    transform: &TransAffine,
) -> LcdMask {
    let mut builder = LcdMaskBuilder::new(mask_w, mask_h);
    builder.with_paths(transform, |add| {
        for (text, x, y) in spans {
            if text.is_empty() { continue; }
            let (mut paths, _) = shape_text(font, text, size, *x, *y);
            for path in paths.iter_mut() {
                add(path);
            }
        }
    });
    builder.finalize()
}

/// Convert a screen-space float clip rect `(x, y, w, h)` to the
/// integer pixel clip box `(x1, y1, x2, y2)` (half-open) used by
/// [`LcdBuffer::composite_mask`].  Floor on the left/bottom and ceil on
/// the right/top so any pixel touched by the clip rect (even partially)
/// is included — matches the AGG raster-clip convention.
pub fn rect_to_pixel_clip(rect: (f64, f64, f64, f64)) -> (i32, i32, i32, i32) {
    let (x, y, w, h) = rect;
    (
        x.floor() as i32,
        y.floor() as i32,
        (x + w).ceil() as i32,
        (y + h).ceil() as i32,
    )
}

// ── LcdMaskBuilder ──────────────────────────────────────────────────────────
//
// Lifts the inner "rasterize one or more AGG paths at 3× X resolution →
// 5-tap low-pass filter → packed 3-byte LCD coverage mask" pipeline out
// of the text-only entry points so any path source can drive it.  This
// is the seam any new caller (rect fill, stroke, future widget paint)
// hooks into when it needs LCD-aware coverage output.

/// Accumulator for an [`LcdMask`].  Build the gray buffer with one or
/// more `with_paths` calls (each opens an AGG rasterizer scope), then
/// `finalize` to apply the 5-tap filter and produce the packed mask.
pub struct LcdMaskBuilder {
    gray:   Vec<u8>,
    gray_w: u32,
    gray_h: u32,
    mask_w: u32,
    mask_h: u32,
    /// Optional screen-space clip rect (in mask pixel coords, post-CTM).
    /// Applied to the AGG renderer as a `clip_box_i` with X scaled by 3
    /// before any path is added, so any rasterised coverage outside the
    /// clip gets dropped at raster time (no need to also clip during
    /// the filter pass — zero gray = zero mask).
    clip:   Option<(f64, f64, f64, f64)>,
}

impl LcdMaskBuilder {
    /// Allocate a zeroed builder for an `mask_w × mask_h` output mask.
    /// The internal gray buffer is `(3 × mask_w) × mask_h` bytes.
    pub fn new(mask_w: u32, mask_h: u32) -> Self {
        let gray_w = mask_w.saturating_mul(3);
        let gray_h = mask_h;
        let gray   = vec![0u8; (gray_w as usize) * (gray_h as usize)];
        Self { gray, gray_w, gray_h, mask_w, mask_h, clip: None }
    }

    /// Set a clip rectangle in screen-space (mask pixel coords).  All
    /// subsequent `with_paths` calls render only inside the clip;
    /// pixels outside it stay zero in the gray buffer (and therefore
    /// produce zero coverage in the final filtered mask).  Builder-style;
    /// chain after `new`.
    pub fn with_clip(mut self, clip: Option<(f64, f64, f64, f64)>) -> Self {
        self.clip = clip;
        self
    }

    /// Open an AGG rasterizer scope and let `f` add as many paths as
    /// it likes via the supplied `&mut FnMut(&mut PathStorage)`.  All
    /// paths share `transform`, with X supersampled by 3 inside the
    /// scope.  Lifetimes prevent us from keeping the renderer alive
    /// across separate method calls (it borrows `self.gray`), so the
    /// closure pattern scopes the borrow precisely.
    pub fn with_paths<F>(&mut self, transform: &TransAffine, f: F)
    where F: FnOnce(&mut dyn FnMut(&mut PathStorage)),
    {
        rasterize_paths_into_gray(
            &mut self.gray, self.gray_w, self.gray_h, transform, self.clip, f,
        );
    }

    /// Apply the 5-tap low-pass filter to the gray buffer and return
    /// the packed mask.  Consumes the builder; callers usually composite
    /// the result via [`LcdBuffer::composite_mask`] or
    /// [`composite_lcd_mask`].
    pub fn finalize(self) -> LcdMask {
        if self.mask_w == 0 || self.mask_h == 0 {
            return LcdMask { data: Vec::new(), width: self.mask_w, height: self.mask_h };
        }
        let data = apply_5_tap_filter(
            &self.gray, self.gray_w, self.mask_w, self.mask_h,
        );
        LcdMask { data, width: self.mask_w, height: self.mask_h }
    }
}

/// Internal: run one AGG rasterizer scope writing into `gray` at 3× X
/// scale.  The closure receives an `add` function that takes a mutable
/// `PathStorage` and renders it with curve flattening + the X-scaled
/// transform applied.  Optional `clip` (in mask pixel coords) is
/// applied to the renderer with X scaled by 3 to match the gray
/// buffer; rasterised coverage outside the clip is dropped at raster
/// time.
fn rasterize_paths_into_gray<F>(
    gray:      &mut [u8],
    gray_w:    u32,
    gray_h:    u32,
    transform: &TransAffine,
    clip:      Option<(f64, f64, f64, f64)>,
    f:         F,
)
where F: FnOnce(&mut dyn FnMut(&mut PathStorage)),
{
    if gray_w == 0 || gray_h == 0 { return; }
    let stride = gray_w as i32;
    let mut ra = RowAccessor::new();
    unsafe { ra.attach(gray.as_mut_ptr(), gray_w, gray_h, stride); }
    let pf = PixfmtGray8::new(&mut ra);
    let mut rb  = RendererBase::new(pf);
    if let Some((cx, cy, cw, ch)) = clip {
        // Clip box is in mask pixel coords.  The gray buffer is 3× X,
        // so multiply X bounds by 3 to land on the right subpixels.
        // `clip_box_i` is inclusive on both ends, so the right/top
        // edges use `-1` after the ceil.
        let x1 = (cx.floor() as i32).saturating_mul(3);
        let y1 = cy.floor() as i32;
        let x2 = ((cx + cw).ceil() as i32).saturating_mul(3) - 1;
        let y2 = (cy + ch).ceil() as i32 - 1;
        rb.clip_box_i(x1, y1, x2, y2);
    }
    let mut ras = RasterizerScanlineAa::new();
    let mut sl  = ScanlineU8::new();

    // Full coverage = 255.  AGG writes `gray_value * alpha / 255` per
    // pixel; with value = 255 the output byte equals AGG's coverage
    // estimate at that pixel — exactly what the 5-tap filter expects
    // as input.
    let cov_color = Gray8::new_opaque(255);

    let mut xform = *transform;
    xform.sx  *= 3.0;
    xform.shx *= 3.0;
    xform.tx  *= 3.0;
    // shy, sy, ty unchanged — only X is supersampled.

    let mut add = |path: &mut PathStorage| {
        let mut curves = ConvCurve::new(path);
        let mut tx     = ConvTransform::new(&mut curves, xform);
        ras.reset();
        ras.add_path(&mut tx, 0);
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &cov_color);
    };
    f(&mut add);
}

/// Internal: run the 5-tap low-pass filter over `gray` and produce the
/// packed `(R,G,B)` mask.  See module docs for the per-channel formula
/// and phase shift.
fn apply_5_tap_filter(gray: &[u8], gray_w: u32, mask_w: u32, mask_h: u32) -> Vec<u8> {
    let mut data = vec![0u8; (mask_w as usize) * (mask_h as usize) * 3];
    let gw = gray_w as i32;
    for py in 0..mask_h {
        let row_start = (py as usize) * (gray_w as usize);
        let row = &gray[row_start .. row_start + gray_w as usize];
        for px in 0..mask_w {
            let base = (px as i32) * 3;
            let sample = |off: i32| -> u32 {
                let pos = base + off;
                if pos < 0 || pos >= gw { 0 } else { row[pos as usize] as u32 }
            };
            // R samples [-2..=2], G shifts +1, B shifts +2 (phase offsets
            // between the three physical subpixels of the output pixel).
            let cov_r = (FILTER_WEIGHTS[0] * sample(-2)
                       + FILTER_WEIGHTS[1] * sample(-1)
                       + FILTER_WEIGHTS[2] * sample(0)
                       + FILTER_WEIGHTS[3] * sample(1)
                       + FILTER_WEIGHTS[4] * sample(2)) / FILTER_SUM;
            let cov_g = (FILTER_WEIGHTS[0] * sample(-1)
                       + FILTER_WEIGHTS[1] * sample(0)
                       + FILTER_WEIGHTS[2] * sample(1)
                       + FILTER_WEIGHTS[3] * sample(2)
                       + FILTER_WEIGHTS[4] * sample(3)) / FILTER_SUM;
            let cov_b = (FILTER_WEIGHTS[0] * sample(0)
                       + FILTER_WEIGHTS[1] * sample(1)
                       + FILTER_WEIGHTS[2] * sample(2)
                       + FILTER_WEIGHTS[3] * sample(3)
                       + FILTER_WEIGHTS[4] * sample(4)) / FILTER_SUM;
            let mi = ((py as usize) * (mask_w as usize) + (px as usize)) * 3;
            data[mi]     = cov_r.min(255) as u8;
            data[mi + 1] = cov_g.min(255) as u8;
            data[mi + 2] = cov_b.min(255) as u8;
        }
    }
    data
}

/// Composite an [`LcdMask`] onto `dst_rgba` using per-channel Porter-Duff
/// "over": each subpixel mixes `src_color` into the live destination by
/// its own coverage.  The destination colour is whatever pixels are
/// currently at the target rect — so this works over any background.
///
/// Both the mask and `dst_rgba` are **Y-up** (row 0 = bottom), matching
/// `agg-gui`'s `Framebuffer` convention.  `(dst_x, dst_y)` is the mask's
/// bottom-left in the destination's Y-up pixel grid; mask row `my` is
/// written to destination row `dst_y + my`.
pub fn composite_lcd_mask(
    dst_rgba: &mut [u8],
    dst_w:    u32,
    dst_h:    u32,
    mask:     &LcdMask,
    src:      Color,
    dst_x:    i32,
    dst_y:    i32,
) {
    if mask.width == 0 || mask.height == 0 { return; }
    let sr = src.r.clamp(0.0, 1.0);
    let sg = src.g.clamp(0.0, 1.0);
    let sb = src.b.clamp(0.0, 1.0);
    let dst_w_i = dst_w as i32;
    let dst_h_i = dst_h as i32;
    let mw = mask.width  as i32;
    let mh = mask.height as i32;

    for my in 0..mh {
        // Both buffers Y-up: mask row my → dst row dst_y + my.
        let dy = dst_y + my;
        if dy < 0 || dy >= dst_h_i { continue; }
        for mx in 0..mw {
            let dx = dst_x + mx;
            if dx < 0 || dx >= dst_w_i { continue; }
            let mi = ((my * mw + mx) * 3) as usize;
            let cr = mask.data[mi]     as f32 / 255.0;
            let cg = mask.data[mi + 1] as f32 / 255.0;
            let cb = mask.data[mi + 2] as f32 / 255.0;
            if cr == 0.0 && cg == 0.0 && cb == 0.0 { continue; }

            let di = ((dy * dst_w_i + dx) * 4) as usize;
            let dr = dst_rgba[di]     as f32 / 255.0;
            let dg = dst_rgba[di + 1] as f32 / 255.0;
            let db = dst_rgba[di + 2] as f32 / 255.0;

            // Per-channel source-over in sRGB space.  Gamma-aware
            // linearization is the correct next step (see the design
            // doc); sRGB-direct is adequate for first-cut validation
            // and matches what FreeType does in its non-linear mode.
            let rr = sr * cr + dr * (1.0 - cr);
            let rg = sg * cg + dg * (1.0 - cg);
            let rbb = sb * cb + db * (1.0 - cb);

            dst_rgba[di]     = (rr  * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
            dst_rgba[di + 1] = (rg  * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
            dst_rgba[di + 2] = (rbb * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
            // Alpha unchanged — mask composites onto the existing dst
            // without introducing transparency.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const FONT_BYTES: &[u8] =
        include_bytes!("../../demo/assets/CascadiaCode.ttf");

    fn font() -> Arc<Font> {
        Arc::new(Font::from_slice(FONT_BYTES).expect("font"))
    }

    /// The rasteriser must produce some non-zero coverage for ordinary
    /// text — sanity check that the pipeline wires up at all.
    #[test]
    fn test_lcd_mask_has_coverage() {
        let mask = rasterize_lcd_mask(
            &font(), "Hello", 16.0, 4.0, 12.0,
            200, 40, &TransAffine::new(),
        );
        let total: u64 = mask.data.iter().map(|&b| b as u64).sum();
        assert!(total > 0, "rasterize_lcd_mask produced all-zero coverage");
    }

    /// Edge pixels must exhibit **per-channel variation** — the
    /// defining property of LCD subpixel rendering.  Without the 5-tap
    /// filter's phase shift between R/G/B, the three channels would be
    /// identical at every pixel.
    #[test]
    fn test_lcd_mask_has_channel_variation() {
        let mask = rasterize_lcd_mask(
            &font(), "Wing", 24.0, 4.0, 16.0,
            400, 40, &TransAffine::new(),
        );
        let mut saw = false;
        for px in mask.data.chunks_exact(3) {
            let r = px[0];
            let g = px[1];
            let b = px[2];
            let mx = r.max(g).max(b);
            let mn = r.min(g).min(b);
            if mx > 20 && (mx - mn) > 10 {
                saw = true;
                break;
            }
        }
        assert!(saw, "no per-channel variation at edges");
    }

    /// Compositing the mask must mix text into any destination bg and
    /// produce plausibly darker pixels for dark-on-light, and plausibly
    /// lighter pixels for light-on-dark, regardless of which bg the mask
    /// was rastered against (it wasn't rastered against any).
    #[test]
    fn test_composite_dark_on_light_and_light_on_dark() {
        let mask = rasterize_lcd_mask(
            &font(), "Hi", 20.0, 2.0, 14.0,
            80, 24, &TransAffine::new(),
        );

        // Dark text on white.
        let mut fb_white = vec![255u8; 80 * 24 * 4];
        composite_lcd_mask(&mut fb_white, 80, 24, &mask, Color::black(), 0, 0);
        let sum_white: u64 = fb_white.chunks_exact(4)
            .map(|p| (p[0] as u64 + p[1] as u64 + p[2] as u64))
            .sum();
        assert!(sum_white < 80 * 24 * 3 * 255,
                "dark-on-white composite left every pixel white");

        // Light text on black.
        let mut fb_black = vec![0u8; 80 * 24 * 4];
        for chunk in fb_black.chunks_exact_mut(4) { chunk[3] = 255; }
        composite_lcd_mask(&mut fb_black, 80, 24, &mask, Color::white(), 0, 0);
        let sum_black: u64 = fb_black.chunks_exact(4)
            .map(|p| (p[0] as u64 + p[1] as u64 + p[2] as u64))
            .sum();
        assert!(sum_black > 0,
                "light-on-black composite left every pixel black");
    }

    // ── LcdBuffer paint primitives ──────────────────────────────────────────

    /// `LcdBuffer::clear` writes the requested colour into every pixel.
    /// Premultiplied alpha applies uniformly across all three channels —
    /// the buffer has no alpha store, so partial-alpha is realised by
    /// darkening the colour, not by storing transparency.
    #[test]
    fn test_lcd_buffer_clear_writes_solid_color() {
        let mut buf = LcdBuffer::new(4, 3);
        buf.clear(Color::rgba(1.0, 0.5, 0.25, 1.0));
        for px in buf.pixels().chunks_exact(3) {
            assert_eq!(px[0], 255);
            assert_eq!(px[1], 128);
            assert_eq!(px[2], 64);
        }

        // Half-alpha → premultiplied colour at half intensity.
        let mut buf2 = LcdBuffer::new(2, 2);
        buf2.clear(Color::rgba(1.0, 1.0, 1.0, 0.5));
        for px in buf2.pixels().chunks_exact(3) {
            assert_eq!(px[0], 128);
            assert_eq!(px[1], 128);
            assert_eq!(px[2], 128);
        }
    }

    /// Compositing a non-empty mask onto a cleared buffer must leave at
    /// least some pixels modified — proves the path connects.
    #[test]
    fn test_lcd_buffer_composite_mask_deposits_coverage() {
        let mask = rasterize_lcd_mask(
            &font(), "Hi", 20.0, 2.0, 14.0,
            80, 24, &TransAffine::new(),
        );
        let mut buf = LcdBuffer::new(80, 24);
        buf.clear(Color::white());                       // white bg
        let before: u64 = buf.pixels().iter().map(|&b| b as u64).sum();
        buf.composite_mask(&mask, Color::black(), 0, 0, None); // black text
        let after: u64 = buf.pixels().iter().map(|&b| b as u64).sum();
        assert!(after < before,
            "compositing dark text onto white bg should reduce summed brightness");
    }

    // ── LcdMaskBuilder + LcdBuffer::fill_path ───────────────────────────────

    /// **Refactor regression** — the legacy `rasterize_lcd_mask_multi`
    /// must produce byte-identical output after being rewritten as a
    /// thin wrapper over `LcdMaskBuilder`.  If the bytes drift, every
    /// cached glyph mask in the existing text path subtly changes and
    /// the equivalence chain to all the prior tests breaks.
    #[test]
    fn test_lcd_mask_builder_matches_legacy_text_path() {
        let f = font();
        let w: u32 = 120;
        let h: u32 = 30;
        let xform  = TransAffine::new();

        // Legacy path.
        let legacy = rasterize_lcd_mask_multi(
            &f, &[("Equiv", 4.0, 18.0)], 22.0, w, h, &xform,
        );

        // Builder path — same setup spelt out by hand.
        let mut builder = LcdMaskBuilder::new(w, h);
        builder.with_paths(&xform, |add| {
            let (mut paths, _) = crate::text::shape_text(&f, "Equiv", 22.0, 4.0, 18.0);
            for p in paths.iter_mut() { add(p); }
        });
        let built = builder.finalize();

        assert_eq!(legacy.width,  built.width);
        assert_eq!(legacy.height, built.height);
        assert_eq!(legacy.data, built.data,
            "LcdMaskBuilder must reproduce rasterize_lcd_mask_multi byte-for-byte");
    }

    /// Non-text smoke test for the path entry point — fill a small
    /// rectangular AGG path through the LCD pipeline and verify pixels
    /// inside the rect are dark, outside are untouched.  Exercises the
    /// builder + composite_mask seam without any text shaping involved.
    #[test]
    fn test_lcd_buffer_fill_path_solid_rect() {
        use agg_rust::basics::PATH_FLAGS_NONE;
        let mut buf = LcdBuffer::new(20, 10);
        buf.clear(Color::white());

        // Rectangle from (5, 3) to (15, 7) in Y-up pixel space.
        let mut path = PathStorage::new();
        path.move_to( 5.0, 3.0);
        path.line_to(15.0, 3.0);
        path.line_to(15.0, 7.0);
        path.line_to( 5.0, 7.0);
        path.close_polygon(PATH_FLAGS_NONE);

        buf.fill_path(&mut path, Color::black(), &TransAffine::new(), None);

        let pixel = |x: usize, y: usize| -> (u8, u8, u8) {
            let i = (y * 20 + x) * 3;
            (buf.pixels()[i], buf.pixels()[i + 1], buf.pixels()[i + 2])
        };

        // Centre of rect — fully covered, must be black on every channel.
        assert_eq!(pixel(10, 5), (0, 0, 0),
            "interior pixel of solid rect should be fully covered black");
        // Outside rect — untouched, must stay white.
        assert_eq!(pixel(1, 1), (255, 255, 255),
            "pixel outside rect should be untouched");
        assert_eq!(pixel(18, 8), (255, 255, 255),
            "pixel outside rect should be untouched");
    }

    /// **End-to-end equivalence** — proves the new path-driven LcdBuffer
    /// pipeline matches the existing text-driven one for the same glyph
    /// outlines, when both are composited onto the same starting bg.
    /// This is the contract the LcdGfxCtx (Step 2) relies on.
    #[test]
    fn test_lcd_buffer_fill_path_matches_text_pipeline_for_glyphs() {
        let f = font();
        let w: u32 = 80;
        let h: u32 = 24;
        let size = 18.0;
        let baseline = (4.0_f64, 14.0_f64);

        // Way A — legacy: rasterize text mask, composite_mask onto white buffer.
        let legacy_mask = rasterize_lcd_mask_multi(
            &f, &[("ag", baseline.0, baseline.1)], size, w, h, &TransAffine::new(),
        );
        let mut buf_a = LcdBuffer::new(w, h);
        buf_a.clear(Color::white());
        buf_a.composite_mask(&legacy_mask, Color::black(), 0, 0, None);

        // Way B — builder + fill_path: shape glyphs to paths, fill each onto a
        // freshly cleared buffer.  The end result must be pixel-identical.
        let (mut paths, _) = crate::text::shape_text(&f, "ag", size, baseline.0, baseline.1);
        let mut buf_b = LcdBuffer::new(w, h);
        buf_b.clear(Color::white());
        // Each glyph is its own path; compose them in one mask via the builder
        // so adjacent glyphs share the same gray buffer (matches the legacy
        // batching — separate fill_path calls would also work but each would
        // re-run the filter independently and two adjacent glyphs near a
        // pixel boundary could disagree on the filter input by one subpixel).
        let mut builder = LcdMaskBuilder::new(w, h);
        builder.with_paths(&TransAffine::new(), |add| {
            for p in paths.iter_mut() { add(p); }
        });
        let mask_b = builder.finalize();
        buf_b.composite_mask(&mask_b, Color::black(), 0, 0, None);

        assert_eq!(buf_a.pixels(), buf_b.pixels(),
            "fill_path-via-builder must match legacy text mask pipeline byte-for-byte");
    }

    /// **Equivalence test** — the load-bearing one for this step.
    ///
    /// Painting `text` two ways must produce identical RGB:
    ///
    ///   A. Existing `composite_lcd_mask` writing into a white RGBA frame.
    ///   B. New `LcdBuffer::clear(white) + composite_mask(black)` route.
    ///
    /// If these diverge, the new buffer-side compositor doesn't match the
    /// existing one and any LcdGfxCtx built on top of it will subtly
    /// disagree with the legacy text path.  This is the contract that
    /// future widget-level migrations rely on.
    #[test]
    fn test_lcd_buffer_composite_matches_composite_lcd_mask() {
        let w: u32 = 100;
        let h: u32 = 28;
        let mask = rasterize_lcd_mask(
            &font(), "Equiv", 22.0, 4.0, 18.0, w, h, &TransAffine::new(),
        );

        // Way A — straight RGBA composite.
        let mut rgba = vec![255u8; (w * h * 4) as usize];
        composite_lcd_mask(&mut rgba, w, h, &mask, Color::black(), 0, 0);

        // Way B — paint into LcdBuffer, then read RGB out directly.
        let mut buf = LcdBuffer::new(w, h);
        buf.clear(Color::white());
        buf.composite_mask(&mask, Color::black(), 0, 0, None);

        for y in 0..h as usize {
            for x in 0..w as usize {
                let ai = (y * w as usize + x) * 4;
                let bi = (y * w as usize + x) * 3;
                let a_rgb = (rgba[ai], rgba[ai + 1], rgba[ai + 2]);
                let b_rgb = (buf.pixels()[bi], buf.pixels()[bi + 1], buf.pixels()[bi + 2]);
                assert_eq!(a_rgb, b_rgb,
                    "RGB mismatch at ({x},{y}): RGBA-path={a_rgb:?} LcdBuffer-path={b_rgb:?}");
            }
        }
    }
}
