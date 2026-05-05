use crate::semantic::ParsedNode;
use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum Delta {
    Added {
        identity: String,
        node_type: String,
    },
    Modified {
        identity: String,
        node_type: String,
        signature_changed: bool,
        body_changed: bool,
    },
    Removed {
        identity: String,
        node_type: String,
    },
}

pub fn diff_snapshots(before: Vec<ParsedNode>, after: Vec<ParsedNode>) -> Vec<Delta> {
    let before_map: HashMap<String, ParsedNode> = before
        .into_iter()
        .map(|n| (n.identity_key.clone(), n))
        .collect();
    let after_map: HashMap<String, ParsedNode> = after
        .into_iter()
        .map(|n| (n.identity_key.clone(), n))
        .collect();

    let mut deltas = Vec::new();

    for (key, new_node) in &after_map {
        match before_map.get(key) {
            None => deltas.push(Delta::Added {
                identity: key.clone(),
                node_type: new_node.node_type.clone(),
            }),
            Some(old) => {
                let sig = old.signature_hash != new_node.signature_hash;
                let body = old.shallow_body_hash != new_node.shallow_body_hash;
                if sig || body {
                    deltas.push(Delta::Modified {
                        identity: key.clone(),
                        node_type: new_node.node_type.clone(),
                        signature_changed: sig,
                        body_changed: body,
                    });
                }
            }
        }
    }

    for (key, old_node) in &before_map {
        if !after_map.contains_key(key) {
            deltas.push(Delta::Removed {
                identity: key.clone(),
                node_type: old_node.node_type.clone(),
            });
        }
    }

    // Added → Modified → Removed, alphabetical within each group
    deltas.sort_by(|a, b| {
        let rank = |d: &Delta| match d {
            Delta::Added { .. } => 0,
            Delta::Modified { .. } => 1,
            Delta::Removed { .. } => 2,
        };

        // ZERO-COPY: Extract references directly to satisfy the borrow checker
        let key_a = match a {
            Delta::Added { identity, .. } => identity,
            Delta::Modified { identity, .. } => identity,
            Delta::Removed { identity, .. } => identity,
        };
        let key_b = match b {
            Delta::Added { identity, .. } => identity,
            Delta::Modified { identity, .. } => identity,
            Delta::Removed { identity, .. } => identity,
        };

        rank(a).cmp(&rank(b)).then_with(|| key_a.cmp(key_b))
    });

    deltas
}

fn format_delta(d: &Delta) -> String {
    match d {
        Delta::Added {
            identity,
            node_type,
        } => match node_type.as_str() {
            "free_function" => format!("Added function `{}`.", identity),
            "impl_method" => format!("Added method `{}`.", identity),
            "struct_def" => format!("Added struct `{}`.", identity),
            "enum_def" => format!("Added enum `{}`.", identity),
            "type_alias" => format!("Added type alias `{}`.", identity),
            _ => format!("Added `{}` [{}].", identity, node_type),
        },
        Delta::Modified {
            identity,
            node_type,
            signature_changed,
            body_changed,
        } => {
            let scope = match node_type.as_str() {
                "struct_def" => "fields",
                "enum_def" => "variants",
                _ => "body",
            };
            match (signature_changed, body_changed) {
                (true, true) => format!(
                    "Modified `{}`: signature and {} both changed.",
                    identity, scope
                ),
                (true, false) => format!(
                    "Modified signature of `{}` ({} unchanged). Check callers.",
                    identity, scope
                ),
                (false, true) => format!(
                    "Modified {} of `{}` (signature unchanged).",
                    scope, identity
                ),
                (false, false) => unreachable!(),
            }
        }
        Delta::Removed {
            identity,
            node_type,
        } => match node_type.as_str() {
            "free_function" => format!("Removed function `{}`. Verify call sites.", identity),
            "impl_method" => format!("Removed method `{}`. Check usages.", identity),
            "struct_def" => format!("Removed struct `{}`. Check instantiations.", identity),
            "enum_def" => format!("Removed enum `{}`. Check match arms.", identity),
            "type_alias" => format!("Removed type alias `{}`.", identity),
            _ => format!("Removed `{}` [{}].", identity, node_type),
        },
    }
}

pub fn verbalize(deltas: &[Delta]) -> String {
    if deltas.is_empty() {
        return "No structural changes detected.".to_string();
    }

    let mut lines: Vec<String> = Vec::new();
    let mut total_len = 0;
    let mut skipped = 0;
    let budget = 800;

    for (i, d) in deltas.iter().enumerate() {
        let line = format_delta(d);
        // Clean truncation: stop immediately if we breach the budget
        if total_len + line.len() > budget {
            skipped = deltas.len() - i;
            break;
        }
        total_len += line.len() + 1; // +1 accounts for the \n joiner
        lines.push(line);
    }

    if skipped > 0 {
        lines.push(format!(
            "(+{} more changes omitted to save context)",
            skipped
        ));
    }

    lines.join("\n")
}
