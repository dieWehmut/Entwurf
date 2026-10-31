//! Advisory GPT composition. Validate complete coverage, then consume only
//! contiguous hints compatible with the immutable canonical node sequence.
use crate::{
    error::Result,
    ir::{KnowledgeIr, StyleSpec, VisualPlan},
    layout::{build_groups, pack_groups, Group, LayoutOptions},
    model::LayoutDocument,
    project::{ImportWarning, VisualPlanStatus},
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct PlannedLayout {
    pub document: Result<LayoutDocument>,
    pub warnings: Vec<ImportWarning>,
    pub status: Option<VisualPlanStatus>,
}
fn warning(warnings: &mut Vec<ImportWarning>, code: &str, message: impl Into<String>) {
    warnings.push(ImportWarning {
        code: code.into(),
        message: message.into(),
    });
}

fn finish(
    mut document: Result<LayoutDocument>,
    warnings: Vec<ImportWarning>,
    status: Option<VisualPlanStatus>,
) -> PlannedLayout {
    if let Ok(doc) = &mut document {
        doc.diagnostics.extend(
            warnings
                .iter()
                .map(|w| format!("{}: {}", w.code, w.message)),
        );
    }
    PlannedLayout {
        document,
        warnings,
        status,
    }
}

pub(crate) fn layout(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    options: &LayoutOptions,
    plan: Option<&VisualPlan>,
) -> PlannedLayout {
    let baseline = || pack_groups(ir, style, options, build_groups(ir));
    let Some(plan) = plan else {
        return PlannedLayout {
            document: baseline(),
            warnings: vec![],
            status: None,
        };
    };
    let mut warnings = Vec::new();
    let positions: BTreeMap<_, _> = ir
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut covered = BTreeSet::new();
    let mut group_ids = BTreeSet::new();
    if plan.page_count == 0 || plan.page_count > options.max_pages.min(3) {
        warning(
            &mut warnings,
            "visual_plan_invalid",
            format!(
                "Requested {} pages exceeds the allowed range 1..={}; using measured layout.",
                plan.page_count,
                options.max_pages.min(3)
            ),
        );
    }
    for group in &plan.groups {
        if group.id.trim().is_empty() || !group_ids.insert(group.id.as_str()) {
            warning(
                &mut warnings,
                "visual_plan_invalid",
                format!(
                    "Proposal has an empty or duplicate group id {:?}.",
                    group.id
                ),
            );
        }
        if group.node_ids.is_empty() || group.page == 0 || group.page > plan.page_count {
            warning(
                &mut warnings,
                "visual_plan_invalid",
                format!(
                    "Group {:?} is empty or requests page {} outside 1..={}.",
                    group.id, group.page, plan.page_count
                ),
            );
        }
        for id in &group.node_ids {
            if !positions.contains_key(id.as_str()) || !covered.insert(id.as_str()) {
                warning(
                    &mut warnings,
                    "visual_plan_invalid",
                    format!(
                        "Group {:?} contains unknown or repeated node {:?}.",
                        group.id, id
                    ),
                );
            }
        }
    }
    for node in &ir.nodes {
        if !covered.contains(node.id.as_str()) {
            warning(
                &mut warnings,
                "visual_plan_invalid",
                format!(
                    "Proposal omits node {:?}; complete coverage is required.",
                    node.id
                ),
            );
        }
    }
    if !warnings.is_empty() {
        return finish(baseline(), warnings, Some(VisualPlanStatus::Ignored));
    }
    // Use baseline fragments for incompatible groups; each compatible group
    // replaces only its contiguous interval, never the order of its nodes.
    let mut assignments = BTreeMap::new();
    let mut eligible = Vec::new();
    for (index, group) in plan.groups.iter().enumerate() {
        let order: Vec<_> = group
            .node_ids
            .iter()
            .map(|id| positions[id.as_str()])
            .collect();
        if !order.windows(2).all(|pair| pair[1] == pair[0] + 1) {
            warning(&mut warnings, "visual_plan_order_protected", format!("Group {:?} grouping and page hint were ignored because its node sequence conflicts with canonical canvas order.", group.id));
            continue;
        }
        for id in &group.node_ids {
            assignments.insert(id.as_str(), index);
        }
        eligible.push((order[0], index));
    }
    let proposed_order: Vec<_> = eligible.iter().map(|(_, i)| *i).collect();
    eligible.sort_unstable();
    for (rank, (_, index)) in eligible.iter().enumerate() {
        if proposed_order[rank] != *index {
            warning(
                &mut warnings,
                "visual_plan_order_protected",
                format!(
                    "Group {:?} ordering hint was ignored to preserve canonical canvas order.",
                    plan.groups[*index].id
                ),
            );
        }
    }
    let applied_count = eligible.len();
    let base = build_groups(ir);
    let mut base_for_node = BTreeMap::new();
    for (i, group) in base.iter().enumerate() {
        for id in &group.node_ids {
            base_for_node.insert(id.as_str(), i);
        }
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut previous_key = None;
    for node in &ir.nodes {
        let key = assignments
            .get(node.id.as_str())
            .copied()
            .map(|i| (true, i))
            .unwrap_or((false, base_for_node[node.id.as_str()]));
        if previous_key == Some(key) {
            groups.last_mut().unwrap().node_ids.push(node.id.clone());
        } else {
            let (id, title, page_hint) = if key.0 {
                let proposal = &plan.groups[key.1];
                (
                    format!("proposal:{}", proposal.id),
                    proposal.title.clone(),
                    Some(proposal.page),
                )
            } else {
                let fallback = &base[key.1];
                (fallback.id.clone(), fallback.title.clone(), None)
            };
            groups.push(Group {
                id: format!("{id}:run{}", groups.len()),
                title,
                node_ids: vec![node.id.clone()],
                page_hint,
            });
            previous_key = Some(key);
        }
    }
    let mut document = match pack_groups(ir, style, options, groups) {
        Ok(doc) => Ok(doc),
        Err(error) => {
            warning(&mut warnings, "visual_plan_measured_fallback", format!("Proposed grouping/page allocation could not be composed: {error}. Retrying the canonical measured layout without proposal hints."));
            return finish(baseline(), warnings, Some(VisualPlanStatus::Ignored));
        }
    };
    if let Ok(doc) = &mut document {
        let actual: BTreeMap<_, _> = doc
            .all_nodes()
            .map(|n| (n.node_id.as_str(), n.page))
            .collect();
        for (_, index) in &eligible {
            let group = &plan.groups[*index];
            let pages: BTreeSet<_> = group
                .node_ids
                .iter()
                .map(|id| actual[id.as_str()])
                .collect();
            if pages.len() != 1 || !pages.contains(&group.page) {
                warning(&mut warnings, "visual_plan_page_fallback", format!("Group {:?} requested page {}, but measured packing preserving node order uses pages {:?}.", group.id, group.page, pages));
            }
        }
        if doc.pages.len() != plan.page_count as usize {
            warning(&mut warnings, "visual_plan_page_count_fallback", format!("Proposal requested {} pages; measured layout uses {}. Empty pages are not inserted and readable content is preserved.", plan.page_count, doc.pages.len()));
        }
    }
    let status = if applied_count == 0 {
        VisualPlanStatus::Ignored
    } else if warnings.is_empty() {
        VisualPlanStatus::Applied
    } else {
        VisualPlanStatus::PartiallyApplied
    };
    finish(document, warnings, Some(status))
}
