# bevy_reflect integration — state & follow-up

Captures the current state of the `bevy_reflect` work, the chosen patterns,
and the work remaining. Hand this to a future session (or your future self)
to pick up cleanly.

---

## Why we're doing this

`bevy_reflect` powers four things we want for agg-gui's broader ambitions
(general windowing, MatterControl/TinkerCAD-style 3D editor, future Blender-
style geometry node editor):

1. **Inspector property editing** — typed editors for any reflected field
   (bool toggles, numeric scrubbers, enum dropdowns, color pickers).
2. **Unified serialization** — replace bespoke save/restore for window
   positions, theme prefs, inspector state, etc. with one TypeRegistry-driven
   path.
3. **Scene format / project files** — for editor scenarios where a tree of
   configurable values needs to roundtrip to disk.
4. **Animation by property path** — tween any reflected field by string path
   (`widget.props.value`).

Bet: adopting the most complete reflection system in the Rust ecosystem now,
while the surface is small, beats retrofitting later.

---

## What ships today

### Cargo feature plumbing

- `agg-gui/reflect` (default-on) — pulls `bevy_reflect = "0.18"`.
- `demo-gl/reflect`, `demo-ui/reflect`, `demo-native/reflect` — forward the
  feature so `cfg(feature = "reflect")` gates work consistently end-to-end.
- No-default-features build still passes (231 tests); reflect build passes
  239 tests.

Compile-time cost: ~80 s clean build for the bevy_reflect dep tree. Real
but acceptable. Disable the feature for downstream consumers who don't need
reflection.

### Phase 1 — value & state types

`#[cfg_attr(feature = "reflect", derive(bevy_reflect::Reflect))]` on:

- `Color`, `Point`, `Size`, `Rect`, `Insets`, `WidgetBase`
- `HAnchor`, `VAnchor` — newtype `u8`, marked `#[reflect(opaque)]` so the
  bitflag wrapper is treated atomically (the `0/1/2/4/...` bit values would
  be meaningless to walk individually)
- `ThemePreference`, `AccentColor` (enums)
- `Visuals` (palette struct)
- `OsWindowState`, `InspectorSavedState` (saved-state structs)

Validated by `tests/reflect_roundtrip.rs::reflect_value_types_implement_reflect`.

### Phase 2 — `Widget::as_reflect` opt-in

Two new trait methods (gated on the `reflect` feature):

```rust
fn as_reflect(&self) -> Option<&dyn bevy_reflect::Reflect> { None }
fn as_reflect_mut(&mut self) -> Option<&mut dyn bevy_reflect::Reflect> { None }
```

Default `None`; widgets opt in by overriding to return `Some(&self.props)`.

### Phase 3a — typed property dump in the inspector

`widget/tree.rs::reflect_fields` walks any `&dyn Reflect`-as-`Struct` and
produces `(name, formatted)` rows. `format_reflect_value` handles `bool`,
`f64`, `f32`, `i32`, `u32`, `usize`, `String`, `Color` cleanly; falls back
to `Debug` for anything else.

`collect_inspector_nodes` calls it after `widget.properties()`, so opt-in
widgets surface their reflected fields automatically with **no per-widget
`properties()` boilerplate**.

### Phase 3b — live editing pipeline

End-to-end working:

- `InspectorNode.path: Vec<usize>` — child-index path from App root,
  populated during snapshot collection.
- `InspectorEdit { path, field_path, new_value: Box<dyn PartialReflect> }` —
  queued edit struct.
- `walk_path_mut(root, path)` — resolves a path to `&mut dyn Widget`.
- `apply_inspector_edit(root, &edit)` — applies via
  `Reflect::reflect_path_mut` + `try_apply`, then calls `widget.mark_dirty()`
  so cache-invalidating widgets (Label) re-rasterise.
- `Widget::mark_dirty` was extended to also invalidate `BackbufferCache`
  (Label's text bitmap), not just `BackbufferState`. This matters because
  reflection bypasses setters that normally invalidate the cache.
- Inspector clickable rows: `paint_properties` records `PropHit` rects for
  each property row; bool rows produce `BoolToggle`, numeric rows
  `NumericStep` (left-half ↓, right-half ↑, step scaled to value magnitude).
  MouseDown in the props pane checks hits and pushes `InspectorEdit` onto a
  shared queue.
- `InspectorPanel::with_edit_queue(...)` builder.
- `DemoHandles::inspector_edits` shared cell.
- `render_app_frame` drains the queue **before** layout/paint each frame
  and forces an inspector-snapshot refresh so the new values appear
  immediately.

Two end-to-end tests:
- `inspector_edit_pipeline_flips_a_bool` — Checkbox.checked
- `inspector_edit_pipeline_changes_an_f64` — Slider.value

### Companion-props pattern — 5 widgets converted

Pattern documented in `SliderProps`/`CheckboxProps`/`ContainerProps`
doc-comments. Each opted-in widget defines a `*Props` struct holding only
its reflectable values:

| Widget | Props |
|---|---|
| `Slider` | `value`, `min`, `max`, `step`, `show_value`, `decimals`, `font_size` |
| `Checkbox` | `checked`, `font_size`, `label_color` |
| `ProgressBar` | `value`, `show_text`, `font_size`, `fill_color` |
| `ToggleSwitch` | `on` |
| `Container` | `background`, `border_color`, `border_width`, `corner_radius`, `inner_padding`, `fit_height` |

The widget holds `pub props: *Props` and routes all reads/writes through
it. The `as_reflect` / `as_reflect_mut` overrides return `Some(&self.props)`.

---

## Why "companion props" instead of `#[derive(Reflect)]` on the widget itself

Two structural problems force a sub-struct rather than direct widget Reflect:

1. **`bevy_reflect::Reflect` requires `Send + Sync`**, but widgets carry
   `Rc<Cell<…>>` and `Box<dyn FnMut(...)>` callbacks that aren't `Sync`.
   Single-threaded GUI state vs. ECS-component-shaped types.

2. **Sub-widgets and `Arc<Font>` are not `Reflect`** and would force a
   cascading derive across every type the widget references — including
   trait objects (`Box<dyn Widget>`) that fundamentally can't be reflected.
   `#[reflect(ignore)]` on those fields requires `Default` impls that
   widgets like `Label` (need a `Font`) can't satisfy without contortion.

The companion struct contains only `Send + Sync + Reflect`-friendly values
— `f64`, `bool`, `Option<usize>`, `Color`, etc. — and the widget routes
all reads/writes through it. The inspector edits the companion live; the
widget reacts because every getter and setter goes through `self.props`.

---

## Remaining widgets — mechanical follow-up

Each follows the same pattern; deferred not because of architectural doubt
but because each has a complication that makes a one-shot refactor risky
without dedicated time.

### Label

- ~40 field references inside `label.rs`.
- Used as a sub-widget by `Button`, `Checkbox`, `Slider`, `DragValue`,
  `Hyperlink`, `RadioGroup`, etc. — those access `label.set_text`,
  `label.set_color`, etc., not direct field access (verified). Builders
  are the only public API.
- 3 external sites set `label.buffered = false` (in `demo-ui/backend_panel`).
  Update those to `label.props.buffered = false` after the move.
- Cache invalidation is tied to setters (`set_font_size`, `set_text`,
  `set_color`, `set_align`, `clear_color`). After moving fields to
  `LabelProps`, each setter must still call `self.cache.invalidate()`.
  The `mark_dirty` hook in `apply_inspector_edit` covers the
  reflection-edit path, so a setter without cache invalidation is OK
  *for inspector edits* but breaks programmatic mutation. Keep the
  invalidations.

Reflectable fields: `text`, `font_size`, `color`, `align`, `buffered`,
`wrap`, `lcd_pref`, `ignore_system_font`.

### DragValue

- Same shape as `Slider` — straightforward Slider-style refactor.
- Reflectable: `value`, `min`, `max`, `speed`, `step`, `decimals`, `font_size`.
- Internal-only fields stay where they are: `dragging`, `mouse_pressed`,
  `press_x`, `drag_start_x`, `drag_start_value`, `focused`, `editing`,
  `edit_text`, `edit_cursor`, `hovered`, `on_change`, `value_label`.

### TextField

- Larger interaction surface (cursor, selection, IME, undo, etc.) but
  most of that is internal state, not reflectable.
- Reflectable: `text`, `placeholder`, `font_size`, `padding` (the f64,
  not Insets), `multiline?`, `password_mask?`.
- Many getters and setters; expect ~30+ field reference updates.

### FlexColumn / FlexRow

- `flex.rs` is **784/800 lines**. Adding two `*Props` struct decls + Default
  impls (~40 lines) plus the field references and `as_reflect` impls (~15
  lines) puts it well over the limit.
- **Required first step:** split the file. Sensible split: move `FlexRow`
  to `flex_row.rs`, leave `FlexColumn` in `flex.rs`, or extract the props
  struct definitions to `flex_props.rs`.
- After split, the refactor is mechanical:
  - **FlexColumn** props: `gap`, `inner_padding`, `background`,
    `use_panel_bg`, `fit_width`, `top_anchor`.
  - **FlexRow** props: `gap`, `inner_padding`, `background`.
- ~30 `self.X` references each. No external callers do direct field
  assignment (verified) — only builder methods, so the field move is
  safe.

### Other widgets worth converting (priority order)

Lower priority but follow the same pattern:

- `Button` — text, font_size, color, padding (sub-widget Label is the
  complication; once Label has props, Button is easy)
- `Hyperlink` — same shape as Button
- `RadioGroup` — selected_index, options (Vec<String> is reflectable)
- `ComboBox` — selected_index, options
- `TabView` — selected_tab, tabs
- `CollapsingHeader` — expanded, header_text
- `Window` — title, position, size, resizable, draggable (lots of state;
  carefully separate persisted from runtime)
- `ScrollView`, `Splitter`, `Resize`, `Tooltip` — mostly bounds/state
  driven, less to reflect
- `ColorPicker` — color (the *output*; would benefit greatly from a
  proper inspector color editor that doesn't yet exist)
- `MarkdownView`, `TreeView`, `WindowTitleBar`, `MenuBar` — composite
  widgets where most state isn't user-tunable

---

## Open design questions

### 1. Numeric editor UX

Current Phase 3b shows `+/-` on the right half of the row, scaled to the
value's magnitude. Crude. Better options:

- **Drag-scrub** — click-and-drag horizontally on the row, like
  `DragValue` does. The widget is *literally* `DragValue`; we could
  embed a mini DragValue per numeric reflected field.
- **Inline text edit** — click to enter a text input mode, type a number,
  Enter to commit.
- **Both** — drag for fast adjustment, double-click to text-edit for
  precision. This is what every modern DCC (Blender, Maya, Houdini) does.

Recommendation: drag-scrub first, text-edit as follow-up. Reuses
`DragValue` infra.

### 2. Color editor

`Color` is reflectable today and shows as `rgba(R, G, B, A)` strings.
For real editing we want a popover color picker per `Color`-typed field.
The `ColorPicker` widget exists — embed it in a popup triggered by clicking
a color row.

### 3. Enum dropdowns

`HAnchor` / `VAnchor` are `#[reflect(opaque)]` — the inspector treats them
as black boxes today. Two paths:

- Drop the opaque flag. They're newtype `u8`; reflection would surface a
  raw integer. Useless for the user.
- Keep opaque, special-case the inspector to recognize HAnchor/VAnchor and
  show a dropdown of the named constants (`LEFT`, `CENTER`, `RIGHT`, etc.).

For real enums (`ThemePreference`, `AccentColor`, `LabelAlign`),
`bevy_reflect` already exposes variants via `ReflectRef::Enum`. The
inspector should recognize this kind and render a dropdown. Cheap to
implement, broad payoff.

### 4. Serialization

We've not yet flipped any persisted state (window positions, inspector
saved state, theme prefs) over to `bevy_reflect`-driven serialization.
The types are all `Reflect` now — the wiring is mechanical:

- Build a `TypeRegistry` once at startup; register every persisted type.
- Replace bespoke `OsWindowState::serialize` / `deserialize` with the
  reflect serde bridge (RON or JSON).

This is a clean replacement of existing custom code, not new surface.
Worth doing because it pulls all the persisted-state plumbing onto one
codebase-wide path.

### 5. Tree structure persistence — DON'T

Tempting follow-up: "now save the whole widget tree." This will fail.
Reasons:

- `Vec<Box<dyn Widget>>` children — needs a registry of every concrete
  widget type AND a way to construct each by tag.
- Shared `Rc<RefCell<…>>` state — identity matters; can't roundtrip.
- Widget construction is imperative (depends on fonts, callbacks, host
  state).

Persist *configurable values inside widgets*, not the tree itself. Build
trees in code; reflect into them.

### 6. `bevy_reflect` version coupling

bevy_reflect ships with Bevy and breaks roughly every 3 months. Mitigation:

- Pin to a specific version (we're on 0.18).
- Gate behind the `reflect` cargo feature (already done).
- Treat upgrades as scheduled work; budget a half-day per major version.

### 7. Path representation

`InspectorEdit.field_path` is currently a `String` (e.g. `"checked"` or
`"margin.left"` or `"props.value"` once props are nested). bevy_reflect
also exposes a `ParsedPath` type that pre-validates paths. For
performance-critical code (animation tweens, not the inspector), prefer
`ParsedPath`. For one-shot inspector edits, `String` is fine.

---

## How to add a new widget to the reflection pipeline

Given an existing widget `Foo`:

1. **Inventory the inspectable fields.** Anything the user might edit at
   runtime: numeric values, bools, colors, enums, strings, simple structs.
   Skip callbacks, sub-widgets, `Rc<RefCell<…>>` shared cells, fonts,
   internal state (drag tracking, animation tweens, hover/focus flags).

2. **Define `FooProps`:**
   ```rust
   #[cfg_attr(feature = "reflect", derive(bevy_reflect::Reflect))]
   #[derive(Clone, Debug, Default)]
   pub struct FooProps {
       pub field_a: f64,
       pub field_b: bool,
       // ...
   }
   ```

3. **Replace those fields on `Foo` with `pub props: FooProps`:**
   ```rust
   pub struct Foo {
       bounds: Rect,
       children: Vec<Box<dyn Widget>>,
       base: WidgetBase,
       pub props: FooProps,
       // ...non-reflectable fields stay
   }
   ```

4. **Update the constructor and all `self.field_a` → `self.props.field_a`.**
   Mechanical. Verify with `grep`.

5. **Update builder methods to write through props:**
   ```rust
   pub fn with_field_a(mut self, v: f64) -> Self {
       self.props.field_a = v;
       self
   }
   ```

6. **Add `as_reflect` to the `Widget` impl:**
   ```rust
   #[cfg(feature = "reflect")]
   fn as_reflect(&self) -> Option<&dyn bevy_reflect::Reflect> {
       Some(&self.props)
   }
   #[cfg(feature = "reflect")]
   fn as_reflect_mut(&mut self) -> Option<&mut dyn bevy_reflect::Reflect> {
       Some(&mut self.props)
   }
   ```

7. **Run `cargo test --features reflect`** — should pass with no widget-
   specific test changes; the inspector test covers the round-trip
   automatically.

That's it. ~30 minutes per widget once you know the file.

---

## Files touched (for git archeology)

- `agg-gui/Cargo.toml` — added `bevy_reflect` dep + `reflect` feature.
- `agg-gui/src/{color,geometry,layout_props,theme,app_state}.rs` — derives.
- `agg-gui/src/widget.rs` — `as_reflect`, `as_reflect_mut`, `mark_dirty`
  extended.
- `agg-gui/src/widget/tree.rs` — `InspectorNode.path`, `InspectorEdit`,
  `walk_path_mut`, `apply_inspector_edit`, `reflect_fields`.
- `agg-gui/src/widgets/inspector.rs` — `with_edit_queue`, `PropHit`,
  click-to-edit dispatch.
- `agg-gui/src/widgets/inspector_props.rs` — hit-collection during paint.
- `agg-gui/src/widgets/{slider,checkbox,progress_bar,toggle_switch,container}.rs`
  — companion-props pattern.
- `agg-gui/src/tests/reflect_roundtrip.rs` — coverage tests.
- `demo-{gl,ui,native}/Cargo.toml` — `reflect` feature forwarding.
- `demo-gl/src/frame.rs` — drain edit queue each frame.
- `demo-ui/src/{api,app_builder}.rs` — `inspector_edits` cell + wiring.
- `demo-native/src/{rendering,main}.rs` — pass the cell into `render_frame`.

---

## Status: solid foundation, partial widget coverage

The reflection foundation is **done and validated**: feature gate, derives
on value types, `Widget::as_reflect` trait method, full edit pipeline with
end-to-end tests, 5 widgets fully reflectable and editable.

Remaining work is **mechanical** — apply the documented pattern to the
~20 other widgets — and **UX polish** — better numeric editor, color
picker integration, enum dropdowns. Neither blocks the things this
foundation enables (inspector editing today, serialization next, node
editor when you're ready).
