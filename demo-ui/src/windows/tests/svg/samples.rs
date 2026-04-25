pub(super) struct SvgSample {
    pub(super) name: &'static str,
    pub(super) svg: &'static [u8],
    pub(super) reference_png: &'static [u8],
}

pub(super) const SVG_SAMPLES: &[SvgSample] = &[
    SvgSample {
        name: "shapes/rect/simple-case.svg",
        svg: include_bytes!(
            "../../../../../tests/resvg-test-suite/tests/shapes/rect/simple-case.svg"
        ),
        reference_png: include_bytes!(
            "../../../../../tests/resvg-test-suite/tests/shapes/rect/simple-case.png"
        ),
    },
    SvgSample {
        name: "shapes/path/M-L-L-Z.svg",
        svg: include_bytes!("../../../../../tests/resvg-test-suite/tests/shapes/path/M-L-L-Z.svg"),
        reference_png: include_bytes!(
            "../../../../../tests/resvg-test-suite/tests/shapes/path/M-L-L-Z.png"
        ),
    },
    SvgSample {
        name: "painting/stroke/line-as-curve-1.svg",
        svg: include_bytes!(
            "../../../../../tests/resvg-test-suite/tests/painting/stroke/line-as-curve-1.svg"
        ),
        reference_png: include_bytes!(
            "../../../../../tests/resvg-test-suite/tests/painting/stroke/line-as-curve-1.png"
        ),
    },
    SvgSample {
        name: "structure/image/embedded-png.svg",
        svg: include_bytes!(
            "../../../../../tests/resvg-test-suite/tests/structure/image/embedded-png.svg"
        ),
        reference_png: include_bytes!(
            "../../../../../tests/resvg-test-suite/tests/structure/image/embedded-png.png"
        ),
    },
];
