//! Deterministic rectilinear shortest paths through a card-edge visibility grid.
use crate::{
    error::{EntwurfError, Result},
    ir::KnowledgeIr,
    model::{LayoutPage, PlacedEdge, Point, Rect},
};
use std::{cmp::Reverse, collections::BinaryHeap};
const EPS: f32 = 0.01;
pub fn segment_hits(a: Point, b: Point, r: Rect) -> bool {
    if (a.y - b.y).abs() < EPS {
        a.y > r.y + EPS
            && a.y < r.bottom() - EPS
            && a.x.max(b.x) > r.x + EPS
            && a.x.min(b.x) < r.right() - EPS
    } else if (a.x - b.x).abs() < EPS {
        a.x > r.x + EPS
            && a.x < r.right() - EPS
            && a.y.max(b.y) > r.y + EPS
            && a.y.min(b.y) < r.bottom() - EPS
    } else {
        true
    }
}
fn inside(p: Point, r: Rect) -> bool {
    p.x > r.x + EPS && p.x < r.right() - EPS && p.y > r.y + EPS && p.y < r.bottom() - EPS
}
fn ports(r: Rect) -> [(Point, Point); 4] {
    let x = r.x + r.width / 2.0;
    let y = r.y + r.height / 2.0;
    [
        (Point::new(r.right(), y), Point::new(r.right() + 2.0, y)),
        (Point::new(r.x, y), Point::new(r.x - 2.0, y)),
        (Point::new(x, r.bottom()), Point::new(x, r.bottom() + 2.0)),
        (Point::new(x, r.y), Point::new(x, r.y - 2.0)),
    ]
}
fn path(page: &LayoutPage, from: Rect, to: Rect) -> Option<Vec<Point>> {
    let bounds = Rect::new(0.5, 0.5, page.width_pt - 1.0, page.height_pt - 1.0);
    let cards: Vec<_> = page.nodes.iter().map(|n| n.rect).collect();
    let obstacles: Vec<_> = cards
        .iter()
        .map(|r| Rect::new(r.x - 2.0, r.y - 2.0, r.width + 4.0, r.height + 4.0))
        .collect();
    let valid_ports = |r: Rect| {
        ports(r)
            .into_iter()
            .filter(|(a, b)| bounds.contains(*b) && !cards.iter().any(|o| segment_hits(*a, *b, *o)))
            .collect::<Vec<_>>()
    };
    let mut starts = valid_ports(from);
    let mut ends = valid_ports(to);
    if starts.is_empty() || ends.is_empty() {
        return None;
    }
    if from == to {
        starts.truncate(1);
        ends.retain(|(p, _)| *p != starts[0].0);
        if ends.is_empty() {
            return None;
        }
    }
    let mut xs = vec![bounds.x, bounds.right()];
    let mut ys = vec![bounds.y, bounds.bottom()];
    for r in &obstacles {
        xs.extend([r.x, r.right()]);
        ys.extend([r.y, r.bottom()]);
    }
    for (_, p) in starts.iter().chain(ends.iter()) {
        xs.push(p.x);
        ys.push(p.y);
    }
    xs.retain(|x| *x >= bounds.x && *x <= bounds.right());
    ys.retain(|y| *y >= bounds.y && *y <= bounds.bottom());
    xs.sort_by(f32::total_cmp);
    ys.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < EPS);
    ys.dedup_by(|a, b| (*a - *b).abs() < EPS);
    let w = xs.len();
    let count = w.checked_mul(ys.len())?;
    if count > 250_000 {
        return None;
    }
    let point = |i: usize| Point::new(xs[i % w], ys[i / w]);
    let index = |p: Point| {
        Some(
            ys.iter().position(|y| (*y - p.y).abs() < EPS)? * w
                + xs.iter().position(|x| (*x - p.x).abs() < EPS)?,
        )
    };
    let blocked: Vec<_> = (0..count)
        .map(|i| obstacles.iter().any(|r| inside(point(i), *r)))
        .collect();
    let mut distances = vec![u64::MAX; count];
    let mut parents = vec![None; count];
    let mut origins = vec![0; count];
    let mut queue = BinaryHeap::new();
    for (which, (_, p)) in starts.iter().enumerate() {
        let i = index(*p)?;
        if !blocked[i] {
            distances[i] = 0;
            origins[i] = which;
            queue.push(Reverse((0u64, i)));
        }
    }
    let targets: Vec<_> = ends
        .iter()
        .enumerate()
        .filter_map(|(which, (_, p))| index(*p).map(|i| (i, which)))
        .collect();
    while let Some(Reverse((cost, i))) = queue.pop() {
        if cost != distances[i] {
            continue;
        }
        if let Some((_, which)) = targets.iter().find(|(j, _)| *j == i) {
            let mut route = vec![ends[*which].0];
            let mut at = i;
            loop {
                route.push(point(at));
                if let Some(parent) = parents[at] {
                    at = parent
                } else {
                    break;
                }
            }
            route.push(starts[origins[i]].0);
            route.reverse();
            let mut simple: Vec<Point> = Vec::new();
            for p in route {
                if simple
                    .last()
                    .is_some_and(|last| (last.x - p.x).abs() < EPS && (last.y - p.y).abs() < EPS)
                {
                    continue;
                }
                while simple.len() >= 2 {
                    let a = simple[simple.len() - 2];
                    let b = simple[simple.len() - 1];
                    if ((a.x - b.x).abs() < EPS && (b.x - p.x).abs() < EPS)
                        || ((a.y - b.y).abs() < EPS && (b.y - p.y).abs() < EPS)
                    {
                        simple.pop();
                    } else {
                        break;
                    }
                }
                simple.push(p);
            }
            return Some(simple);
        }
        let x = i % w;
        let y = i / w;
        let next = [
            (x > 0).then(|| i - 1),
            (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w),
            (y + 1 < ys.len()).then(|| i + w),
        ];
        for j in next.into_iter().flatten() {
            if blocked[j]
                || obstacles
                    .iter()
                    .any(|r| segment_hits(point(i), point(j), *r))
            {
                continue;
            }
            let a = point(i);
            let b = point(j);
            let distance = (((a.x - b.x).abs() + (a.y - b.y).abs()) * 1000.0)
                .round()
                .max(1.0) as u64;
            let new = cost + distance;
            if new < distances[j] {
                distances[j] = new;
                parents[j] = Some(i);
                origins[j] = origins[i];
                queue.push(Reverse((new, j)));
            }
        }
    }
    None
}
pub fn reroute(ir: &KnowledgeIr, page: &LayoutPage) -> Result<Vec<PlacedEdge>> {
    let mut edges = Vec::new();
    for e in &ir.edges {
        let (Some(from), Some(to)) = (
            page.nodes.iter().find(|n| n.node_id == e.from),
            page.nodes.iter().find(|n| n.node_id == e.to),
        ) else {
            continue;
        };
        let points = path(page, from.rect, to.rect).ok_or_else(|| {
            EntwurfError::Render(format!("no obstacle-free route for edge {}", e.id))
        })?;
        edges.push(PlacedEdge {
            edge_id: e.id.clone(),
            from_node: e.from.clone(),
            to_node: e.to.clone(),
            page: page.index,
            points,
            label: e.label.clone(),
        });
    }
    Ok(edges)
}
