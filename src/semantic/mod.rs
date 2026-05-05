mod common;
mod cpp;
mod go;
mod java;
mod javascript;
mod kotlin;
mod markdown;
mod python;
mod ruby;
mod rust;
mod swift;
#[cfg(test)]
mod tests;
mod typescript;

pub use common::ParsedNode;
#[cfg(test)]
pub use rust::parse_standard_rust_ast;

pub fn parse_semantic_ast(
    file_path: &str,
    source_code: &str,
    query_str: &str,
) -> Option<Vec<ParsedNode>> {
    if file_path.ends_with(".rs") {
        rust::parse_standard_rust_ast(source_code, query_str)
    } else if file_path.ends_with(".py") {
        python::parse_python_ast(source_code)
    } else if file_path.ends_with(".ts") {
        typescript::parse_typescript_ast(source_code, false)
    } else if file_path.ends_with(".tsx") {
        typescript::parse_typescript_ast(source_code, true)
    } else if file_path.ends_with(".js") || file_path.ends_with(".jsx") {
        javascript::parse_javascript_ast(source_code)
    } else if file_path.ends_with(".go") {
        go::parse_go_ast(source_code)
    } else if file_path.ends_with(".java") {
        java::parse_java_ast(source_code)
    } else if file_path.ends_with(".c")
        || file_path.ends_with(".cc")
        || file_path.ends_with(".cpp")
        || file_path.ends_with(".h")
        || file_path.ends_with(".hpp")
    {
        cpp::parse_cpp_ast(source_code)
    } else if file_path.ends_with(".rb") {
        ruby::parse_ruby_ast(source_code)
    } else if file_path.ends_with(".swift") {
        swift::parse_swift_ast(source_code)
    } else if file_path.ends_with(".kt") || file_path.ends_with(".kts") {
        kotlin::parse_kotlin_ast(source_code)
    } else if file_path.ends_with(".md") {
        markdown::parse_markdown_ast(source_code)
    } else {
        None
    }
}
