//! Native WGL demo for agg-gui — Phase 2.
//!
//! Renders the Phase 1 demo scene via AGG → Framebuffer → GL texture →
//! full-screen quad. The framebuffer uses bottom-up (Y-up) row order which
//! matches OpenGL's texture layout, so no Y-flip is needed at upload time.

use std::num::NonZeroU32;

use agg_gui::{CompOp, Framebuffer, GfxCtx};

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{GlSurface, SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use glow::HasContext;
use raw_window_handle::HasWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowAttributes;

// ---------------------------------------------------------------------------
// GL shaders — full-screen quad using gl_VertexID (no VBO required)
// ---------------------------------------------------------------------------

const VERT_SHADER: &str = r#"#version 330 core
out vec2 v_tex_coord;
void main() {
    // Triangle strip covering [-1, 1]^2 in clip space.
    // Vertex IDs 0–3 produce the four corners:
    //   0: (-1,-1)  1: ( 1,-1)  2: (-1, 1)  3: ( 1, 1)
    float x = float((gl_VertexID & 1) * 2) - 1.0;
    float y = float((gl_VertexID >> 1) * 2) - 1.0;
    gl_Position = vec4(x, y, 0.0, 1.0);
    // UV (0,0) = bottom-left, matching our Y-up framebuffer layout.
    // No flip needed — GL textures are also Y-up (row 0 = bottom).
    v_tex_coord = vec2((x + 1.0) * 0.5, (y + 1.0) * 0.5);
}
"#;

const FRAG_SHADER: &str = r#"#version 330 core
in vec2 v_tex_coord;
out vec4 frag_color;
uniform sampler2D u_texture;
void main() {
    frag_color = texture(u_texture, v_tex_coord);
}
"#;

// ---------------------------------------------------------------------------
// GlPresenter — uploads the framebuffer to a GL texture and draws the quad
// ---------------------------------------------------------------------------

struct GlPresenter {
    gl: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    texture: glow::Texture,
    texture_width: u32,
    texture_height: u32,
}

impl GlPresenter {
    unsafe fn new(gl: glow::Context) -> Self {
        // Compile shaders
        let program = gl.create_program().expect("create_program");

        let vert = gl.create_shader(glow::VERTEX_SHADER).unwrap();
        gl.shader_source(vert, VERT_SHADER);
        gl.compile_shader(vert);
        assert!(
            gl.get_shader_compile_status(vert),
            "vert: {}",
            gl.get_shader_info_log(vert)
        );

        let frag = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
        gl.shader_source(frag, FRAG_SHADER);
        gl.compile_shader(frag);
        assert!(
            gl.get_shader_compile_status(frag),
            "frag: {}",
            gl.get_shader_info_log(frag)
        );

        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        assert!(
            gl.get_program_link_status(program),
            "link: {}",
            gl.get_program_info_log(program)
        );
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        // Empty VAO (vertices come from gl_VertexID)
        let vao = gl.create_vertex_array().unwrap();

        // Framebuffer texture
        let texture = gl.create_texture().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
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

        Self {
            gl,
            program,
            vao,
            texture,
            texture_width: 0,
            texture_height: 0,
        }
    }

    /// Upload pixel data from a framebuffer. Call every frame.
    unsafe fn update_texture(&mut self, fb: &Framebuffer) {
        let w = fb.width();
        let h = fb.height();
        self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        if w != self.texture_width || h != self.texture_height {
            // Reallocate texture storage on size change.
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(fb.pixels()),
            );
            self.texture_width = w;
            self.texture_height = h;
        } else {
            // Reuse existing storage — faster than re-allocating.
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(fb.pixels()),
            );
        }
    }

    /// Draw the full-screen quad.
    unsafe fn present(&self) {
        self.gl.clear(glow::COLOR_BUFFER_BIT);
        self.gl.use_program(Some(self.program));
        self.gl.bind_vertex_array(Some(self.vao));
        self.gl.active_texture(glow::TEXTURE0);
        self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let event_loop = EventLoop::new().expect("EventLoop::new");

    let window_attributes = WindowAttributes::default()
        .with_title("agg-gui — Phase 2 Demo")
        .with_inner_size(LogicalSize::new(1280u32, 720u32));

    let template = ConfigTemplateBuilder::new().with_alpha_size(0);
    let display_builder =
        DisplayBuilder::new().with_window_attributes(Some(window_attributes));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs
                .reduce(|a, b| if b.num_samples() > a.num_samples() { b } else { a })
                .expect("no suitable GL config")
        })
        .expect("DisplayBuilder::build");

    let window = window.expect("window");
    let raw_window_handle = window.window_handle().expect("window_handle").as_raw();

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(raw_window_handle));

    let gl_display = gl_config.display();
    let not_current_gl_context = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .expect("create_context")
    };

    let size = window.inner_size();
    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width.max(1)).unwrap(),
        NonZeroU32::new(size.height.max(1)).unwrap(),
    );

    let gl_surface = unsafe {
        gl_display
            .create_window_surface(&gl_config, &surface_attributes)
            .expect("create_window_surface")
    };

    let gl_context = not_current_gl_context
        .make_current(&gl_surface)
        .expect("make_current");

    let gl = unsafe {
        glow::Context::from_loader_function_cstr(|s| gl_display.get_proc_address(s))
    };

    let mut presenter = unsafe { GlPresenter::new(gl) };
    let mut fb = Framebuffer::new(size.width.max(1), size.height.max(1));

    // Draw and upload the first frame immediately.
    render_and_upload(&mut fb, &mut presenter);

    #[allow(deprecated)]
    event_loop
        .run(|event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);
            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    elwt.exit();
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(new_size),
                    ..
                } => {
                    if new_size.width > 0 && new_size.height > 0 {
                        gl_surface.resize(
                            &gl_context,
                            NonZeroU32::new(new_size.width).unwrap(),
                            NonZeroU32::new(new_size.height).unwrap(),
                        );
                        unsafe {
                            presenter.gl.viewport(
                                0,
                                0,
                                new_size.width as i32,
                                new_size.height as i32,
                            );
                        }
                        fb.resize(new_size.width, new_size.height);
                        render_and_upload(&mut fb, &mut presenter);
                    }
                }
                Event::AboutToWait => {
                    render_and_upload(&mut fb, &mut presenter);
                    unsafe { presenter.present() };
                    gl_surface.swap_buffers(&gl_context).expect("swap_buffers");
                }
                _ => {}
            }
        })
        .expect("event_loop.run");
}

fn render_and_upload(fb: &mut Framebuffer, presenter: &mut GlPresenter) {
    let w = fb.width();
    let h = fb.height();
    {
        let mut ctx = GfxCtx::new(fb);
        draw_phase2_demo(&mut ctx, w, h);
    }
    unsafe { presenter.update_texture(fb) };
}

// ---------------------------------------------------------------------------
// Phase 2 demo scene — shared with demo-wasm (kept in sync manually)
// ---------------------------------------------------------------------------

fn draw_phase2_demo(ctx: &mut GfxCtx, width: u32, height: u32) {
    use agg_gui::Color;

    let w = width as f64;
    let h = height as f64;

    ctx.clear(Color::rgb(0.94, 0.94, 0.96));

    let pad = (w.min(h) * 0.03).max(10.0);
    let gap = pad * 0.6;
    let col_w = (w - pad * 2.0 - gap) / 2.0;
    let row_h = (h - pad * 2.0 - gap) / 2.0;

    let panels = [
        (pad,               pad + row_h + gap, col_w, row_h),
        (pad + col_w + gap, pad + row_h + gap, col_w, row_h),
        (pad,               pad,               col_w, row_h),
        (pad + col_w + gap, pad,               col_w, row_h),
    ];

    for &(px, py, pw, ph) in &panels {
        draw_card(ctx, px, py, pw, ph);
    }

    {
        let (px, py, pw, ph) = panels[0];
        draw_panel_title(ctx, px, py, pw, ph, "Rounded Rects");
        draw_rounded_rects_demo(ctx, px, py + ph * 0.15, pw, ph * 0.78);
    }
    {
        let (px, py, pw, ph) = panels[1];
        draw_panel_title(ctx, px, py, pw, ph, "Blend Modes");
        draw_blend_modes_demo(ctx, px, py + ph * 0.15, pw, ph * 0.78);
    }
    {
        let (px, py, pw, ph) = panels[2];
        draw_panel_title(ctx, px, py, pw, ph, "Clip Rect");
        draw_clip_demo(ctx, px, py + ph * 0.15, pw, ph * 0.78);
    }
    {
        let (px, py, pw, ph) = panels[3];
        draw_panel_title(ctx, px, py, pw, ph, "Transform Stack");
        draw_transform_demo(ctx, px, py + ph * 0.15, pw, ph * 0.78);
    }

    let label_size = (w * 0.012).clamp(9.0, 13.0);
    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.35));
    ctx.fill_text_gsv("agg-gui  Phase 2", pad, pad * 0.4, label_size);
}

fn draw_card(ctx: &mut GfxCtx, x: f64, y: f64, w: f64, h: f64) {
    use agg_gui::Color;
    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.08));
    ctx.set_blend_mode(CompOp::Multiply);
    ctx.begin_path();
    ctx.rounded_rect(x + 2.0, y - 2.0, w, h, 10.0);
    ctx.fill();
    ctx.set_blend_mode(CompOp::SrcOver);
    ctx.set_fill_color(Color::rgb(1.0, 1.0, 1.0));
    ctx.begin_path();
    ctx.rounded_rect(x, y, w, h, 10.0);
    ctx.fill();
}

fn draw_panel_title(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64, title: &str) {
    use agg_gui::Color;
    let size = (pw * 0.055).clamp(10.0, 16.0);
    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.55));
    ctx.fill_text_gsv(title, px + pw * 0.05, py + ph * 0.86, size);
}

fn draw_rounded_rects_demo(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64) {
    use agg_gui::Color;
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
        ctx.set_fill_color(col.with_alpha(0.18));
        ctx.begin_path();
        ctx.rounded_rect(inner_x, iy, inner_w, row_h, r);
        ctx.fill();
        ctx.set_stroke_color(col);
        ctx.set_line_width(1.5);
        ctx.begin_path();
        ctx.rounded_rect(inner_x, iy, inner_w, row_h, r);
        ctx.stroke();
        let label = format!("r = {}", r as i32);
        let lsize = (pw * 0.04).clamp(8.0, 12.0);
        ctx.set_fill_color(col);
        ctx.fill_text_gsv(&label, inner_x + inner_w * 0.03, iy + row_h * 0.28, lsize);
    }
}

fn draw_blend_modes_demo(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64) {
    use agg_gui::Color;
    let cy = py + ph * 0.5;
    let col_w = pw / 3.0;
    let lsize = (pw * 0.032).clamp(7.0, 10.0);
    let modes: [(CompOp, &str); 3] = [
        (CompOp::Multiply, "Multiply"),
        (CompOp::Screen,   "Screen"),
        (CompOp::Overlay,  "Overlay"),
    ];
    for (i, &(mode, label)) in modes.iter().enumerate() {
        let ccx = px + col_w * (i as f64 + 0.5);
        let small_r = pw.min(ph) * 0.15;
        ctx.set_blend_mode(CompOp::SrcOver);
        ctx.set_fill_color(Color::rgba(0.22, 0.45, 0.87, 0.9));
        ctx.begin_path();
        ctx.circle(ccx - small_r * 0.35, cy - small_r * 0.2, small_r);
        ctx.fill();
        ctx.set_blend_mode(mode);
        ctx.set_fill_color(Color::rgba(0.91, 0.28, 0.18, 0.9));
        ctx.begin_path();
        ctx.circle(ccx + small_r * 0.35, cy + small_r * 0.2, small_r);
        ctx.fill();
        ctx.set_fill_color(Color::rgba(0.14, 0.76, 0.39, 0.85));
        ctx.begin_path();
        ctx.circle(ccx, cy - small_r * 0.55, small_r);
        ctx.fill();
        ctx.set_blend_mode(CompOp::SrcOver);
        ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.5));
        let lx = ccx - lsize * label.len() as f64 * 0.35;
        ctx.fill_text_gsv(label, lx, py + ph * 0.08, lsize);
    }
}

fn draw_clip_demo(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64) {
    use agg_gui::Color;
    ctx.set_blend_mode(CompOp::SrcOver);
    let margin = pw * 0.08;
    let cx = px + pw * 0.5;
    let cy = py + ph * 0.5;
    let clip_x = px + margin * 1.5;
    let clip_y = py + margin * 1.5;
    let clip_w = pw - margin * 3.0;
    let clip_h = ph - margin * 3.5;

    ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.06));
    ctx.begin_path();
    ctx.rounded_rect(px + margin * 0.3, py + margin * 0.3,
                     pw - margin * 0.6, ph - margin * 0.6, 6.0);
    ctx.fill();

    ctx.save();
    ctx.clip_rect(clip_x, clip_y, clip_w, clip_h);

    let n = 8;
    let ring_r = pw.min(ph) * 0.28;
    let dot_r  = pw.min(ph) * 0.09;
    let colors = [
        Color::rgb(0.27, 0.53, 0.91), Color::rgb(0.91, 0.35, 0.22),
        Color::rgb(0.22, 0.76, 0.42), Color::rgb(0.88, 0.65, 0.10),
        Color::rgb(0.62, 0.28, 0.88), Color::rgb(0.10, 0.72, 0.88),
        Color::rgb(0.95, 0.38, 0.62), Color::rgb(0.38, 0.82, 0.12),
    ];
    for i in 0..n {
        let angle = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
        ctx.set_fill_color(colors[i % colors.len()]);
        ctx.begin_path();
        ctx.circle(cx + angle.cos() * ring_r, cy + angle.sin() * ring_r, dot_r);
        ctx.fill();
    }
    ctx.set_fill_color(Color::rgba(0.27, 0.53, 0.91, 0.25));
    ctx.begin_path();
    ctx.circle(cx, cy, ring_r * 0.55);
    ctx.fill();
    ctx.set_stroke_color(Color::rgba(0.27, 0.53, 0.91, 0.6));
    ctx.set_line_width(2.0);
    ctx.begin_path();
    ctx.circle(cx, cy, ring_r * 0.55);
    ctx.stroke();
    ctx.restore();

    ctx.set_stroke_color(Color::rgba(0.3, 0.3, 0.3, 0.4));
    ctx.set_line_width(1.5);
    ctx.begin_path();
    ctx.rounded_rect(clip_x, clip_y, clip_w, clip_h, 4.0);
    ctx.stroke();
}

fn draw_transform_demo(ctx: &mut GfxCtx, px: f64, py: f64, pw: f64, ph: f64) {
    use agg_gui::Color;
    ctx.set_blend_mode(CompOp::SrcOver);
    let cx = px + pw * 0.5;
    let cy = py + ph * 0.5;
    let unit = pw.min(ph) * 0.12;
    let levels = [
        (unit * 2.8, 0.0_f64,                    Color::rgba(0.27, 0.53, 0.91, 0.25), Color::rgba(0.27, 0.53, 0.91, 0.8)),
        (unit * 2.0, std::f64::consts::PI / 6.0, Color::rgba(0.22, 0.76, 0.42, 0.25), Color::rgba(0.22, 0.76, 0.42, 0.8)),
        (unit * 1.2, std::f64::consts::PI / 4.0, Color::rgba(0.91, 0.42, 0.22, 0.3),  Color::rgba(0.91, 0.42, 0.22, 0.9)),
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
    ctx.set_fill_color(Color::rgb(0.2, 0.2, 0.25));
    ctx.begin_path();
    ctx.circle(cx, cy, unit * 0.18);
    ctx.fill();
    let ax_len = unit * 1.5;
    ctx.set_stroke_color(Color::rgba(0.85, 0.2, 0.2, 0.7));
    ctx.set_line_width(1.5);
    ctx.begin_path();
    ctx.move_to(cx, cy);
    ctx.line_to(cx + ax_len, cy);
    ctx.stroke();
    ctx.set_stroke_color(Color::rgba(0.1, 0.7, 0.2, 0.7));
    ctx.begin_path();
    ctx.move_to(cx, cy);
    ctx.line_to(cx, cy + ax_len);
    ctx.stroke();
}
