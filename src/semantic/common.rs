use tree_sitter::Node;

#[derive(Debug, Clone)]
pub struct ParsedNode {
    pub identity_key: String,
    pub node_type: String,
    pub signature_hash: String,
    pub shallow_body_hash: String,
}

fn hash_str(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

pub(crate) fn hash_node(node: Option<Node>, source: &str) -> String {
    let Some(n) = node else { return String::new() };
    hash_str(n.utf8_text(source.as_bytes()).unwrap_or(""))
}

pub(crate) fn hash_shallow_children(node: Option<Node>, source: &str) -> String {
    let Some(n) = node else { return String::new() };
    let mut cursor = n.walk();
    let mut h = blake3::Hasher::new();
    for child in n.named_children(&mut cursor) {
        h.update(child.utf8_text(source.as_bytes()).unwrap_or("").as_bytes());
    }
    h.finalize().to_hex().to_string()
}

pub(crate) fn push_parsed_node(
    nodes: &mut Vec<ParsedNode>,
    identity_key: String,
    node_type: &str,
    signature_node: Option<Node>,
    body_node: Option<Node>,
    source: &str,
) {
    nodes.push(ParsedNode {
        identity_key,
        node_type: node_type.to_string(),
        signature_hash: hash_node(signature_node, source),
        shallow_body_hash: hash_shallow_children(body_node, source),
    });
}
