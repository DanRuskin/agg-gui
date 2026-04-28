//! SVG text rendering regression tests.
//!
//! `usvg` flattens text to drawable primitives during parsing.  These tests
//! keep the library renderer honest about dispatching those flattened nodes
//! through the same path/image renderer used by the rest of SVG.

use super::*;
use std::sync::Arc;

fn text_options() -> SvgParseOptions {
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_font_data(include_bytes!("../../../demo/assets/CascadiaCode.ttf").to_vec());
    fontdb.set_serif_family("Cascadia Code");
    fontdb.set_sans_serif_family("Cascadia Code");
    fontdb.set_monospace_family("Cascadia Code");

    SvgParseOptions::new()
        .with_font_family("Cascadia Code")
        .with_fontdb(Arc::new(fontdb))
}

#[test]
fn renders_basic_text_from_flattened_usvg_paths() {
    let svg = br##"
        <svg xmlns="http://www.w3.org/2000/svg" width="40" height="18">
            <rect width="40" height="18" fill="#000000"/>
            <text x="2" y="14" font-family="Cascadia Code, monospace"
                  font-size="14" fill="#00ff00">Hi</text>
        </svg>
    "##;

    let fb = render_svg_to_framebuffer_with_options(svg, &text_options())
        .expect("SVG text should render");

    assert!(
        fb.pixels()
            .chunks_exact(4)
            .any(|px| px[1] > 80 && px[0] < 40 && px[2] < 40 && px[3] > 0),
        "text should paint visible green glyph pixels"
    );
}

#[test]
fn renders_shields_style_badge_text_runs() {
    let svg = br##"
        <svg xmlns="http://www.w3.org/2000/svg" width="102" height="20" role="img">
            <linearGradient id="s" x2="0" y2="100%">
                <stop offset="0" stop-color="#bbb" stop-opacity=".1"/>
                <stop offset="1" stop-opacity=".1"/>
            </linearGradient>
            <clipPath id="r">
                <rect width="102" height="20" rx="3" fill="#fff"/>
            </clipPath>
            <g clip-path="url(#r)">
                <rect width="57" height="20" fill="#555"/>
                <rect x="57" width="45" height="20" fill="#fe7d37"/>
                <rect width="102" height="20" fill="url(#s)"/>
            </g>
            <g fill="#fff" text-anchor="middle"
               font-family="Verdana,Geneva,DejaVu Sans,sans-serif"
               text-rendering="geometricPrecision" font-size="110">
                <text x="295" y="140" transform="scale(.1)" textLength="470">crates.io</text>
                <text x="785" y="140" transform="scale(.1)" textLength="350">v0.1.0</text>
            </g>
        </svg>
    "##;

    let fb = render_svg_to_framebuffer_with_options(svg, &text_options())
        .expect("badge SVG should render");

    assert!(
        has_bright_text_pixel(&fb, 8..57),
        "left badge label should render visible white text"
    );
    assert!(
        has_bright_text_pixel(&fb, 60..100),
        "right badge value should render visible white text"
    );
}

fn has_bright_text_pixel(fb: &Framebuffer, xs: std::ops::Range<u32>) -> bool {
    for y in 4..16 {
        for x in xs.clone() {
            let i = ((y * fb.width() + x) * 4) as usize;
            let px = &fb.pixels()[i..i + 4];
            if px[0] > 180 && px[1] > 180 && px[2] > 180 && px[3] > 0 {
                return true;
            }
        }
    }
    false
}
