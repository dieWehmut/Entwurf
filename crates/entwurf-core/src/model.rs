//! Shared geometry and artifact model used by layout, validation and export.

use serde::{Deserialize, Serialize};

use crate::ir::{Id, NodeKind};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.right()
            && point.y >= self.y
            && point.y <= self.bottom()
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        let epsilon = 0.01;
        self.x + epsilon < other.right()
            && self.right() - epsilon > other.x
            && self.y + epsilon < other.bottom()
            && self.bottom() - epsilon > other.y
    }

    pub fn inset(&self, amount: f32) -> Rect {
        Rect::new(
            self.x + amount,
            self.y + amount,
            (self.width - amount * 2.0).max(0.0),
            (self.height - amount * 2.0).max(0.0),
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineShape {
    Orthogonal,
    Straight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextLine {
    pub text: String,
    pub x: f32,
    pub baseline: f32,
    pub font_size: f32,
    pub bold: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedNode {
    pub node_id: Id,
    pub kind: NodeKind,
    pub title: String,
    pub body: String,
    pub latex: Option<String>,
    pub page: u16,
    pub rect: Rect,
    pub title_lines: Vec<TextLine>,
    pub body_lines: Vec<TextLine>,
    pub latex_lines: Vec<TextLine>,
    pub text_clipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedEdge {
    pub edge_id: Id,
    pub from_node: Id,
    pub to_node: Id,
    pub page: u16,
    pub points: Vec<Point>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedGroup {
    pub id: Id,
    pub title: String,
    pub page: u16,
    pub rect: Rect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutPage {
    pub index: u16,
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_pt: f32,
    pub groups: Vec<PlacedGroup>,
    pub nodes: Vec<PlacedNode>,
    pub edges: Vec<PlacedEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutDocument {
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_pt: f32,
    pub pages: Vec<LayoutPage>,
    pub diagnostics: Vec<String>,
}

impl LayoutDocument {
    pub fn node_count(&self) -> usize {
        self.pages.iter().map(|page| page.nodes.len()).sum()
    }

    pub fn all_nodes(&self) -> impl Iterator<Item = &PlacedNode> {
        self.pages.iter().flat_map(|page| page.nodes.iter())
    }

    pub fn all_edges(&self) -> impl Iterator<Item = &PlacedEdge> {
        self.pages.iter().flat_map(|page| page.edges.iter())
    }
}

pub const A3_WIDTH_MM: f32 = 420.0;
pub const A3_HEIGHT_MM: f32 = 297.0;
pub const MM_TO_PT: f32 = 72.0 / 25.4;

pub fn mm_to_pt(mm: f32) -> f32 {
    mm * MM_TO_PT
}

pub fn pt_to_mm(pt: f32) -> f32 {
    pt / MM_TO_PT
}
