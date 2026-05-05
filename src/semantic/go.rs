use crate::semantic::common::{push_parsed_node, ParsedNode};
use tree_sitter::{Node, Parser};

pub(crate) fn parse_go_ast(source_code: &str) -> Option<Vec<ParsedNode>> {
    let lang = tree_sitter_go::LANGUAGE.into();

    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .expect("Failed to load Go grammar");

    let tree = parser.parse(source_code, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    let mut nodes = Vec::new();
    visit_go_node(root, source_code, &mut nodes);
    Some(nodes)
}

fn visit_go_node(node: Node, source_code: &str, nodes: &mut Vec<ParsedNode>) {
    match node.kind() {
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source_code.as_bytes()).unwrap_or("");
                if !name.is_empty() {
                    push_parsed_node(
                        nodes,
                        name.to_string(),
                        "free_function",
                        node.child_by_field_name("parameters"),
                        node.child_by_field_name("body"),
                        source_code,
                    );
                }
            }
        }
        "method_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source_code.as_bytes()).unwrap_or("");
                let receiver = node
                    .child_by_field_name("receiver")
                    .and_then(|receiver_node| normalized_go_receiver(receiver_node, source_code));
                if !name.is_empty() {
                    let identity = receiver
                        .map(|receiver| format!("{receiver}::{name}"))
                        .unwrap_or_else(|| name.to_string());
                    push_parsed_node(
                        nodes,
                        identity,
                        "impl_method",
                        node.child_by_field_name("parameters"),
                        node.child_by_field_name("body"),
                        source_code,
                    );
                }
            }
        }
        "type_spec" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source_code.as_bytes()).unwrap_or("");
                if !name.is_empty() {
                    let type_node = node.child_by_field_name("type");
                    let node_type = match type_node.map(|n| n.kind()) {
                        Some("struct_type") | Some("interface_type") => "struct_def",
                        _ => "type_alias",
                    };
                    push_parsed_node(
                        nodes,
                        name.to_string(),
                        node_type,
                        type_node,
                        type_node,
                        source_code,
                    );
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_go_node(child, source_code, nodes);
    }
}

fn normalized_go_receiver(node: Node, source_code: &str) -> Option<String> {
    let text = node.utf8_text(source_code.as_bytes()).ok()?;
    let receiver = text
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split_whitespace()
        .last()?
        .trim_start_matches('*')
        .trim_start_matches('&')
        .split('[')
        .next()?;
    if receiver.is_empty() {
        None
    } else {
        Some(receiver.to_string())
    }
}
