//! Composed `Widget` tree for the node-editor canvas.
//!
//! Every visible piece of a node is now a real `Widget` with a proper
//! child-parent relationship:
//!
//! ```text
//! NodeWidget                       — the node body + chrome
//! ├── NodeHeaderWidget             — title bar (drawn first)
//! └── NodeRowWidget* (one per row)
//!     ├── SocketDotWidget?         — the connector dot (left or right)
//!     ├── RowLabelWidget           — the row's text label
//!     └── ValueEditorWidget?       — inline number / color / bool editor
//! ```
//!
//! Coordinates follow agg-gui's convention: parent-local, Y-up, origin
//! at the parent's **bottom-left** corner.  `NodeWidget`'s own bounds
//! live in canvas-space — `NodeEditor` already has the pan/zoom transform
//! applied to its `DrawCtx` when it calls `paint_subtree` on the node
//! widgets, so canvas-space happens to be the right space for the
//! `NodeWidget` bounds.
//!
//! The widgets are paint-side only: they consume an immutable
//! `NodeLayoutInfo` produced by `crate::draw` plus the live `CanvasPalette`
//! and `NodeGraphModel`.  Hit-testing for selection, drag, and connection
//! drawing continues to flow through `NodeLayoutInfo` on `NodeEditor`
//! itself; the per-widget bounds give the inspector a real tree to walk
//! without forcing a second event-routing rewrite.

use std::sync::Arc;

use agg_gui::{
    Color, DrawCtx, Event, EventResult, HAnchor, Insets, Rect, Size, VAnchor, Widget, WidgetBase,
};

use crate::draw::{
    CanvasPalette, NodeLayoutInfo, NodeRow, PropLayout, SocketLayout, SocketSide, NODE_RADIUS,
    ROW_HEIGHT, SOCKET_RADIUS, TITLE_HEIGHT,
};
use crate::model::{NodeGraphModel, PropertyValue};

const ROW_PADDING_X: f64 = 6.0;
const LABEL_FONT_SIZE: f64 = 11.0;
const TITLE_FONT_SIZE: f64 = 13.0;

/// Shared per-frame context every node widget needs to render.  Cloning
/// the `Arc` is cheap — the inner data is rebuilt by `NodeEditor` each
/// paint frame.
#[derive(Clone)]
pub struct NodePaintContext {
    pub palette: Arc<CanvasPalette>,
    /// Socket colour lookup by type id.  Captured up-front so the row /
    /// socket widgets don't need to lock the host model during paint.
    pub socket_colors: Arc<dyn Fn(crate::model::SocketTypeId) -> Color + Send + Sync>,
    /// Title-bar colour lookup by category.
    pub title_colors: Arc<dyn Fn(&str, Color) -> Color + Send + Sync>,
}

impl NodePaintContext {
    /// Build a fresh context from the live palette and model.  Resolves
    /// socket / title colours by snapshotting the model into owned
    /// closures so the widgets don't reach back into the model later.
    pub fn from_model<M: NodeGraphModel + ?Sized>(palette: CanvasPalette, model: &M) -> Self {
        // Capture the model's colour data into a small owned table the
        // closures can read from without needing the borrow.  Sockets +
        // categories tend to be tiny (single digit count), so an alloc
        // per paint is fine.
        let mut socket_pairs: Vec<(crate::model::SocketTypeId, Color)> = Vec::new();
        for (ty, col) in collect_socket_colors(model) {
            socket_pairs.push((ty, col));
        }
        let socket_pairs = Arc::new(socket_pairs);
        let socket_pairs_clone = socket_pairs.clone();
        let socket_colors = Arc::new(move |ty: crate::model::SocketTypeId| -> Color {
            socket_pairs_clone
                .iter()
                .find(|(t, _)| *t == ty)
                .map(|(_, c)| *c)
                .unwrap_or_else(|| Color::rgba(0.55, 0.58, 0.66, 1.0))
        }) as Arc<dyn Fn(_) -> _ + Send + Sync>;

        let mut category_pairs: Vec<(String, Color)> = Vec::new();
        for (cat, col) in collect_category_colors(model, palette.node_title_fallback) {
            category_pairs.push((cat, col));
        }
        let category_pairs = Arc::new(category_pairs);
        let category_pairs_clone = category_pairs.clone();
        let title_colors: Arc<dyn Fn(&str, Color) -> Color + Send + Sync> =
            Arc::new(move |cat: &str, fallback: Color| -> Color {
                category_pairs_clone
                    .iter()
                    .find(|(c, _)| c == cat)
                    .map(|(_, col)| *col)
                    .unwrap_or(fallback)
            });

        Self {
            palette: Arc::new(palette),
            socket_colors,
            title_colors,
        }
    }
}

fn collect_socket_colors<M: NodeGraphModel + ?Sized>(
    model: &M,
) -> Vec<(crate::model::SocketTypeId, Color)> {
    let mut seen: Vec<crate::model::SocketTypeId> = Vec::new();
    for n in model.nodes() {
        for s in n.inputs.iter().chain(n.outputs.iter()) {
            if !seen.contains(&s.socket_type) {
                seen.push(s.socket_type);
            }
        }
    }
    seen.into_iter()
        .map(|ty| (ty, model.socket_color(ty)))
        .collect()
}

fn collect_category_colors<M: NodeGraphModel + ?Sized>(
    model: &M,
    fallback: Color,
) -> Vec<(String, Color)> {
    let mut seen: Vec<String> = Vec::new();
    for n in model.nodes() {
        if !seen.contains(&n.category) {
            seen.push(n.category.clone());
        }
    }
    seen.into_iter()
        .map(|c| {
            let col = model.category_color(&c, fallback);
            (c, col)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// NodeWidget — the top-level node container
// ---------------------------------------------------------------------------

/// A full node — chrome (body, header, border) plus a row child for
/// every output, input, and unbound property.
pub struct NodeWidget {
    bounds: Rect,
    base: WidgetBase,
    children: Vec<Box<dyn Widget>>,
    node_id: crate::model::NodeId,
    display_name: String,
    category: String,
    selected: bool,
    ctx: NodePaintContext,
}

impl NodeWidget {
    /// Construct a fresh widget tree mirroring `layout`.  The returned
    /// widget already carries its row children in the correct order.
    pub fn from_layout(layout: &NodeLayoutInfo, selected: bool, ctx: NodePaintContext) -> Self {
        let w = layout.size[0];
        let h = layout.size[1];
        // Canvas top-left → widget bottom-left in agg-gui's Y-up frame.
        let canvas_bottom_y = layout.top_left[1] - h;
        let bounds = Rect::new(layout.top_left[0], canvas_bottom_y, w, h);

        let mut children: Vec<Box<dyn Widget>> = Vec::with_capacity(layout.rows.len() + 1);
        children.push(Box::new(NodeHeaderWidget::new(
            w,
            h,
            layout.display_name.clone(),
            layout.category.clone(),
            ctx.clone(),
        )));

        for (row_index, row) in layout.rows.iter().enumerate() {
            children.push(Box::new(NodeRowWidget::from_row(
                row,
                row_index,
                w,
                h,
                ctx.clone(),
            )));
        }

        Self {
            bounds,
            base: WidgetBase::new()
                .with_h_anchor(HAnchor::FIT)
                .with_v_anchor(VAnchor::FIT),
            children,
            node_id: layout.node_id,
            display_name: layout.display_name.clone(),
            category: layout.category.clone(),
            selected,
            ctx,
        }
    }

    pub fn node_id(&self) -> crate::model::NodeId {
        self.node_id
    }
}

impl Widget for NodeWidget {
    fn type_name(&self) -> &'static str {
        "NodeWidget"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }
    fn widget_base_mut(&mut self) -> Option<&mut WidgetBase> {
        Some(&mut self.base)
    }
    fn h_anchor(&self) -> HAnchor {
        self.base.h_anchor
    }
    fn v_anchor(&self) -> VAnchor {
        self.base.v_anchor
    }
    fn margin(&self) -> Insets {
        self.base.margin
    }
    // The canvas pans / zooms in fractional units; force-snapping to
    // device pixels at every node would visibly jitter during pan.
    fn enforce_integer_bounds(&self) -> bool {
        false
    }
    fn properties(&self) -> Vec<(&'static str, String)> {
        vec![
            ("node_id", format!("{}", self.node_id.0)),
            ("display_name", self.display_name.clone()),
            ("category", self.category.clone()),
            ("selected", format!("{}", self.selected)),
        ]
    }

    fn layout(&mut self, available: Size) -> Size {
        // Bounds are owned by the parent (the canvas) — return what we
        // already carry so we keep the node-space size.
        let _ = available;
        Size::new(self.bounds.width, self.bounds.height)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let w = self.bounds.width;
        let h = self.bounds.height;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let body_color = if self.selected {
            self.ctx.palette.node_body_selected
        } else {
            self.ctx.palette.node_body
        };
        // Body fill.
        ctx.set_fill_color(body_color);
        ctx.begin_path();
        ctx.rounded_rect(0.0, 0.0, w, h, NODE_RADIUS);
        ctx.fill();
        // Border.
        ctx.set_stroke_color(self.ctx.palette.node_border);
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.rounded_rect(0.0, 0.0, w, h, NODE_RADIUS);
        ctx.stroke();
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        // Event routing is still owned by `NodeEditor` (canvas-space
        // hit testing).  This widget exists for composition + paint.
        EventResult::Ignored
    }
}

// ---------------------------------------------------------------------------
// NodeHeaderWidget — the coloured title bar
// ---------------------------------------------------------------------------

pub struct NodeHeaderWidget {
    bounds: Rect,
    base: WidgetBase,
    children: Vec<Box<dyn Widget>>,
    title: String,
    category: String,
    ctx: NodePaintContext,
}

impl NodeHeaderWidget {
    fn new(node_w: f64, node_h: f64, title: String, category: String, ctx: NodePaintContext) -> Self {
        // Header sits at the very top of the node — bottom-left at
        // (0, node_h - TITLE_HEIGHT) in NodeWidget-local Y-up coords.
        let bounds = Rect::new(0.0, node_h - TITLE_HEIGHT, node_w, TITLE_HEIGHT);
        Self {
            bounds,
            base: WidgetBase::new(),
            children: Vec::new(),
            title,
            category,
            ctx,
        }
    }
}

impl Widget for NodeHeaderWidget {
    fn type_name(&self) -> &'static str {
        "NodeHeaderWidget"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }
    fn enforce_integer_bounds(&self) -> bool {
        false
    }
    fn layout(&mut self, _: Size) -> Size {
        Size::new(self.bounds.width, self.bounds.height)
    }
    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let w = self.bounds.width;
        let h = self.bounds.height;
        let title_color =
            (self.ctx.title_colors)(&self.category, self.ctx.palette.node_title_fallback);
        // Rounded top corners by painting a rounded rect then masking
        // the bottom strip with a rectangle.  Visually identical to
        // `draw_node_chrome`'s previous logic.
        ctx.set_fill_color(title_color);
        ctx.begin_path();
        ctx.rounded_rect(0.0, 0.0, w, h, NODE_RADIUS);
        ctx.fill();
        ctx.set_fill_color(title_color);
        ctx.begin_path();
        ctx.rect(0.0, 0.0, w, NODE_RADIUS);
        ctx.fill();

        ctx.set_fill_color(self.ctx.palette.label_text);
        ctx.set_font_size(TITLE_FONT_SIZE);
        // Text baseline ~4px above the header's bottom, matching the
        // previous procedural layout.
        ctx.fill_text(&self.title, 10.0, h * 0.5 - 4.0);
    }
    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

// ---------------------------------------------------------------------------
// NodeRowWidget — a single row inside a node, with its own sub-widget tree
// ---------------------------------------------------------------------------

pub struct NodeRowWidget {
    bounds: Rect,
    base: WidgetBase,
    children: Vec<Box<dyn Widget>>,
    row_name: String,
    row_kind: RowKind,
}

#[derive(Clone, Debug)]
enum RowKind {
    Output,
    Input { has_editor: bool },
    Property,
}

impl NodeRowWidget {
    fn from_row(
        row: &NodeRow,
        row_index: usize,
        node_w: f64,
        node_h: f64,
        ctx: NodePaintContext,
    ) -> Self {
        // Row at `row_index` (0 = top, directly under the title) sits at
        // y ∈ [node_h - TITLE_HEIGHT - (row_index+1)*ROW_HEIGHT,
        //      node_h - TITLE_HEIGHT - row_index *ROW_HEIGHT].
        let row_top = node_h - TITLE_HEIGHT - (row_index as f64) * ROW_HEIGHT;
        let row_bot = row_top - ROW_HEIGHT;
        let bounds = Rect::new(0.0, row_bot, node_w, ROW_HEIGHT);

        let (row_name, row_kind, children) = match row {
            NodeRow::Output(socket) => {
                let mut children: Vec<Box<dyn Widget>> = Vec::new();
                children.push(Box::new(SocketDotWidget::new(
                    socket.clone(),
                    SocketSide::Output,
                    node_w,
                    ROW_HEIGHT,
                    ctx.clone(),
                )));
                children.push(Box::new(RowLabelWidget::new_right(
                    socket.display_label.clone(),
                    node_w,
                    ROW_HEIGHT,
                    ctx.clone(),
                )));
                (
                    format!("output:{}", socket.name),
                    RowKind::Output,
                    children,
                )
            }
            NodeRow::Input { socket, editor } => {
                let mut children: Vec<Box<dyn Widget>> = Vec::new();
                children.push(Box::new(SocketDotWidget::new(
                    socket.clone(),
                    SocketSide::Input,
                    node_w,
                    ROW_HEIGHT,
                    ctx.clone(),
                )));
                children.push(Box::new(RowLabelWidget::new_left(
                    socket.display_label.clone(),
                    node_w,
                    ROW_HEIGHT,
                    ctx.clone(),
                )));
                let has_editor = editor.is_some();
                if let Some(ed) = editor {
                    children.push(Box::new(ValueEditorWidget::new(
                        ed.clone(),
                        node_w,
                        ROW_HEIGHT,
                        ctx.clone(),
                        /* show_label */ false,
                    )));
                }
                (
                    format!("input:{}", socket.name),
                    RowKind::Input { has_editor },
                    children,
                )
            }
            NodeRow::Property(prop) => {
                let mut children: Vec<Box<dyn Widget>> = Vec::new();
                children.push(Box::new(ValueEditorWidget::new(
                    prop.clone(),
                    node_w,
                    ROW_HEIGHT,
                    ctx.clone(),
                    /* show_label */ true,
                )));
                (
                    format!("prop:{}", prop.name),
                    RowKind::Property,
                    children,
                )
            }
        };

        Self {
            bounds,
            base: WidgetBase::new(),
            children,
            row_name,
            row_kind,
        }
    }
}

impl Widget for NodeRowWidget {
    fn type_name(&self) -> &'static str {
        "NodeRowWidget"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }
    fn enforce_integer_bounds(&self) -> bool {
        false
    }
    fn properties(&self) -> Vec<(&'static str, String)> {
        vec![
            ("row", self.row_name.clone()),
            (
                "kind",
                match &self.row_kind {
                    RowKind::Output => "output".into(),
                    RowKind::Input { has_editor } => format!("input(editor={has_editor})"),
                    RowKind::Property => "property".into(),
                },
            ),
        ]
    }
    fn layout(&mut self, _: Size) -> Size {
        Size::new(self.bounds.width, self.bounds.height)
    }
    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {
        // Row backdrop is invisible — visuals come from children.
    }
    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

// ---------------------------------------------------------------------------
// SocketDotWidget — the coloured circle on the left or right edge
// ---------------------------------------------------------------------------

pub struct SocketDotWidget {
    bounds: Rect,
    base: WidgetBase,
    children: Vec<Box<dyn Widget>>,
    socket: SocketLayout,
    side: SocketSide,
    ctx: NodePaintContext,
}

impl SocketDotWidget {
    fn new(
        socket: SocketLayout,
        side: SocketSide,
        node_w: f64,
        row_h: f64,
        ctx: NodePaintContext,
    ) -> Self {
        // The dot is drawn at `socket.center` in canvas-space, which the
        // row layout puts at the row's vertical centre.  In row-local
        // coordinates that's (0, row_h/2) for an input or (node_w, row_h/2)
        // for an output.  The widget bounds are a small square centred on
        // that point so the inspector outlines feel right.
        let cx = match side {
            SocketSide::Input => 0.0,
            SocketSide::Output => node_w,
        };
        let cy = row_h * 0.5;
        let r = SOCKET_RADIUS;
        let bounds = Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r);
        Self {
            bounds,
            base: WidgetBase::new(),
            children: Vec::new(),
            socket,
            side,
            ctx,
        }
    }
}

impl Widget for SocketDotWidget {
    fn type_name(&self) -> &'static str {
        "SocketDotWidget"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }
    fn enforce_integer_bounds(&self) -> bool {
        false
    }
    fn properties(&self) -> Vec<(&'static str, String)> {
        vec![
            ("socket", self.socket.name.clone()),
            (
                "side",
                match self.side {
                    SocketSide::Input => "input".into(),
                    SocketSide::Output => "output".into(),
                },
            ),
            ("type", format!("{}", self.socket.socket_type.0)),
        ]
    }
    fn layout(&mut self, _: Size) -> Size {
        Size::new(self.bounds.width, self.bounds.height)
    }
    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        // The widget is a 2R x 2R square; draw the dot at its centre in
        // local coords.  `bounds.width` is exactly 2*SOCKET_RADIUS so
        // we can recover the radius without referencing the constant.
        let r = self.bounds.width * 0.5;
        let cx = r;
        let cy = self.bounds.height * 0.5;
        let fill = (self.ctx.socket_colors)(self.socket.socket_type);
        ctx.set_fill_color(fill);
        ctx.begin_path();
        ctx.circle(cx, cy, r);
        ctx.fill();
        ctx.set_stroke_color(self.ctx.palette.node_border);
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.circle(cx, cy, r);
        ctx.stroke();
    }
    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

// ---------------------------------------------------------------------------
// RowLabelWidget — the row's text label
// ---------------------------------------------------------------------------

/// Where the label hugs the row — left edge (input rows) or right edge
/// (output rows).
#[derive(Clone, Copy, Debug)]
enum LabelSide {
    Left,
    Right,
}

pub struct RowLabelWidget {
    bounds: Rect,
    base: WidgetBase,
    children: Vec<Box<dyn Widget>>,
    text: String,
    side: LabelSide,
    ctx: NodePaintContext,
}

impl RowLabelWidget {
    fn new_left(text: String, node_w: f64, row_h: f64, ctx: NodePaintContext) -> Self {
        // Reserve from the dot's right edge to the right edge of the
        // row.  Painting reads `text_x` from `side`.
        let left = SOCKET_RADIUS * 2.0 + ROW_PADDING_X;
        let bounds = Rect::new(left, 0.0, (node_w - left).max(0.0), row_h);
        Self {
            bounds,
            base: WidgetBase::new(),
            children: Vec::new(),
            text,
            side: LabelSide::Left,
            ctx,
        }
    }

    fn new_right(text: String, node_w: f64, row_h: f64, ctx: NodePaintContext) -> Self {
        let right_inset = SOCKET_RADIUS * 2.0 + ROW_PADDING_X;
        let width = (node_w - right_inset).max(0.0);
        let bounds = Rect::new(0.0, 0.0, width, row_h);
        Self {
            bounds,
            base: WidgetBase::new(),
            children: Vec::new(),
            text,
            side: LabelSide::Right,
            ctx,
        }
    }
}

impl Widget for RowLabelWidget {
    fn type_name(&self) -> &'static str {
        "RowLabelWidget"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }
    fn enforce_integer_bounds(&self) -> bool {
        false
    }
    fn properties(&self) -> Vec<(&'static str, String)> {
        vec![("text", self.text.clone())]
    }
    fn layout(&mut self, _: Size) -> Size {
        Size::new(self.bounds.width, self.bounds.height)
    }
    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        if self.text.is_empty() {
            return;
        }
        ctx.set_fill_color(self.ctx.palette.label_text);
        ctx.set_font_size(LABEL_FONT_SIZE);
        let baseline_y = self.bounds.height * 0.5 - 4.0;
        let x = match self.side {
            LabelSide::Left => 0.0,
            LabelSide::Right => {
                let est = (self.text.len() as f64) * 6.5;
                (self.bounds.width - est).max(0.0)
            }
        };
        ctx.fill_text(&self.text, x, baseline_y);
    }
    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

// ---------------------------------------------------------------------------
// ValueEditorWidget — the inline number / colour / bool pill
// ---------------------------------------------------------------------------

pub struct ValueEditorWidget {
    bounds: Rect,
    base: WidgetBase,
    children: Vec<Box<dyn Widget>>,
    prop: PropLayout,
    /// When `true` the editor draws its own row label on the left side —
    /// used for unbound property rows that don't have a sibling
    /// `RowLabelWidget`.
    show_label: bool,
    ctx: NodePaintContext,
}

impl ValueEditorWidget {
    fn new(
        prop: PropLayout,
        node_w: f64,
        row_h: f64,
        ctx: NodePaintContext,
        show_label: bool,
    ) -> Self {
        // The PropLayout carries canvas-space coords; convert to
        // row-local by subtracting the row's left edge (node-local 0)
        // and the row's bottom edge.  The simpler path is to read the
        // PropLayout's own width and right-align inside the row.
        let width = prop.size[0];
        let row_left = node_w - width - SOCKET_RADIUS;
        // For unbound property rows the editor spans the full inner
        // width — detect via `show_label`.
        let (x, w) = if show_label {
            (1.0, node_w - 2.0)
        } else {
            (row_left, width)
        };
        let bounds = Rect::new(x, 1.0, w, row_h - 2.0);
        Self {
            bounds,
            base: WidgetBase::new(),
            children: Vec::new(),
            prop,
            show_label,
            ctx,
        }
    }
}

impl Widget for ValueEditorWidget {
    fn type_name(&self) -> &'static str {
        "ValueEditorWidget"
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
    }
    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }
    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }
    fn enforce_integer_bounds(&self) -> bool {
        false
    }
    fn properties(&self) -> Vec<(&'static str, String)> {
        vec![
            ("property", self.prop.name.clone()),
            ("value", format_value(&self.prop.current)),
        ]
    }
    fn layout(&mut self, _: Size) -> Size {
        Size::new(self.bounds.width, self.bounds.height)
    }
    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let w = self.bounds.width;
        let h = self.bounds.height;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let body = self.ctx.palette.node_body;
        let body_lum = 0.299 * body.r + 0.587 * body.g + 0.114 * body.b;
        let pill_bg = if body_lum < 0.5 {
            Color::rgba(0.15, 0.16, 0.20, 0.9)
        } else {
            Color::rgba(0.93, 0.93, 0.94, 0.9)
        };

        ctx.set_fill_color(pill_bg);
        ctx.begin_path();
        ctx.rounded_rect(0.0, 0.0, w, h, 3.0);
        ctx.fill();

        if let PropertyValue::Color(c) = &self.prop.current {
            let inset = 3.0;
            ctx.set_fill_color(Color::rgba(c[0], c[1], c[2], c[3]));
            ctx.begin_path();
            ctx.rounded_rect(inset, inset, (w - 2.0 * inset).max(0.0), (h - 2.0 * inset).max(0.0), 2.0);
            ctx.fill();
            return;
        }

        // Optional left-aligned label (only for unbound property rows).
        if self.show_label {
            ctx.set_fill_color(self.ctx.palette.label_text);
            ctx.set_font_size(LABEL_FONT_SIZE);
            ctx.fill_text(&self.prop.name, ROW_PADDING_X, h * 0.5 - 4.0);
        }

        let value_str = format_value(&self.prop.current);
        if value_str.is_empty() {
            return;
        }
        ctx.set_fill_color(self.ctx.palette.label_text);
        ctx.set_font_size(LABEL_FONT_SIZE);
        let est = (value_str.len() as f64) * 6.0;
        let x = (w - est - 6.0).max(0.0);
        ctx.fill_text(&value_str, x, h * 0.5 - 4.0);
    }
    fn on_event(&mut self, _: &Event) -> EventResult {
        // Drag-edit dispatch still happens through `NodeEditor` because
        // canvas-space hit-testing already exists there.
        EventResult::Ignored
    }
}

fn format_value(v: &PropertyValue) -> String {
    match v {
        PropertyValue::Number(n) => {
            if n.fract().abs() < 1e-6 {
                format!("{}", *n as i64)
            } else {
                format!("{:.3}", n)
            }
        }
        PropertyValue::Bool(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        PropertyValue::Color(_) => String::new(),
        PropertyValue::Other { display } => display.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::{layout_node, NODE_WIDTH};
    use crate::model::{NodeId, NodeView, PropertyView, SocketTypeId, SocketView};

    struct DummyModel;
    impl NodeGraphModel for DummyModel {
        fn nodes(&self) -> Vec<NodeView> {
            vec![]
        }
        fn edges(&self) -> Vec<crate::model::EdgeView> {
            vec![]
        }
        fn node_types_by_category(&self) -> Vec<(String, Vec<crate::model::NodeTypeView>)> {
            vec![]
        }
        fn set_node_position(&mut self, _: NodeId, _: [f64; 2]) {}
        fn add_node(&mut self, _: &str, _: [f64; 2]) -> Option<NodeId> {
            None
        }
        fn remove_node(&mut self, _: NodeId) {}
        fn try_add_edge(
            &mut self,
            _: NodeId,
            _: &str,
            _: NodeId,
            _: &str,
        ) -> crate::model::EdgeResult {
            crate::model::EdgeResult::Rejected
        }
        fn set_property(&mut self, _: NodeId, _: &str, _: PropertyValue) {}
    }

    fn make_node() -> NodeView {
        NodeView {
            id: NodeId(42),
            type_id: "Extrude".into(),
            display_name: "Extrude".into(),
            category: "Operations 3D".into(),
            position: [10.0, 50.0],
            outputs: vec![SocketView {
                name: "Geometry".into(),
                socket_type: SocketTypeId(7),
                display_label: Some("Geometry".into()),
            }],
            inputs: vec![SocketView {
                name: "Paths".into(),
                socket_type: SocketTypeId(6),
                display_label: Some("Paths".into()),
            }],
            properties: vec![PropertyView {
                name: "height".into(),
                display_label: Some("Height".into()),
                current: PropertyValue::Number(5.0),
                min: Some(0.0),
                max: Some(40.0),
                bound_input: None,
            }],
        }
    }

    // Keep `NODE_WIDTH` referenced from the test module so the import
    // is genuinely used regardless of optimisation level.
    #[test]
    fn imported_node_width_matches_layout_default() {
        let layout = layout_node(&make_node());
        assert!((layout.size[0] - NODE_WIDTH).abs() < 1e-9);
    }

    #[test]
    fn node_widget_carries_header_and_row_children() {
        let layout = layout_node(&make_node());
        let ctx = NodePaintContext::from_model(CanvasPalette::dark(), &DummyModel);
        let nw = NodeWidget::from_layout(&layout, false, ctx);
        // First child = header; remaining children = rows.
        assert!(!nw.children().is_empty());
        assert_eq!(nw.children()[0].type_name(), "NodeHeaderWidget");
        let row_count = layout.rows.len();
        assert_eq!(nw.children().len(), row_count + 1);
        for i in 1..=row_count {
            assert_eq!(nw.children()[i].type_name(), "NodeRowWidget");
        }
    }

    #[test]
    fn input_row_contains_socket_and_label_subwidgets() {
        let layout = layout_node(&make_node());
        let ctx = NodePaintContext::from_model(CanvasPalette::dark(), &DummyModel);
        let nw = NodeWidget::from_layout(&layout, false, ctx);
        // Find the row that owns the Paths input.
        let row = nw
            .children()
            .iter()
            .filter(|c| c.type_name() == "NodeRowWidget")
            .find(|c| {
                c.properties()
                    .iter()
                    .any(|(k, v)| *k == "row" && v == "input:Paths")
            })
            .expect("expected an input row for Paths");
        let kinds: Vec<&'static str> = row.children().iter().map(|c| c.type_name()).collect();
        assert!(kinds.contains(&"SocketDotWidget"));
        assert!(kinds.contains(&"RowLabelWidget"));
    }

    #[test]
    fn output_row_dot_sits_on_right_side() {
        let layout = layout_node(&make_node());
        let ctx = NodePaintContext::from_model(CanvasPalette::dark(), &DummyModel);
        let nw = NodeWidget::from_layout(&layout, false, ctx);
        let row = nw
            .children()
            .iter()
            .filter(|c| c.type_name() == "NodeRowWidget")
            .find(|c| {
                c.properties()
                    .iter()
                    .any(|(k, v)| *k == "row" && v == "output:Geometry")
            })
            .expect("expected an output row for Geometry");
        let dot = row
            .children()
            .iter()
            .find(|c| c.type_name() == "SocketDotWidget")
            .expect("expected a socket dot in the output row");
        // The dot's centre should land on the node's right edge — i.e.
        // bounds.x + bounds.width / 2 ≈ NODE_WIDTH (within rounding).
        let centre_x = dot.bounds().x + dot.bounds().width * 0.5;
        assert!(
            (centre_x - NODE_WIDTH).abs() < 1e-6,
            "output dot centre should hug the right edge"
        );
    }

    #[test]
    fn property_row_owns_value_editor() {
        let layout = layout_node(&make_node());
        let ctx = NodePaintContext::from_model(CanvasPalette::dark(), &DummyModel);
        let nw = NodeWidget::from_layout(&layout, false, ctx);
        let row = nw
            .children()
            .iter()
            .filter(|c| c.type_name() == "NodeRowWidget")
            .find(|c| {
                c.properties()
                    .iter()
                    .any(|(k, v)| *k == "row" && v == "prop:height")
            })
            .expect("expected a property row for height");
        let kinds: Vec<&'static str> = row.children().iter().map(|c| c.type_name()).collect();
        assert_eq!(kinds, vec!["ValueEditorWidget"]);
    }
}
