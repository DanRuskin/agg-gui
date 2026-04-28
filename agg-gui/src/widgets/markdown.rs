//! `MarkdownView` — render a Markdown string as formatted text with images.
//!
//! Uses `pulldown-cmark` for parsing, then converts the event stream into a
//! flat list of styled lines, inline image runs, and image placeholders. Word-wrapping is
//! computed in `layout()` using the standalone `measure_text_metrics` function
//! so no `DrawCtx` is needed at layout time.
//!
//! # Image support
//!
//! Pass an `image_provider` closure via [`MarkdownView::with_image_provider`].
//! It receives the image URL/path string and should return
//! `Some((rgba_bytes, width, height))` or `None` for unknown images.  The data
//! must be tightly-packed RGBA8 in row-major order, **top-row first**.
//!
//! Images are decoded and cached on the first `layout()` call and then drawn
//! via `DrawCtx::draw_image_rgba()` on every `paint()`.
//!
//! # Supported Markdown features
//!
//! - Headings H1–H4 (larger font sizes)
//! - Paragraphs (word-wrapped)
//! - Bullet lists (`-`/`*`) with "• " prefix
//! - Ordered lists with "N. " prefix
//! - Inline code `` `x` `` (highlight)
//! - Fenced code blocks (background box)
//! - Horizontal rules (thin separator line)
//! - Images via `image_provider` callback; compact inline placeholder when unavailable
//! - Links (coloured text, URL is not opened — add `on_link_click` if needed)

use std::sync::Arc;

use pulldown_cmark::{Event as MdEvent, Options, Parser, Tag, TagEnd};

use crate::color::Color;
use crate::draw_ctx::DrawCtx;
use crate::event::{Event, EventResult};
use crate::geometry::{Rect, Size};
use crate::layout_props::{HAnchor, Insets, VAnchor, WidgetBase};
use crate::text::{measure_text_metrics, Font};
use crate::widget::Widget;

// ── Styled line representation ─────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum LineStyle {
    Body,
    H1,
    H2,
    H3,
    H4,
    Code,
    Rule,
}

impl LineStyle {
    fn font_size(self, base: f64) -> f64 {
        match self {
            LineStyle::H1 => base * 1.8,
            LineStyle::H2 => base * 1.5,
            LineStyle::H3 => base * 1.25,
            LineStyle::H4 => base * 1.1,
            LineStyle::Body => base,
            LineStyle::Code => base * 0.9,
            LineStyle::Rule => base,
        }
    }
}

// ── Layout item ────────────────────────────────────────────────────────────────

/// A single item in the laid-out view.
#[derive(Clone)]
enum LayoutItem {
    /// A text row (including blank spacing rows and horizontal rules).
    Line {
        runs: Vec<LineRun>,
        style: LineStyle,
        indent: f64,
        y: f64,
        height: f64,
    },
}

#[derive(Clone)]
enum LineRun {
    Text {
        text: String,
        x: f64,
    },
    Image {
        alt: String,
        cache_idx: usize,
        x: f64,
        y_offset: f64,
        width: f64,
        height: f64,
    },
}

// ── Intermediate paragraph item (before layout) ────────────────────────────────

#[derive(Clone)]
enum InlineItem {
    Text(String),
    Image { url: String, alt: String },
}

enum ParagraphItem {
    Flow {
        items: Vec<InlineItem>,
        style: LineStyle,
        indent: f64,
    },
    Spacer,
    Rule,
}

// ── Image cache entry ──────────────────────────────────────────────────────────

struct ImageEntry {
    url: String,
    /// `None` = provider returned nothing, `Some(...)` = decoded image.
    data: Option<(Vec<u8>, u32, u32)>,
}

// ── MarkdownView widget ────────────────────────────────────────────────────────

/// A widget that renders a Markdown string as formatted, word-wrapped text
/// with optional image support.
pub struct MarkdownView {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    base: WidgetBase,

    markdown: String,
    font: Arc<Font>,
    font_size: f64,
    padding: f64,

    /// Optional image decoder.  Receives a URL/path, returns RGBA8 pixel data
    /// (top-row first) + (width, height), or `None` if unavailable.
    image_provider: Option<Box<dyn Fn(&str) -> Option<(Vec<u8>, u32, u32)>>>,

    /// Cached image data, indexed by `LineRun::Image::cache_idx`.
    image_cache: Vec<ImageEntry>,

    /// Laid-out items (populated by `layout()`).
    items: Vec<LayoutItem>,
    /// Total content height from the last layout pass.
    content_h: f64,
}

impl MarkdownView {
    pub fn new(markdown: impl Into<String>, font: Arc<Font>) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            base: WidgetBase::new(),
            markdown: markdown.into(),
            font,
            font_size: 14.0,
            padding: 8.0,
            image_provider: None,
            image_cache: Vec::new(),
            items: Vec::new(),
            content_h: 0.0,
        }
    }

    pub fn with_font_size(mut self, size: f64) -> Self {
        self.font_size = size;
        self
    }
    pub fn with_padding(mut self, p: f64) -> Self {
        self.padding = p;
        self
    }

    /// Currently-active font — honours the thread-local system-font override
    /// (`font_settings::current_system_font`) so system-font changes propagate
    /// live without rebuilding the markdown view.
    fn active_font(&self) -> Arc<Font> {
        crate::font_settings::current_system_font().unwrap_or_else(|| Arc::clone(&self.font))
    }

    /// Supply an image provider closure.
    ///
    /// The closure receives a URL/path string from the Markdown source and must
    /// return `Some((rgba_bytes, width, height))` or `None`.
    pub fn with_image_provider(
        mut self,
        provider: impl Fn(&str) -> Option<(Vec<u8>, u32, u32)> + 'static,
    ) -> Self {
        self.image_provider = Some(Box::new(provider));
        self
    }

    pub fn with_margin(mut self, m: Insets) -> Self {
        self.base.margin = m;
        self
    }
    pub fn with_h_anchor(mut self, h: HAnchor) -> Self {
        self.base.h_anchor = h;
        self
    }
    pub fn with_v_anchor(mut self, v: VAnchor) -> Self {
        self.base.v_anchor = v;
        self
    }

    // ── Markdown → paragraph items ────────────────────────────────────────────

    fn parse_paragraphs(&self) -> Vec<ParagraphItem> {
        let mut out = Vec::new();
        let opts =
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
        let parser = Parser::new_ext(&self.markdown, opts);

        let mut cur_items = Vec::new();
        let mut cur_text = String::new();
        let mut cur_style = LineStyle::Body;
        let mut cur_indent = 0.0_f64;
        let mut list_depth = 0u32;
        let mut list_ordinal: Vec<u64> = Vec::new();
        let mut in_image: Option<String> = None;

        fn flush_text(items: &mut Vec<InlineItem>, text: &mut String) {
            let t = text.trim().to_string();
            if !t.is_empty() {
                items.push(InlineItem::Text(t));
            }
            text.clear();
        }
        fn flush_flow(
            out: &mut Vec<ParagraphItem>,
            items: &mut Vec<InlineItem>,
            text: &mut String,
            style: LineStyle,
            indent: f64,
        ) {
            flush_text(items, text);
            if !items.is_empty() {
                out.push(ParagraphItem::Flow {
                    items: std::mem::take(items),
                    style,
                    indent,
                });
            }
        }
        fn add_spacer(out: &mut Vec<ParagraphItem>) {
            if !matches!(out.last(), Some(ParagraphItem::Spacer)) {
                out.push(ParagraphItem::Spacer);
            }
        }
        fn append_text(text: &mut String, value: &str) {
            if !text.is_empty() && !text.ends_with(' ') && !text.ends_with('\n') {
                text.push(' ');
            }
            text.push_str(value.trim_start());
        }

        for ev in parser {
            match ev {
                MdEvent::Start(Tag::Image { dest_url, .. }) => {
                    flush_text(&mut cur_items, &mut cur_text);
                    in_image = Some(dest_url.to_string());
                }
                MdEvent::End(TagEnd::Image) => {
                    if let Some(url) = in_image.take() {
                        let alt = cur_text.trim().to_string();
                        cur_text.clear();
                        cur_items.push(InlineItem::Image { url, alt });
                    }
                }
                MdEvent::Text(t) if in_image.is_some() => cur_text.push_str(&t),
                MdEvent::Start(Tag::Heading { level, .. }) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                    cur_style = match level as u8 {
                        1 => LineStyle::H1,
                        2 => LineStyle::H2,
                        3 => LineStyle::H3,
                        _ => LineStyle::H4,
                    };
                    cur_indent = 0.0;
                }
                MdEvent::End(TagEnd::Heading(_)) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                    add_spacer(&mut out);
                    cur_style = LineStyle::Body;
                    cur_indent = 0.0;
                }
                MdEvent::Start(Tag::Paragraph) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                }
                MdEvent::End(TagEnd::Paragraph) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                    add_spacer(&mut out);
                }
                MdEvent::Start(Tag::List(first)) => {
                    list_depth += 1;
                    list_ordinal.push(first.unwrap_or(1));
                    cur_indent = list_depth as f64 * 16.0;
                }
                MdEvent::End(TagEnd::List(_)) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                    list_depth = list_depth.saturating_sub(1);
                    list_ordinal.pop();
                    cur_indent = list_depth as f64 * 16.0;
                    if list_depth == 0 {
                        add_spacer(&mut out);
                    }
                }
                MdEvent::Start(Tag::Item) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                    if let Some(n) = list_ordinal.last_mut() {
                        cur_text = format!("{}. ", n);
                        *n += 1;
                    } else {
                        cur_text = "• ".to_string();
                    }
                }
                MdEvent::End(TagEnd::Item) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                }
                MdEvent::Start(Tag::CodeBlock(_)) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                    cur_style = LineStyle::Code;
                }
                MdEvent::End(TagEnd::CodeBlock) => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                    add_spacer(&mut out);
                    cur_style = LineStyle::Body;
                }
                MdEvent::Rule => {
                    flush_flow(
                        &mut out,
                        &mut cur_items,
                        &mut cur_text,
                        cur_style,
                        cur_indent,
                    );
                    out.push(ParagraphItem::Rule);
                }
                MdEvent::Text(t) => append_text(&mut cur_text, &t),
                MdEvent::Code(t) => append_text(&mut cur_text, &format!("`{t}`")),
                MdEvent::SoftBreak | MdEvent::HardBreak => cur_text.push(' '),
                MdEvent::Start(Tag::Link { .. }) | MdEvent::End(TagEnd::Link) => {}
                _ => {}
            }
        }
        flush_flow(
            &mut out,
            &mut cur_items,
            &mut cur_text,
            cur_style,
            cur_indent,
        );
        out
    }

    // ── Word-wrapping ─────────────────────────────────────────────────────────

    fn text_width(&self, text: &str, style: LineStyle) -> f64 {
        let font_size = style.font_size(self.font_size);
        measure_text_metrics(&self.active_font(), text, font_size).width
    }

    fn inline_image_size(&self, cache_idx: usize, alt: &str, max_w: f64) -> (f64, f64) {
        if let Some((_, iw, ih)) = self.image_cache[cache_idx].data.as_ref() {
            let scale = (max_w / *iw as f64).min(1.0);
            (*iw as f64 * scale, *ih as f64 * scale)
        } else {
            let label = if alt.is_empty() { "image" } else { alt };
            let w = self.text_width(label, LineStyle::Body) + 16.0;
            (w.min(max_w), self.font_size * 1.45)
        }
    }

    fn push_text_run(runs: &mut Vec<LineRun>, text: String, x: f64) {
        if let Some(LineRun::Text { text: last, .. }) = runs.last_mut() {
            last.push_str(&text);
        } else {
            runs.push(LineRun::Text { text, x });
        }
    }

    fn push_line(
        items: &mut Vec<LayoutItem>,
        runs: &mut Vec<LineRun>,
        style: LineStyle,
        indent: f64,
        height: f64,
    ) {
        for run in runs.iter_mut() {
            if let LineRun::Image {
                y_offset,
                height: image_h,
                ..
            } = run
            {
                *y_offset = (height - *image_h).max(0.0) * 0.5;
            }
        }
        items.push(LayoutItem::Line {
            runs: std::mem::take(runs),
            style,
            indent,
            y: 0.0,
            height,
        });
    }

    // ── Image cache management ────────────────────────────────────────────────

    /// Return the cache index for `url`, loading it via the provider if not yet cached.
    fn get_or_load_image(&mut self, url: &str) -> usize {
        // Check if already cached.
        if let Some(idx) = self.image_cache.iter().position(|e| e.url == url) {
            return idx;
        }
        // Load via provider.
        let data = self.image_provider.as_ref().and_then(|p| p(url));
        let idx = self.image_cache.len();
        self.image_cache.push(ImageEntry {
            url: url.to_string(),
            data,
        });
        idx
    }
}

impl Widget for MarkdownView {
    fn type_name(&self) -> &'static str {
        "MarkdownView"
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

    fn margin(&self) -> Insets {
        self.base.margin
    }
    fn h_anchor(&self) -> HAnchor {
        self.base.h_anchor
    }
    fn v_anchor(&self) -> VAnchor {
        self.base.v_anchor
    }

    fn layout(&mut self, available: Size) -> Size {
        let pad = self.padding;
        let max_w = (available.width - pad * 2.0).max(1.0);

        let paragraphs = self.parse_paragraphs();
        let mut laid_out = Vec::new();

        for item in &paragraphs {
            match item {
                ParagraphItem::Rule => laid_out.push(LayoutItem::Line {
                    runs: Vec::new(),
                    style: LineStyle::Rule,
                    indent: 0.0,
                    y: 0.0,
                    height: 8.0,
                }),
                ParagraphItem::Spacer => {
                    let metrics = measure_text_metrics(&self.active_font(), "", self.font_size);
                    laid_out.push(LayoutItem::Line {
                        runs: Vec::new(),
                        style: LineStyle::Body,
                        indent: 0.0,
                        y: 0.0,
                        height: metrics.line_height * 0.65,
                    });
                }
                ParagraphItem::Flow {
                    items,
                    style,
                    indent,
                } => {
                    let font_size = style.font_size(self.font_size);
                    let metrics = measure_text_metrics(&self.active_font(), "", font_size);
                    let line_h = metrics.line_height * 1.3;
                    let avail = (max_w - indent).max(1.0);
                    let mut runs = Vec::new();
                    let mut used = 0.0;
                    let mut row_h = line_h;
                    for inline in items {
                        match inline {
                            InlineItem::Text(text) => {
                                for word in text.split_whitespace() {
                                    let mut value = word.to_string();
                                    if used > 0.0 {
                                        value.insert(0, ' ');
                                    }
                                    let mut w = self.text_width(&value, *style);
                                    if used > 0.0 && used + w > avail {
                                        Self::push_line(
                                            &mut laid_out,
                                            &mut runs,
                                            *style,
                                            *indent,
                                            row_h,
                                        );
                                        used = 0.0;
                                        row_h = line_h;
                                        value = word.to_string();
                                        w = self.text_width(&value, *style);
                                    }
                                    Self::push_text_run(&mut runs, value, used);
                                    used += w;
                                }
                            }
                            InlineItem::Image { url, alt } => {
                                let cache_idx = self.get_or_load_image(url);
                                let (iw, ih) = self.inline_image_size(cache_idx, alt, avail);
                                if used > 0.0 && used + iw > avail {
                                    Self::push_line(
                                        &mut laid_out,
                                        &mut runs,
                                        *style,
                                        *indent,
                                        row_h,
                                    );
                                    used = 0.0;
                                    row_h = line_h;
                                }
                                runs.push(LineRun::Image {
                                    alt: alt.clone(),
                                    cache_idx,
                                    x: used,
                                    y_offset: (row_h - ih).max(0.0) * 0.5,
                                    width: iw,
                                    height: ih,
                                });
                                used += iw + 4.0;
                                row_h = row_h.max(ih);
                            }
                        }
                    }
                    if !runs.is_empty() {
                        Self::push_line(&mut laid_out, &mut runs, *style, *indent, row_h);
                    }
                }
            }
        }

        // Assign Y positions (Y-up: cursor starts at top and decrements).
        let total_h: f64 = laid_out
            .iter()
            .map(|item| match item {
                LayoutItem::Line { height, .. } => *height,
            })
            .sum::<f64>()
            + pad * 2.0;
        let mut y = total_h - pad;

        self.items.clear();
        for mut item in laid_out {
            let item_h = match &item {
                LayoutItem::Line { height, .. } => *height,
            };
            y -= item_h;
            match &mut item {
                LayoutItem::Line { y: item_y, .. } => *item_y = y,
            }
            self.items.push(item);
        }

        self.content_h = total_h;
        self.bounds = Rect::new(0.0, 0.0, available.width, total_h);
        Size::new(available.width, total_h)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let pad = self.padding;
        let w = self.bounds.width;
        let font = self.active_font();
        ctx.set_font(Arc::clone(&font));

        for item in &self.items {
            match item {
                LayoutItem::Line {
                    runs,
                    style,
                    indent,
                    y,
                    height,
                } => {
                    let fs = style.font_size(self.font_size);
                    ctx.set_font_size(fs);

                    let tx = pad + indent;
                    let ty = y + height * 0.5;
                    let metrics = measure_text_metrics(&font, "", fs);
                    let text_y = ty - (metrics.ascent - metrics.descent) * 0.5;

                    match style {
                        LineStyle::Rule => {
                            ctx.set_fill_color(v.separator);
                            ctx.begin_path();
                            ctx.rect(pad, ty, w - pad * 2.0, 1.0);
                            ctx.fill();
                        }
                        LineStyle::Code => {
                            ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.15));
                            ctx.begin_path();
                            ctx.rounded_rect(pad, *y, w - pad * 2.0, *height, 3.0);
                            ctx.fill();
                            ctx.set_fill_color(v.accent);
                            for run in runs {
                                if let LineRun::Text { text, x } = run {
                                    ctx.fill_text(text, tx + x + 4.0, text_y);
                                }
                            }
                        }
                        _ => {
                            ctx.set_fill_color(v.text_color);
                            for run in runs {
                                match run {
                                    LineRun::Text { text, x } => {
                                        ctx.fill_text(text, tx + x, text_y)
                                    }
                                    LineRun::Image {
                                        alt,
                                        cache_idx,
                                        x,
                                        y_offset,
                                        width,
                                        height,
                                    } => {
                                        let rx = tx + x;
                                        let ry = y + y_offset;
                                        if let Some(entry) = self.image_cache.get(*cache_idx) {
                                            if let Some((data, iw, ih)) = &entry.data {
                                                ctx.draw_image_rgba(
                                                    data.as_slice(),
                                                    *iw,
                                                    *ih,
                                                    rx,
                                                    ry,
                                                    *width,
                                                    *height,
                                                );
                                            } else {
                                                ctx.set_fill_color(Color::rgba(
                                                    0.5, 0.5, 0.5, 0.15,
                                                ));
                                                ctx.begin_path();
                                                ctx.rounded_rect(rx, ry, *width, *height, 3.0);
                                                ctx.fill();
                                                ctx.set_fill_color(v.text_dim);
                                                ctx.set_font_size(self.font_size * 0.85);
                                                let label = if alt.is_empty() {
                                                    "image".to_string()
                                                } else {
                                                    alt.clone()
                                                };
                                                ctx.fill_text(&label, rx + 8.0, ry + height * 0.5);
                                                ctx.set_font_size(fs);
                                                ctx.set_fill_color(v.text_color);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if matches!(style, LineStyle::H1 | LineStyle::H2) && !runs.is_empty() {
                        ctx.set_fill_color(v.separator);
                        ctx.begin_path();
                        ctx.rect(pad, *y, w - pad * 2.0, 1.0);
                        ctx.fill();
                    }
                }
            }
        }
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}
