//! External KnowledgeIR has no filesystem authority. Bind source evidence to
//! retained project originals before invoking any provenance/content validator.
use crate::{
    error::{EntwurfError, Result},
    ir::*,
    project::Project,
    storage::ProjectPaths,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
};

pub const MAX_IR_BYTES: usize = 16 * 1024 * 1024;
fn invalid(message: impl Into<String>) -> EntwurfError {
    EntwurfError::InvalidInput(message.into())
}

fn unique<'a>(ids: impl Iterator<Item = &'a str>, what: &str) -> Result<BTreeSet<&'a str>> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if id.trim().is_empty() || !seen.insert(id) {
            return Err(invalid(format!("duplicate or empty {what} identity")));
        }
    }
    Ok(seen)
}
fn references(ir: &KnowledgeIr) -> Result<()> {
    let sources = unique(ir.sources.iter().map(|s| s.id.as_str()), "source")?;
    let nodes = unique(ir.nodes.iter().map(|n| n.id.as_str()), "node")?;
    unique(ir.edges.iter().map(|e| e.id.as_str()), "edge")?;
    unique(ir.corrections.iter().map(|c| c.id.as_str()), "correction")?;
    unique(ir.unverified.iter().map(|u| u.id.as_str()), "review item")?;
    let anchors = |items: &[SourceAnchor]| -> Result<()> {
        if items
            .iter()
            .any(|a| !sources.contains(a.source_id.as_str()))
        {
            return Err(invalid("anchor references an unknown source"));
        }
        Ok(())
    };
    let parents: BTreeMap<_, _> = ir
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut checked = BTreeSet::new();
    for node in &ir.nodes {
        anchors(&node.anchors)?;
        let mut cursor = Some(node.id.as_str());
        let mut seen = BTreeSet::new();
        while let Some(id) = cursor {
            if checked.contains(id) {
                break;
            }
            if !seen.insert(id) {
                return Err(invalid("node parent references contain a cycle"));
            }
            cursor = *parents
                .get(id)
                .ok_or_else(|| invalid("node parent references an unknown node"))?;
        }
        checked.extend(seen);
    }
    for edge in &ir.edges {
        if !nodes.contains(edge.from.as_str()) || !nodes.contains(edge.to.as_str()) {
            return Err(invalid("edge references an unknown node"));
        }
        anchors(&edge.anchors)?;
    }
    for correction in &ir.corrections {
        if !nodes.contains(correction.node_id.as_str()) {
            return Err(invalid("correction references an unknown node"));
        }
        anchors(&correction.anchors)?;
        if correction.field == CorrectionField::Anchors {
            for value in [&correction.before, &correction.after] {
                let evidence: Vec<SourceAnchor> = serde_json::from_str(value)
                    .map_err(|_| invalid("anchor correction contains invalid evidence"))?;
                anchors(&evidence)?;
            }
        }
    }
    if ir
        .unverified
        .iter()
        .any(|u| u.node_id.as_deref().is_some_and(|id| !nodes.contains(id)))
    {
        return Err(invalid("review item references an unknown node"));
    }
    Ok(())
}

pub fn parse_and_bind(paths: &ProjectPaths, project: &Project, json: &str) -> Result<KnowledgeIr> {
    if json.len() > MAX_IR_BYTES {
        return Err(invalid("Knowledge IR exceeds 16 MiB"));
    }
    let value = serde_json::from_str(json)
        .map_err(|e| invalid(format!("invalid Knowledge IR JSON: {e}")))?;
    crate::contracts::validate_json("knowledge-ir.schema.json", &value)?;
    let mut ir: KnowledgeIr = serde_json::from_value(value).map_err(|e| invalid(e.to_string()))?;
    references(&ir)?;
    let root = fs::canonicalize(&paths.root).map_err(|e| EntwurfError::read(&paths.root, e))?;
    let source_root = root.join("sources");
    // Reject even a project sources-directory symlink before any file reads.
    if fs::canonicalize(&source_root).map_err(|e| EntwurfError::read(&source_root, e))?
        != source_root
    {
        return Err(invalid(
            "retained source directory is outside project-owned storage",
        ));
    }
    for source in &mut ir.sources {
        // The incoming path and transcript are never read or used as evidence.
        let candidates: Vec<_> = project
            .tasks
            .iter()
            .flat_map(|t| t.versions.iter().rev())
            .flat_map(|v| &v.ir.sources)
            .filter(|s| {
                s.content_hash == source.content_hash
                    && s.kind == source.kind
                    && s.byte_len == source.byte_len
            })
            .collect();
        // Notes only select evidence that already exists in trusted history.
        // This preserves an exported successful extraction even when another
        // task retained a failed parse of the same bytes. Unmatched input notes
        // are discarded; the fallback is still actual retained evidence.
        let trusted = candidates
            .iter()
            .copied()
            .find(|s| s.notes == source.notes)
            .or_else(|| candidates.first().copied())
            .ok_or_else(|| {
                invalid(format!(
                    "No retained source matches {}; import the original sources first",
                    source.original_name
                ))
            })?;
        if trusted.content_hash.len() != 64
            || !trusted.content_hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("invalid retained source hash"));
        }
        let expected = source_root.join(&trusted.content_hash);
        let resolved = fs::canonicalize(&trusted.stored_path)
            .map_err(|e| EntwurfError::read(&trusted.stored_path, e))?;
        if resolved != expected {
            return Err(invalid(
                "retained source is outside project-owned content-addressed storage",
            ));
        }
        let mut file = fs::File::open(&resolved).map_err(|e| EntwurfError::read(&resolved, e))?;
        let metadata = file
            .metadata()
            .map_err(|e| EntwurfError::read(&resolved, e))?;
        if !metadata.is_file() || metadata.len() != trusted.byte_len {
            return Err(invalid(
                "retained source size differs from its imported identity",
            ));
        }
        let mut hash = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            let read = file
                .read(&mut buf)
                .map_err(|e| EntwurfError::read(&resolved, e))?;
            if read == 0 {
                break;
            }
            hash.update(&buf[..read]);
        }
        if format!("{:x}", hash.finalize()) != trusted.content_hash {
            return Err(invalid(
                "retained source SHA-256 differs from its imported identity",
            ));
        }
        let id = source.id.clone();
        *source = trusted.clone();
        // Imported anchors/correction before-after payloads remain exact because
        // evidence is rebound under the imported source identity.
        source.id = id;
        source.stored_path = resolved.to_string_lossy().into_owned();
    }
    Ok(ir)
}
