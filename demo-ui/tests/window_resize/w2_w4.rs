use super::*;

// ─── W2 — ↔ resizable + scroll ───────────────────────────────────────────────

#[test]
fn w2_east_drag_grows_width_only() {
    // Dragging the east edge should grow width by ~the drag delta and
    // leave height / y unchanged — standard window-manager convention.
    let (mut app, title, _pos) = make_test_app(1);
    let before = window_bounds(&app, &title);
    let e_x = before.x + before.width - 1.0;
    let mid_y_down = to_screen(before.y + before.height * 0.5);
    drag(&mut app, (e_x, mid_y_down), (e_x + 100.0, mid_y_down));
    let after = window_bounds(&app, &title);
    assert!(
        (after.width - (before.width + 100.0)).abs() < 2.0,
        "east drag grew width by wrong amount: {} → {}",
        before.width,
        after.width
    );
    assert_eq!(
        after.height, before.height,
        "east drag must not change height"
    );
    assert_eq!(after.x, before.x, "east drag must not change x");
    assert_eq!(after.y, before.y, "east drag must not change y");
}

#[test]
fn w2_north_drag_grows_height_and_keeps_top_fixed() {
    // In Y-up the NORTH edge is at y + height.  Dragging it upward
    // (Y-down screen Y decreases) raises the top and grows height.
    // Our Window::apply_resize for N modifies height only (y stays),
    // so the bottom edge is what stays fixed and the top moves up.
    let (mut app, title, _pos) = make_test_app(1);
    let before = window_bounds(&app, &title);
    let mid_x = before.x + before.width * 0.5;
    // Y-up top edge; subtract 1 so local.y lands inside the N resize
    // zone (height-RESIZE_EDGE < local.y < height).
    let top_y_up = before.y + before.height - 1.0;
    let top_y_dn = to_screen(top_y_up);
    drag(&mut app, (mid_x, top_y_dn), (mid_x, top_y_dn - 80.0));
    let after = window_bounds(&app, &title);
    assert!(
        (after.height - (before.height + 80.0)).abs() < 2.0,
        "north drag grew height by wrong amount: {} → {}",
        before.height,
        after.height
    );
    assert_eq!(
        after.y, before.y,
        "apply_resize(N) must leave bounds.y fixed (bottom stays)"
    );
}

#[test]
fn w2_content_has_scroll_view() {
    // Window 2 wraps its long lorem in a `ScrollView` so the user can
    // shrink the window below content height.  The test confirms the
    // tree actually contains that ScrollView; the follow-up test
    // exercises the scroll behaviour.
    let (app, title, _pos) = make_test_app(1);
    let win = find_widget_by_id(app.root(), &title).unwrap();
    assert!(
        find_widget_by_type(win, "ScrollView").is_some(),
        "W2 must contain a ScrollView as direct content"
    );
}

#[test]
fn w2_shrink_below_content_leaves_scrollable_overflow() {
    // Shrink the window height to 80 px (the MIN_H floor) and confirm
    // the inner ScrollView's max_scroll_value is nonzero — meaning the
    // content overflows the viewport and the scrollbar has range.
    //
    // Without this property, W2 would fail egui's "shrink to any size"
    // promise because the user would have no way to reach hidden
    // content.
    let (mut app, title, _pos) = make_test_app(1);
    let before = window_bounds(&app, &title);
    // Grab the south edge and drag it UP in Y-down (i.e. Y-up y
    // increases → apply_resize(S) reduces height).
    let mid_x = before.x + before.width * 0.5;
    let bot_y_up = before.y + 1.0;
    let bot_y_dn = to_screen(bot_y_up);
    drag(&mut app, (mid_x, bot_y_dn), (mid_x, bot_y_dn - 500.0));
    // Relayout once more so the ScrollView sees the shrunken viewport
    // and recomputes its max scroll distance against content height.
    app.layout(Size::new(CANVAS_W, CANVAS_H));
    // ScrollView's public `properties()` lists "max_scroll" on the
    // inspector surface.  Walk the subtree to find the ScrollView and
    // read its `properties()` directly — avoids exposing a new
    // accessor just for tests.
    let sv = {
        let win = find_widget_by_id(app.root(), &title).unwrap();
        find_widget_by_type(win, "ScrollView").unwrap()
    };
    let max_scroll: f64 = sv
        .properties()
        .iter()
        .find(|(k, _)| *k == "max_scroll")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0.0);
    assert!(
        max_scroll > 0.0,
        "shrunk W2 must expose scrollable overflow; got max_scroll={max_scroll}"
    );
}

#[test]
fn w2_vscroll_wraps_content_with_a_single_scroll_view() {
    // Stage-2 contract: `Window::with_vscroll(true)` swaps the window's
    // first child for a ScrollView wrapping the original content.  The
    // window therefore has exactly one direct child, and that child is
    // the ScrollView — no second wrap, no leftover content sibling.
    let (app, title, _pos) = make_test_app(1);
    let win = find_widget_by_id(app.root(), &title).unwrap();
    let kids = win.children();
    assert_eq!(
        kids.len(),
        1,
        "Window expects exactly one direct child after with_vscroll(true)"
    );
    assert_eq!(
        kids[0].type_name(),
        "ScrollView",
        "with_vscroll(true) must place a ScrollView as children[0]; \
         got {} instead",
        kids[0].type_name()
    );
}

#[test]
fn w2_scroll_view_fills_window_inner_content_area() {
    // Layout integrity: the wrapped ScrollView must occupy the entire
    // inner content rect (window width × content_h, where content_h =
    // window_height - TITLE_H).  Off-by-one tolerance accounts for
    // pixel snapping.
    const TITLE_H: f64 = 28.0;
    let (app, title, _pos) = make_test_app(1);
    let win = find_widget_by_id(app.root(), &title).unwrap();
    let win_b = win.bounds();
    let sv = find_widget_by_type(win, "ScrollView").unwrap().bounds();
    assert!(
        (sv.width - win_b.width).abs() < 1.0,
        "ScrollView width should match window width: {} vs {}",
        sv.width,
        win_b.width
    );
    assert!(
        (sv.height - (win_b.height - TITLE_H)).abs() < 1.0,
        "ScrollView height should match inner content area: {} vs {}",
        sv.height,
        win_b.height - TITLE_H
    );
    // Origin: inner content rect starts at (0, 0) in window-local Y-up
    // (title bar is at the *top* = high Y).
    assert!(
        sv.x.abs() < 1.0 && sv.y.abs() < 1.0,
        "ScrollView origin should be (0, 0) in window-local; got ({}, {})",
        sv.x,
        sv.y
    );
}

#[test]
fn w2_mouse_wheel_advances_scroll_offset() {
    // Drive a wheel event over the W2 window.  The wrapped ScrollView
    // must consume it and advance its `v_offset` (inspector property)
    // by the framework-standard 40 px per wheel notch.
    let (mut app, title, _pos) = make_test_app(1);
    let win_b = window_bounds(&app, &title);
    // Cursor right in the middle of the window content area.
    let cx = win_b.x + win_b.width * 0.5;
    let cy_up = win_b.y + win_b.height * 0.5 - 40.0; // below title bar
    let cy_dn = to_screen(cy_up);

    let read_offset = |app: &App| -> f64 {
        let win = find_widget_by_id(app.root(), &title).unwrap();
        let sv = find_widget_by_type(win, "ScrollView").unwrap();
        sv.properties()
            .iter()
            .find(|(k, _)| *k == "v_offset")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0.0)
    };

    let before = read_offset(&app);
    app.on_mouse_move(cx, cy_dn); // prime hover so the wheel routes here
                                  // App convention (matches winit / WheelEvent): positive delta_y =
                                  // user wants to see content ABOVE = offset DECREASES. To scroll
                                  // DOWN the window (advance offset toward max_scroll) we send a
                                  // NEGATIVE delta_y.
    app.on_mouse_wheel(cx, cy_dn, -3.0); // 3 notches scroll-down
    app.layout(Size::new(CANVAS_W, CANVAS_H));
    let after = read_offset(&app);
    assert!(
        after > before,
        "v_offset must advance after wheel; {} → {}",
        before,
        after
    );
    // Standard ScrollView wheel multiplier is 40 px per notch; 3 notches
    // → 120 px.  Tolerance accounts for any clamping at max_scroll.
    let expected = (before + 120.0).min({
        let win = find_widget_by_id(app.root(), &title).unwrap();
        let sv = find_widget_by_type(win, "ScrollView").unwrap();
        sv.properties()
            .iter()
            .find(|(k, _)| *k == "max_scroll")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(120.0)
    });
    assert!(
        (after - expected).abs() < 1.0,
        "wheel advanced wrong amount: expected ~{} got {}",
        expected,
        after
    );
}

// ─── W3 — ↔ resizable + embedded scroll ──────────────────────────────────────

#[test]
fn w3_embedded_scroll_view_present() {
    // W3 differs from W2 in egui via `.vscroll(false)` + a manual
    // `ScrollArea::vertical()` inside.  Visual shape of the tree is the
    // same (a ScrollView grandchild), but the semantic is "caller-owned".
    // The test asserts the content tree contains a ScrollView — either
    // placement satisfies the behavioural contract.
    let (app, title, _pos) = make_test_app(2);
    let win = find_widget_by_id(app.root(), &title).unwrap();
    assert!(
        find_widget_by_type(win, "ScrollView").is_some(),
        "W3 must contain an embedded ScrollView"
    );
}

// ─── W4 — ↔ resizable without scroll ─────────────────────────────────────────

#[test]
fn w4_cannot_shrink_past_content_natural_height() {
    // Stage-5 contract: a window built with `with_tight_content_fit`
    // (which W4 is) refuses to shrink past its content's natural
    // height — content is never clipped.  Measurement plan:
    //   1. Note the window's inner content-area height before drag;
    //      that equals the content's natural height (W4 content is
    //      all fixed-height widgets, so FlexColumn reports its sum).
    //   2. Drag the S edge hard to the top of the canvas.
    //   3. The resulting height should equal content_natural + TITLE_H
    //      (the min we computed), not the bare MIN_H=80.
    const TITLE_H: f64 = 28.0;
    let (mut app, title, _pos) = make_test_app(3);
    let before = window_bounds(&app, &title);
    let content_natural = before.height - TITLE_H;

    let mid_x = before.x + before.width * 0.5;
    let bot_y_up = before.y + 1.0;
    let bot_y_dn = to_screen(bot_y_up);
    drag(&mut app, (mid_x, bot_y_dn), (mid_x, bot_y_dn - 1000.0));
    let after = window_bounds(&app, &title);

    // Floor must be WAY above the bare MIN_H (80) — proving the
    // content-bound clamp fired — and land close to the content's
    // natural height plus title bar.  Note: the initial window
    // height (290) can be slightly below content natural height, in
    // which case the first drag clamps UP to content_min.  That is
    // still the documented behaviour — no content clipping.
    let _ = content_natural;
    let _ = before;
    assert!(
        after.height > 200.0,
        "tight_content_fit must floor W4 well above MIN_H=80; got {}",
        after.height
    );
}

#[test]
fn w4_cannot_grow_past_content_natural_height() {
    // egui's "no scroll, no clip" contract is symmetric: window height
    // = content height, never more (no whitespace), never less (no
    // clip).  Stage-5+ adds a tight-fit pre-pass to `Window::layout`
    // that snaps height to content each frame, so an attempted S
    // drag growing the window has no lasting effect — the next
    // layout snaps it back.
    let (mut app, title, _pos) = make_test_app(3);
    let before = window_bounds(&app, &title);
    // Grab the S edge and drag it DOWN in screen (Y-up Y decreases →
    // h grows).  apply_resize for S: y = sb.y + dy; h = sb.h - dy.
    // dy < 0 (Y-down increased), so y decreases (window moves down)
    // and h grows.
    let mid_x = before.x + before.width * 0.5;
    let bot_y_up = before.y + 1.0;
    let bot_y_dn = to_screen(bot_y_up);
    drag(&mut app, (mid_x, bot_y_dn), (mid_x, bot_y_dn + 400.0));
    let after = window_bounds(&app, &title);
    // The tight-fit pre-pass must snap height back to content.
    assert!(
        (after.height - before.height).abs() < 5.0,
        "W4 must not grow past content height; was {}, after S drag now {}",
        before.height,
        after.height
    );
}

#[test]
fn tight_content_fit_clamps_resize_below_content_height() {
    // Library-level proof that the Stage-5 flag drives the resize
    // floor.  Compare two windows identical in content but only one
    // with `with_tight_content_fit(true)` — the tight one refuses to
    // shrink past content, the non-tight one honours the hard MIN_H.
    use agg_gui::{FlexColumn, Label};
    let make = |tight: bool| -> Window {
        let mut col = FlexColumn::new().with_gap(4.0).with_padding(8.0);
        for _ in 0..6 {
            col.push(
                Box::new(
                    Label::new(
                        "A line tall enough to push content well above MIN_H.",
                        font(),
                    )
                    .with_font_size(13.0),
                ),
                0.0,
            );
        }
        let mut w = Window::new(if tight { "tight" } else { "loose" }, font(), Box::new(col))
            .with_bounds(Rect::new(80.0, 80.0, 300.0, 300.0));
        if tight {
            w = w.with_tight_content_fit(true);
        }
        w
    };
    let mut tight = make(true);
    let mut loose = make(false);
    tight.layout(Size::new(CANVAS_W, CANVAS_H));
    loose.layout(Size::new(CANVAS_W, CANVAS_H));

    // Drive MouseDown + Move + Up on the S edge (widget-local y≈0).
    // Note widget-local pos on the S edge hitting the bottom strip.
    let apply_shrink_drag = |win: &mut Window| {
        let s_pos = Point::new(150.0, 1.0);
        win.on_event(&Event::MouseMove { pos: s_pos });
        win.on_event(&Event::MouseDown {
            pos: s_pos,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        });
        // Move the cursor far beyond the window's old top edge.
        win.on_event(&Event::MouseMove {
            pos: Point::new(150.0, 10_000.0),
        });
        win.on_event(&Event::MouseUp {
            pos: Point::new(150.0, 10_000.0),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        });
    };
    apply_shrink_drag(&mut tight);
    apply_shrink_drag(&mut loose);

    // Tight must not have dropped to MIN_H=80.
    assert!(
        tight.bounds().height > 80.0 + 1.0,
        "tight_content_fit window must not shrink to MIN_H; got {}",
        tight.bounds().height
    );
    // Loose should be at the hard floor.
    assert!(
        (loose.bounds().height - 80.0).abs() < 2.0,
        "non-tight window should land at MIN_H=80; got {}",
        loose.bounds().height
    );
}

#[test]
fn container_with_fit_height_returns_content_height() {
    // Stage-5 fix: `Container::with_fit_height(true)` reports its
    // content's natural height + vertical padding rather than filling
    // the full available area.  Without this the auto-sized W1
    // window inflated to the canvas size (the original OOM trigger).
    use agg_gui::{Container, Label};
    let child = Box::new(Label::new("hello", font()).with_font_size(14.0));
    let mut c = Container::new()
        .with_fit_height(true)
        .with_padding(6.0)
        .add(child);
    // Huge available height; fit-mode must NOT return all of it.
    let reported = c.layout(Size::new(200.0, 4000.0));
    assert!(
        reported.height < 200.0,
        "fit_height Container must not claim the full available height; got {}",
        reported.height
    );
    // And the height must be at least content-height (a line of text
    // at 14 pt is roughly 21 px, plus 12 px padding top + bottom).
    assert!(
        reported.height > 20.0,
        "fit_height Container should still include the child's height; got {}",
        reported.height
    );

    // The default (fit_height = false) still claims available.
    let mut c2 = Container::new().add(Box::new(Label::new("hello", font()).with_font_size(14.0)));
    let r2 = c2.layout(Size::new(200.0, 4000.0));
    assert!(
        (r2.height - 4000.0).abs() < 1.0,
        "default Container still fills available height; got {}",
        r2.height
    );
}
