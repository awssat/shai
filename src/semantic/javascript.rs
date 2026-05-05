use crate::semantic::common::{push_parsed_node, ParsedNode};
use tree_sitter::{Node, Parser};

pub(crate) fn parse_javascript_ast(source_code: &str) -> Option<Vec<ParsedNode>> {
    let lang = tree_sitter_javascript::LANGUAGE.into();

    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .expect("Failed to load JavaScript grammar");

    let tree = parser.parse(source_code, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    let mut nodes = Vec::new();
    visit_javascript_node(root, source_code, None, &mut nodes);
    Some(nodes)
}

fn visit_javascript_node(
    node: Node,
    source_code: &str,
    scope: Option<&str>,
    nodes: &mut Vec<ParsedNode>,
) {
    match node.kind() {
        "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let class_name = name_node.utf8_text(source_code.as_bytes()).unwrap_or("");
                if !class_name.is_empty() {
                    push_parsed_node(
                        nodes,
                        class_name.to_string(),
                        "struct_def",
                        None,
                        node.child_by_field_name("body"),
                        source_code,
                    );
                    if let Some(body) = node.child_by_field_name("body") {
                        let mut cursor = body.walk();
                        for child in body.named_children(&mut cursor) {
                            visit_javascript_node(child, source_code, Some(class_name), nodes);
                        }
                    }
                    return;
                }
            }
        }
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
        "method_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source_code.as_bytes()).unwrap_or("");
                if !name.is_empty() {
                    let identity = scope
                        .map(|owner| format!("{owner}::{name}"))
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
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_javascript_node(child, source_code, scope, nodes);
    }
}
