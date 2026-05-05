use crate::semantic::common::{push_parsed_node, ParsedNode};
use tree_sitter::{Node, Parser};

pub(crate) fn parse_markdown_ast(source_code: &str) -> Option<Vec<ParsedNode>> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_md::LANGUAGE.into()).ok()?;
    let tree = parser.parse(source_code, None)?;
    let mut nodes = Vec::new();
    visit_markdown_node(tree.root_node(), source_code, &mut nodes);
    Some(nodes)
}

fn visit_markdown_node(node: Node, source: &str, nodes: &mut Vec<ParsedNode>) {
    if node.kind() == "atx_heading" || node.kind() == "setext_heading" {
        if let Some(heading_content) = node.child_by_field_name("heading_content") {
            let name = heading_content.utf8_text(source.as_bytes()).unwrap_or("").trim().to_string();
            if !name.is_empty() {
                push_parsed_node(
                    nodes,
                    name,
                    "markdown_heading",
                    None,
                    None,
                    source,
                );
            }
        } else {
            // fallback if tree-sitter-md exposes it differently
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "inline" || child.kind() == "paragraph" {
                    let name = child.utf8_text(source.as_bytes()).unwrap_or("").trim().to_string();
                    if !name.is_empty() {
                        push_parsed_node(nodes, name, "markdown_heading", None, None, source);
                        break;
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_markdown_node(child, source, nodes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_trailing_space() {
        let md1 = "# Hello\n\nSome text.";
        let md2 = "# Hello\n\nSome text. ";
        let ast1 = parse_markdown_ast(md1).unwrap();
        let ast2 = parse_markdown_ast(md2).unwrap();
        assert_eq!(ast1.len(), 1);
        assert_eq!(ast1[0].identity_key, "Hello");
        assert_eq!(ast2.len(), 1);
        assert_eq!(ast2[0].identity_key, "Hello");
    }
}
