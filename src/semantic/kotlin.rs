use crate::semantic::common::{push_parsed_node, ParsedNode};
use tree_sitter::{Node, Parser};

pub(crate) fn parse_kotlin_ast(source_code: &str) -> Option<Vec<ParsedNode>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(source_code, None)?;
    let mut nodes = Vec::new();
    visit_kotlin_node(tree.root_node(), source_code, None, &mut nodes);
    Some(nodes)
}

fn visit_kotlin_node(node: Node, source: &str, scope: Option<String>, nodes: &mut Vec<ParsedNode>) {
    match node.kind() {
        "class_declaration" | "object_declaration" => {
            if let Some(name_node) = node.child_by_field_name("identifier") {
                let name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let id = scope
                        .clone()
                        .map(|s| format!("{}.{}", s, name))
                        .unwrap_or_else(|| name.clone());
                    push_parsed_node(
                        nodes,
                        id.clone(),
                        "struct_def",
                        None,
                        node.child_by_field_name("body"),
                        source,
                    );
                    if let Some(body) = node.child_by_field_name("body") {
                        let mut cursor = body.walk();
                        for child in body.named_children(&mut cursor) {
                            visit_kotlin_node(child, source, Some(id.clone()), nodes);
                        }
                    }
                    return;
                }
            }
        }
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("identifier") {
                let name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let id = scope
                        .clone()
                        .map(|s| format!("{}.{}", s, name))
                        .unwrap_or_else(|| name.clone());
                    push_parsed_node(
                        nodes,
                        id,
                        if scope.is_some() {
                            "impl_method"
                        } else {
                            "free_function"
                        },
                        node.child_by_field_name("value_parameters"),
                        node.child_by_field_name("body"),
                        source,
                    );
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_kotlin_node(child, source, scope.clone(), nodes);
    }
}
