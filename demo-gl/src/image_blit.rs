use std::rc::Rc;

use glow::HasContext;

use crate::gl_support::texture_key;
use crate::GlGfxCtx;

impl GlGfxCtx {
    pub(crate) fn draw_image_rgba_slice_impl(
        &mut self,
        data: &[u8],
        img_w: u32,
        img_h: u32,
        dst_x: f64,
        dst_y: f64,
        dst_w: f64,
        dst_h: f64,
    ) {
        if img_w == 0 || img_h == 0 || dst_w <= 0.0 || dst_h <= 0.0 {
            return;
        }
        if data.len() < (img_w as usize) * (img_h as usize) * 4 {
            return;
        }

        // Honour whatever CTM the caller has set — sub-pixel positions are
        // legitimate (smooth scrolling, animation).  Callers that need
        // pixel-perfect 1:1 blits (e.g. `Label` backbuffers, the pixel-
        // alignment test) must explicitly call `ctx.snap_to_pixel()` first.
        let bl = self.transform_pt(dst_x, dst_y);
        let br = self.transform_pt(dst_x + dst_w, dst_y);
        let tr = self.transform_pt(dst_x + dst_w, dst_y + dst_h);
        let tl = self.transform_pt(dst_x, dst_y + dst_h);
        let verts: [f32; 24] = [
            bl[0], bl[1], 0.0, 1.0, br[0], br[1], 1.0, 1.0, tr[0], tr[1], 1.0, 0.0, bl[0], bl[1],
            0.0, 1.0, tr[0], tr[1], 1.0, 0.0, tl[0], tl[1], 0.0, 0.0,
        ];

        // Cache key blends pointer, length, dimensions, and the first+last
        // few bytes.  Pointer changes when `Label` rebuilds its pixel cache
        // (drops old `Vec<u8>`, allocates new), so the key naturally
        // invalidates.  Head/tail-byte hash guards against the (rare) case
        // where a new allocation lands at the freed pointer address.
        let key = texture_key(data, img_w, img_h);
        let existing = self.texture_cache.get(&key).map(|&(t, _, _)| t);

        unsafe {
            let gl = Rc::clone(&self.gl);
            let tex = match existing {
                Some(t) => {
                    // LRU touch — move key to back.
                    if let Some(pos) = self.texture_cache_order.iter().position(|&k| k == key) {
                        self.texture_cache_order.remove(pos);
                    }
                    self.texture_cache_order.push_back(key);
                    t
                }
                None => {
                    let tex = gl.create_texture().expect("create texture");
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::LINEAR as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAG_FILTER,
                        glow::LINEAR as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_WRAP_S,
                        glow::CLAMP_TO_EDGE as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_WRAP_T,
                        glow::CLAMP_TO_EDGE as i32,
                    );
                    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA as i32,
                        img_w as i32,
                        img_h as i32,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        Some(data),
                    );
                    self.texture_cache.insert(key, (tex, img_w, img_h));
                    self.texture_cache_order.push_back(key);
                    // LRU evict to cap.
                    const TEX_CACHE_MAX: usize = 512;
                    while self.texture_cache.len() > TEX_CACHE_MAX {
                        if let Some(old) = self.texture_cache_order.pop_front() {
                            if let Some((old_tex, _, _)) = self.texture_cache.remove(&old) {
                                gl.delete_texture(old_tex);
                            }
                        } else {
                            break;
                        }
                    }
                    tex
                }
            };

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.use_program(Some(self.tex_prog));
            gl.uniform_2_f32(self.tex_res_loc.as_ref(), self.viewport.0, self.viewport.1);
            gl.uniform_1_i32(self.tex_sampler_loc.as_ref(), 0);
            gl.bind_vertex_array(Some(self.tex_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.tex_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&verts),
                glow::DYNAMIC_DRAW,
            );
            gl.enable(glow::BLEND);
            // Preserve framebuffer alpha just like begin_frame(). On WebGL the
            // browser composites the canvas alpha over the page, so image
            // blits must not switch later translucent UI draws into an
            // alpha-punching blend mode.
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ZERO,
                glow::ONE,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
            gl.bind_vertex_array(None);
        }
    }
}
