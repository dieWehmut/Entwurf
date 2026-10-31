//! Shared geometry and artifact model used by layout, validation and export.

// Wire declarations are generated from the canonical JSON Schemas.
include!("../../../packages/contracts/generated/model.rs");

use serde::{Deserialize, Serialize};

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
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
