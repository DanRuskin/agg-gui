//! SVG clip-path regression tests.
//!
//! Badge SVGs commonly wrap their colored segments in a `clipPath`.  The
//! renderer currently maps clip paths to their rectangular bounds, which covers
//! simple clipping and gives us a tested stepping stone toward arbitrary masks.

use super::*;

#[test]
fn group_clip_path_bounds_constrain_children() {
    let svg = br##"
        <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <clipPath id="left-half">
                <rect width="5" height="10"/>
            </clipPath>
            <g clip-path="url(#left-half)">
                <rect width="10" height="10" fill="#ff0000"/>
            </g>
        </svg>
    "##;

    let fb = render_svg_to_framebuffer(svg).expect("SVG clip path should render");

    let clipped_pixel = ((5 * fb.width() + 7) * 4) as usize;
    assert_eq!(
        &fb.pixels()[clipped_pixel..clipped_pixel + 4],
        &[0, 0, 0, 0],
        "clip path should prevent the right half from painting"
    );

    let painted_pixel = ((5 * fb.width() + 2) * 4) as usize;
    assert_eq!(
        &fb.pixels()[painted_pixel..painted_pixel + 4],
        &[255, 0, 0, 255],
        "content inside the clip path should still paint"
    );
}
