use crate::semantic::common::{hash_node, hash_shallow_children, ParsedNode};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

pub fn parse_standard_rust_ast(source_code: &str, query_str: &str) -> Option<Vec<ParsedNode>> {
    let lang = tree_sitter_rust::LANGUAGE.into();

    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .expect("Failed to load Rust grammar");

    let tree = parser.parse(source_code, None)?;
    let root = tree.root_node();

    if root.has_error() {
        return None;
    }

    let query = Query::new(&lang, query_str).expect("Invalid tree-sitter query");

    let mut cursor = QueryCursor::new();
    let mut nodes = Vec::new();

    let mut matches = cursor.matches(&query, root, source_code.as_bytes());
    while let Some(qmatch) = matches.next() {
        let mut func_name: Option<&str> = None;
        let mut struct_name: Option<&str> = None;
        let mut enum_name: Option<&str> = None;
        let mut impl_target: Option<&str> = None;
        let mut method_name: Option<&str> = None;
        let mut type_alias: Option<&str> = None;
        let mut params_node: Option<Node> = None;
        let mut body_node: Option<Node> = None;

        for cap in qmatch.captures {
            let name = query.capture_names()[cap.index as usize];
            let text = cap.node.utf8_text(source_code.as_bytes()).unwrap_or("");

            match name {
                "func_name" => func_name = Some(text),
                "func_params" => params_node = Some(cap.node),
                "func_body" => body_node = Some(cap.node),
                "struct_name" => struct_name = Some(text),
                "struct_fields" => body_node = Some(cap.node),
                "enum_name" => enum_name = Some(text),
                "enum_variants" => body_node = Some(cap.node),
                "impl_target" => impl_target = Some(text),
                "method_name" => method_name = Some(text),
                "method_params" => params_node = Some(cap.node),
                "method_body" => body_node = Some(cap.node),
                "type_alias_name" => type_alias = Some(text),
                _ => {}
            }
        }

        let (identity_key, node_type) = if let Some(n) = func_name {
            (n.to_string(), "free_function".to_string())
        } else if let Some(n) = struct_name {
            (n.to_string(), "struct_def".to_string())
        } else if let Some(n) = enum_name {
            (n.to_string(), "enum_def".to_string())
        } else if let (Some(tgt), Some(meth)) = (impl_target, method_name) {
            (format!("{}::{}", tgt, meth), "impl_method".to_string())
        } else if let Some(n) = type_alias {
            (n.to_string(), "type_alias".to_string())
        } else {
            continue;
        };

        nodes.push(ParsedNode {
            identity_key,
            node_type,
            signature_hash: hash_node(params_node, source_code),
            shallow_body_hash: hash_shallow_children(body_node, source_code),
        });
    }

    Some(nodes)
}
