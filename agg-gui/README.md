# agg-gui

A Rust GUI framework built on [Anti-Grain Geometry (AGG)](https://github.com/larsbrubaker/agg-rust).
Widgets rasterize directly via AGG paths — no retained scene graph, no DOM, no CSS.
The rendering model is immediate-mode: full redraw every frame, deterministic layout, Y-up coordinates throughout.

> Part of the [rust-apps](https://github.com/larsbrubaker/rust-apps) suite — a collection of Rust graphics and geometry libraries by Lars Brubaker.

[![crates.io](https://img.shields.io/crates/v/agg-gui.svg)](https://crates.io/crates/agg-gui)
[![docs.rs](https://docs.rs/agg-gui/badge.svg)](https://docs.rs/agg-gui)
[![CI](https://github.com/larsbrubaker/agg-gui/actions/workflows/ci.yml/badge.svg?branch=main&event=push)](https://github.com/larsbrubaker/agg-gui/actions/workflows/ci.yml)

## Live Demo

> **[Open interactive WASM demo →](https://larsbrubaker.github.io/agg-gui/)**

[![agg-gui demo: System and Scrolling windows over the animated bar-grid background](readme_hero.png)](https://larsbrubaker.github.io/agg-gui/)

## Install

```sh
cargo add agg-gui
```

Optional features:

| Feature | Enables |
|---------|---------|
| `winit-adapter` | `agg_gui::winit_adapter` — maps winit `MouseButton` / `Modifiers` / `Key` / `CursorIcon` to the crate's input types |
| `clipboard` | `arboard`-backed system clipboard integration |

```toml
[dependencies]
agg-gui = { version = "0.1", features = ["winit-adapter", "clipboard"] }
```

## Widget Library

| Widget | Description |
|--------|-------------|
| `Label` | Static text, theme-aware color, left/center/right alignment |
| `Button` | Themeable background, focus ring, disabled state, click callback |
| `Checkbox` | Animated check mark, shared state cell for two-way binding |
| `Slider` | Linear value control with focus ring and keyboard nudge |
| `DragValue` | Click-drag to increment/decrement numeric values |
| `RadioGroup` | Single-selection group with shared state |
| `ProgressBar` | Filled track with optional label |
| `ToggleSwitch` | Animated on/off toggle |
| `TextField` | Full text editing: cursor, selection, clipboard, undo/redo |
| `Hyperlink` | Underlined link text with click callback |
| `ScrollView` | Vertical scroll with drag-thumb and mouse-wheel support |
| `Window` | Floating panel: draggable title bar, close button, resize handles, collapse |
| `FlexColumn` | Vertical flex layout with gap, padding, fixed + growing children |
| `FlexRow` | Horizontal flex layout |
| `Stack` | Z-ordered overlay layout (for floating windows) |
| `SizedBox` | Fixed-size constraint wrapper |
| `Splitter` | Draggable divider between two panes |
| `TabView` | Tabbed panel switcher with persistable active-tab cell |
| `TreeView` | Hierarchical list with expand/collapse and drag-and-drop |
| `Container` | Border + background decorator |
| `MarkdownView` | Markdown renderer: headings, paragraphs, lists, code blocks, images |
| `Separator` | Horizontal or vertical rule |
| `Spacer` / `Padding` | Layout utility widgets |

## Features

- **Theme system** — dark / light / system themes, runtime-switchable; every widget reads
  colors from `ctx.visuals()` (no hardcoded colors).
- **Flex layout** — fixed + growing children, per-child margins, cross-axis anchoring,
  min/max constraints, inner padding.
- **Event system** — Y-up mouse events routed by hit-test with proper Z-order.
  Capture semantics for drag. Keyboard focus with Tab navigation and focus rings.
- **Multi-touch** — gesture aggregator (`current_multi_touch()`) exposes per-frame
  zoom / rotation / translation / pressure deltas. Works on mobile browsers and
  touchscreen laptops.
- **Drawing API** — `DrawCtx` covers paths, fills, strokes, rounded rects, circles,
  arcs, Bézier curves, text, transforms, clipping, compositing layers, image blitting,
  and inline GL content. Two implementations: software AGG rasterizer + halo-AA GL path.
- **Inspector** — built-in widget-tree inspector overlay highlighting hovered widgets,
  showing bounds and properties, reporting hover position.

## Quick Start

```rust,ignore
use agg_gui::{App, FlexColumn, Label, Button};
use std::sync::Arc;

let font = Arc::new(agg_gui::Font::from_slice(FONT_BYTES).unwrap());

let root = FlexColumn::new()
    .with_gap(8.0)
    .with_padding(16.0)
    .add(Box::new(Label::new("Hello, world!", Arc::clone(&font))))
    .add(Box::new(
        Button::new("Click me", Arc::clone(&font))
            .on_click(|| println!("clicked"))
    ));

let mut app = App::new(Box::new(root));
// Feed OS events via `app.on_mouse_*` / `app.on_key_down`;
// call `app.layout(size)` + `app.paint(&mut ctx)` each frame.
```

See the [demo shell](https://github.com/larsbrubaker/agg-gui) for a complete example
covering 28+ demo windows, themes, persistence, and a GL 3-D cube.

## Design Principles

- **Y-up coordinates everywhere** — origin at bottom-left, positive Y upward. One conversion at event ingestion; no per-widget flipping.
- **Direct-to-surface rendering** — AGG paths rasterize straight to the target surface. No retained scene graph, no layout cache to invalidate.
- **Full redraw every frame** — no dirty regions, no incremental update complexity.
- **Theme via thread-local** — `set_visuals()` writes to a thread-local read by every `DrawCtx::visuals()` call. Zero plumbing required in widget constructors.
- **Two-way state binding** — `Rc<Cell<T>>` shared between widgets keeps UI in sync without callbacks.
- **No unsafe, no `RefCell` pervasion** — the widget tree is owned by `App`; mutable traversal uses index-based child access to satisfy the borrow checker cleanly.

## License

MIT
