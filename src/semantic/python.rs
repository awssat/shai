use crate::semantic::common::{push_parsed_node, ParsedNode};
use tree_sitter::{Node, Parser};

pub(crate) fn parse_python_ast(source_code: &str) -> Option<Vec<ParsedNode>> {
    let lang = tree_sitter_python::LANGUAGE.into();

    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .expect("Failed to load Python grammar");

    let tree = parser.parse(source_code, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    let mut nodes = Vec::new();
    visit_python_node(root, source_code, None, &mut nodes);
    Some(nodes)
}

fn visit_python_node(
    node: Node,
    source_code: &str,
    scope: Option<&str>,
    nodes: &mut Vec<ParsedNode>,
) {
    match node.kind() {
        "class_definition" => {
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
                            visit_python_node(child, source_code, Some(class_name), nodes);
                        }
                    }
                    return;
                }
            }
        }
        "function_definition" | "async_function_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source_code.as_bytes()).unwrap_or("");
                if !name.is_empty() {
                    let identity = scope
                        .map(|class_name| format!("{class_name}::{name}"))
                        .unwrap_or_else(|| name.to_string());
                    let node_type = if scope.is_some() {
                        "impl_method"
                    } else {
                        "free_function"
                    };
                    push_parsed_node(
                        nodes,
                        identity,
                        node_type,
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
        visit_python_node(child, source_code, scope, nodes);
    }
}
