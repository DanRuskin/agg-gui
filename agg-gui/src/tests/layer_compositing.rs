//! Software-renderer layer composite tests.
//!
//! Verify that `GfxCtx::push_layer` / `pop_layer` produce the expected
//! Porter-Duff `SrcOver` blend onto the parent framebuffer.

use super::*;

#[test]
fn test_push_pop_layer_solid_composites_correctly() {
    let mut fb = Framebuffer::new(20, 20);
    let mut ctx = GfxCtx::new(&mut fb);
    ctx.clear(Color::white());

    ctx.push_layer(20.0, 20.0);
    ctx.set_fill_color(Color::rgba(1.0, 0.0, 0.0, 1.0));
    ctx.begin_path();
    ctx.rect(0.0, 0.0, 20.0, 20.0);
    ctx.fill();
    ctx.pop_layer();

    drop(ctx);

    let center = sample(&fb, 10, 10);
    assert!(
        is_red(center),
        "After layer composite, centre must be red; got {center:?}"
    );
}

#[test]
fn test_push_pop_layer_alpha_blends_into_parent() {
    let mut fb = Framebuffer::new(20, 20);
    let mut ctx = GfxCtx::new(&mut fb);
    ctx.clear(Color::white());

    ctx.push_layer(20.0, 20.0);
    ctx.set_fill_color(Color::rgba(1.0, 0.0, 0.0, 0.5));
    ctx.begin_path();
    ctx.rect(0.0, 0.0, 20.0, 20.0);
    ctx.fill();
    ctx.pop_layer();

    drop(ctx);

    let [r, g, b, _] = sample(&fb, 10, 10);
    assert!(r > 200, "Red channel must be high; got {r}");
    assert!(
        g > 80 && g < 200,
        "Green channel must be mid-tone (pink); got {g}"
    );
    assert!(
        b > 80 && b < 200,
        "Blue channel must be mid-tone (pink); got {b}"
    );
}
