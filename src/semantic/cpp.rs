use crate::semantic::common::{push_parsed_node, ParsedNode};
use tree_sitter::{Node, Parser};

pub(crate) fn parse_cpp_ast(source_code: &str) -> Option<Vec<ParsedNode>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cpp::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(source_code, None)?;
    let mut nodes = Vec::new();
    visit_cpp_node(tree.root_node(), source_code, None, &mut nodes);
    Some(nodes)
}

fn visit_cpp_node(node: Node, source: &str, scope: Option<String>, nodes: &mut Vec<ParsedNode>) {
    match node.kind() {
        "function_definition" => {
            if let Some(declarator) = node.child_by_field_name("declarator") {
                let name = extract_cpp_identifier(declarator, source);
                if !name.is_empty() {
                    let id = scope
                        .clone()
                        .map(|s| format!("{}::{}", s, name))
                        .unwrap_or_else(|| name.clone());
                    push_parsed_node(
                        nodes,
                        id,
                        if scope.is_some() {
                            "impl_method"
                        } else {
                            "free_function"
                        },
                        None,
                        node.child_by_field_name("body"),
                        source,
                    );
                }
            }
        }
        "class_specifier" | "struct_specifier" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let id = scope
                        .clone()
                        .map(|s| format!("{}::{}", s, name))
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
                            visit_cpp_node(child, source, Some(id.clone()), nodes);
                        }
                    }
                    return;
                }
            }
        }
        "namespace_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let id = scope
                        .clone()
                        .map(|s| format!("{}::{}", s, name))
                        .unwrap_or_else(|| name.clone());
                    if let Some(body) = node.child_by_field_name("body") {
                        let mut cursor = body.walk();
                        for child in body.named_children(&mut cursor) {
                            visit_cpp_node(child, source, Some(id.clone()), nodes);
                        }
                    }
                    return;
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_cpp_node(child, source, scope.clone(), nodes);
    }
}

fn extract_cpp_identifier(node: Node, source: &str) -> String {
    if node.kind() == "identifier"
        || node.kind() == "field_identifier"
        || node.kind() == "type_identifier"
    {
        return node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
    }
    if node.kind() == "function_declarator" {
        if let Some(declarator) = node.child_by_field_name("declarator") {
            return extract_cpp_identifier(declarator, source);
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let res = extract_cpp_identifier(child, source);
        if !res.is_empty() {
            return res;
        }
    }
    String::new()
}
