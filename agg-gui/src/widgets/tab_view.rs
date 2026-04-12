//! `TabView` — a tabbed container with a clickable tab bar.
//!
//! Only the active tab's content widget is placed in `self.children` (so the
//! framework's `paint_subtree` and `hit_test_subtree` only see it). When the
//! active tab changes, the widgets are swapped between `self.children` and the
//! internal `tab_contents` storage using `std::mem::replace`.
//!
//! # Y-up layout
//!
//! The tab bar sits at the **top** of the widget (high Y values in Y-up).
//! The content area occupies the space below it.

use std::sync::Arc;

use crate::color::Color;
use crate::event::{Event, EventResult, MouseButton};
use crate::geometry::{Point, Rect, Size};
use crate::gfx_ctx::GfxCtx;
use crate::text::Font;
use crate::widget::Widget;
use crate::widgets::primitives::Spacer;

/// Tab bar position.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TabBarPosition {
    Top,
}

/// A tabbed panel container.
pub struct TabView {
    bounds: Rect,
    /// The active tab's content widget. At most 1 element.
    children: Vec<Box<dyn Widget>>,
    /// Storage for all tab widgets. The active tab's slot holds a Spacer
    /// placeholder while its widget lives in `self.children`.
    tab_contents: Vec<Box<dyn Widget>>,
    tab_labels: Vec<String>,
    active_tab: usize,
    tab_bar_height: f64,
    font: Arc<Font>,
    font_size: f64,
    hovered_tab: Option<usize>,
}

impl TabView {
    pub fn new(font: Arc<Font>) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            tab_contents: Vec::new(),
            tab_labels: Vec::new(),
            active_tab: 0,
            tab_bar_height: 36.0,
            font,
            font_size: 13.0,
            hovered_tab: None,
        }
    }

    pub fn with_tab_bar_height(mut self, h: f64) -> Self {
        self.tab_bar_height = h;
        self
    }

    pub fn with_font_size(mut self, size: f64) -> Self {
        self.font_size = size;
        self
    }

    /// Add a tab with a label and its content widget.
    pub fn add_tab(mut self, label: impl Into<String>, content: Box<dyn Widget>) -> Self {
        let idx = self.tab_labels.len();
        self.tab_labels.push(label.into());

        if idx == 0 {
            // First tab goes straight into children (active by default).
            self.children.push(content);
            self.tab_contents.push(Box::new(Spacer::new())); // placeholder
        } else {
            self.tab_contents.push(content);
        }
        self
    }

    fn switch_to(&mut self, new_idx: usize) {
        if new_idx == self.active_tab || new_idx >= self.tab_labels.len() {
            return;
        }

        // Move current active child back to storage slot.
        if let Some(current) = self.children.pop() {
            self.tab_contents[self.active_tab] = current;
        }

        // Move new tab's child into active slot.
        let placeholder: Box<dyn Widget> = Box::new(Spacer::new());
        let new_child = std::mem::replace(&mut self.tab_contents[new_idx], placeholder);
        self.children.push(new_child);

        self.active_tab = new_idx;
    }

    fn content_height(&self) -> f64 {
        (self.bounds.height - self.tab_bar_height).max(0.0)
    }

    fn tab_index_at(&self, pos: Point) -> Option<usize> {
        // Tab bar is at the top: y >= content_height
        if pos.y < self.content_height() { return None; }
        let n = self.tab_labels.len().max(1);
        let tab_w = self.bounds.width / n as f64;
        let i = (pos.x / tab_w) as usize;
        if i < self.tab_labels.len() { Some(i) } else { None }
    }
}

impl Widget for TabView {
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, b: Rect) { self.bounds = b; }
    fn children(&self) -> &[Box<dyn Widget>] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> { &mut self.children }

    fn layout(&mut self, available: Size) -> Size {
        let content_h = (available.height - self.tab_bar_height).max(0.0);
        let content_area = Size::new(available.width, content_h);

        if let Some(child) = self.children.first_mut() {
            child.layout(content_area);
            // Content occupies [0 .. content_h] in Y-up; tab bar is above it.
            child.set_bounds(Rect::new(0.0, 0.0, available.width, content_h));
        }

        available
    }

    fn paint(&mut self, ctx: &mut GfxCtx) {
        let w = self.bounds.width;
        let h = self.bounds.height;
        let tab_h = self.tab_bar_height;
        let content_h = self.content_height();
        let n = self.tab_labels.len().max(1);
        let tab_w = w / n as f64;
        let bar_y = content_h; // bottom of tab bar in Y-up

        // Tab bar background
        ctx.set_fill_color(Color::rgb(0.97, 0.97, 0.98));
        ctx.begin_path();
        ctx.rect(0.0, bar_y, w, tab_h);
        ctx.fill();

        // Tab bar top separator line
        ctx.set_stroke_color(Color::rgba(0.0, 0.0, 0.0, 0.12));
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.move_to(0.0, bar_y);
        ctx.line_to(w, bar_y);
        ctx.stroke();

        // Individual tab labels
        ctx.set_font(Arc::clone(&self.font));
        ctx.set_font_size(self.font_size);

        for (i, label) in self.tab_labels.iter().enumerate() {
            let tx = i as f64 * tab_w;
            let is_active = i == self.active_tab;
            let is_hovered = self.hovered_tab == Some(i);

            // Hover background
            if is_hovered && !is_active {
                ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.04));
                ctx.begin_path();
                ctx.rect(tx, bar_y, tab_w, tab_h);
                ctx.fill();
            }

            // Active indicator bar at the very top of the tab bar
            if is_active {
                ctx.set_fill_color(Color::rgb(0.22, 0.45, 0.88));
                ctx.begin_path();
                ctx.rect(tx, h - 2.5, tab_w, 2.5);
                ctx.fill();
            }

            // Label
            let label_color = if is_active {
                Color::rgb(0.22, 0.45, 0.88)
            } else if is_hovered {
                Color::rgb(0.3, 0.3, 0.35)
            } else {
                Color::rgba(0.0, 0.0, 0.0, 0.55)
            };
            ctx.set_fill_color(label_color);
            if let Some(m) = ctx.measure_text(label) {
                let lx = tx + (tab_w - m.width) * 0.5;
                let ly = bar_y + (tab_h - (m.ascent + m.descent)) * 0.5 + m.descent;
                ctx.fill_text(label, lx, ly);
            }
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { pos } => {
                self.hovered_tab = self.tab_index_at(*pos);
                EventResult::Ignored
            }
            Event::MouseDown { pos, button: MouseButton::Left, .. } => {
                if let Some(i) = self.tab_index_at(*pos) {
                    self.switch_to(i);
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }
}
