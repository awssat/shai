use super::{parse_semantic_ast, parse_standard_rust_ast};
use crate::verbalizer::{diff_snapshots, verbalize};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

fn rust_query() -> &'static str {
    include_str!("../../queries/rust.scm")
}

fn query_match_count(source: &str) -> usize {
    let lang = tree_sitter_rust::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let query = Query::new(&lang, rust_query()).unwrap();
    let mut cursor = QueryCursor::new();
    cursor
        .matches(&query, tree.root_node(), source.as_bytes())
        .count()
}

#[test]
fn captures_free_function_and_verbalizes_it() {
    let source = r#"
fn player_movement(mut query: Query<&mut Transform>) {
    let _ = &mut query;
}
"#;

    assert!(
        query_match_count(source) > 0,
        "query should match free function fixture"
    );

    let nodes = parse_standard_rust_ast(source, rust_query()).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].identity_key, "player_movement");
    assert_eq!(nodes[0].node_type, "free_function");

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert_eq!(summary, "Added function `player_movement`.");
}

#[test]
fn captures_impl_method_and_verbalizes_it() {
    let source = r#"
struct PlayerPlugin;

impl PlayerPlugin {
    fn update(&self) {}
}
"#;

    assert!(
        query_match_count(source) > 0,
        "query should match impl fixture"
    );

    let nodes = parse_standard_rust_ast(source, rust_query()).unwrap();
    assert!(
        nodes
            .iter()
            .any(|node| node.identity_key == "PlayerPlugin::update"
                && node.node_type == "impl_method")
    );

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert!(summary.contains("Added method `PlayerPlugin::update`."));
}

#[test]
fn captures_struct_and_verbalizes_it() {
    let source = r#"
struct Health {
    current: f32,
    max: f32,
}
"#;

    assert!(
        query_match_count(source) > 0,
        "query should match struct fixture"
    );

    let nodes = parse_standard_rust_ast(source, rust_query()).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].identity_key, "Health");
    assert_eq!(nodes[0].node_type, "struct_def");

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert_eq!(summary, "Added struct `Health`.");
}

#[test]
fn enum_variant_changes_are_detected() {
    let before = "enum AppState { Loading }";
    let after = "enum AppState { Loading, InGame }";

    assert!(
        query_match_count(after) > 0,
        "query should match enum fixture"
    );

    let before_nodes = parse_standard_rust_ast(before, rust_query()).unwrap();
    let after_nodes = parse_standard_rust_ast(after, rust_query()).unwrap();

    assert_eq!(before_nodes[0].identity_key, "AppState");
    assert_eq!(after_nodes[0].identity_key, "AppState");
    assert_eq!(before_nodes[0].node_type, "enum_def");
    assert_eq!(after_nodes[0].node_type, "enum_def");
    assert_ne!(
        before_nodes[0].shallow_body_hash,
        after_nodes[0].shallow_body_hash
    );

    let summary = verbalize(&diff_snapshots(before_nodes, after_nodes));
    assert_eq!(
        summary,
        "Modified variants of `AppState` (signature unchanged)."
    );
}

#[test]
fn captures_type_alias_and_verbalizes_it() {
    let source = r#"
type Health = f32;
"#;

    assert!(
        query_match_count(source) > 0,
        "query should match type alias fixture"
    );

    let nodes = parse_standard_rust_ast(source, rust_query()).unwrap();
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Health" && node.node_type == "type_alias"));

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert!(summary.contains("Added type alias `Health`."));
}

#[test]
fn captures_python_class_and_method() {
    let source = r#"
class Parser:
    def parse(self, text):
        return text
"#;

    let nodes = parse_semantic_ast("parser.py", source, "").unwrap();
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Parser" && node.node_type == "struct_def"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Parser::parse" && node.node_type == "impl_method"));

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert!(summary.contains("Added struct `Parser`."));
    assert!(summary.contains("Added method `Parser::parse`."));
}

#[test]
fn captures_typescript_interface_enum_and_function() {
    let source = r#"
interface Store {
  load(key: string): string;
}

enum Mode {
  Fast,
}

function hydrate(input: string): string {
  return input;
}
"#;

    let nodes = parse_semantic_ast("store.ts", source, "").unwrap();
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Store" && node.node_type == "struct_def"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Store::load" && node.node_type == "impl_method"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Mode" && node.node_type == "enum_def"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "hydrate" && node.node_type == "free_function"));

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert!(summary.contains("Added struct `Store`."));
    assert!(summary.contains("Added enum `Mode`."));
    assert!(summary.contains("Added function `hydrate`."));
}

#[test]
fn captures_javascript_class_and_function() {
    let source = r#"
class Store {
  load(key) {
    return key;
  }
}

function hydrate(input) {
  return input;
}
"#;

    let nodes = parse_semantic_ast("store.js", source, "").unwrap();
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Store" && node.node_type == "struct_def"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Store::load" && node.node_type == "impl_method"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "hydrate" && node.node_type == "free_function"));

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert!(summary.contains("Added struct `Store`."));
    assert!(summary.contains("Added method `Store::load`."));
    assert!(summary.contains("Added function `hydrate`."));
}

#[test]
fn captures_go_type_method_and_function() {
    let source = r#"
type Store struct{}

func (s *Store) Load(key string) string {
    return key
}

func Hydrate(input string) string {
    return input
}
"#;

    let nodes = parse_semantic_ast("store.go", source, "").unwrap();
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Store" && node.node_type == "struct_def"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Store::Load" && node.node_type == "impl_method"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Hydrate" && node.node_type == "free_function"));

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert!(summary.contains("Added struct `Store`."));
    assert!(summary.contains("Added method `Store::Load`."));
    assert!(summary.contains("Added function `Hydrate`."));
}

#[test]
fn captures_java_class_enum_and_method() {
    let source = r#"
class Store {
    String load(String key) {
        return key;
    }
}

enum Mode {
    FAST
}
"#;

    let nodes = parse_semantic_ast("Store.java", source, "").unwrap();
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Store" && node.node_type == "struct_def"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Store::load" && node.node_type == "impl_method"));
    assert!(nodes
        .iter()
        .any(|node| node.identity_key == "Mode" && node.node_type == "enum_def"));

    let summary = verbalize(&diff_snapshots(vec![], nodes));
    assert!(summary.contains("Added struct `Store`."));
    assert!(summary.contains("Added method `Store::load`."));
    assert!(summary.contains("Added enum `Mode`."));
}
