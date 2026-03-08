use crate::models::{Symbol, SymbolKind, IndexStats};
use crate::store::SymbolStore;
use anyhow::{anyhow, Result};
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    C,
    Cpp,
    Elixir,
}

impl SupportedLanguage {
    pub fn as_str(&self) -> &'static str {
        match self {
            SupportedLanguage::Rust => "rust",
            SupportedLanguage::Python => "python",
            SupportedLanguage::TypeScript => "typescript",
            SupportedLanguage::JavaScript => "javascript",
            SupportedLanguage::Go => "go",
            SupportedLanguage::C => "c",
            SupportedLanguage::Cpp => "cpp",
            SupportedLanguage::Elixir => "elixir",
        }
    }

    pub fn tree_sitter_language(&self) -> Language {
        match self {
            SupportedLanguage::Rust => tree_sitter_rust::language(),
            SupportedLanguage::Python => tree_sitter_python::language(),
            SupportedLanguage::TypeScript => tree_sitter_typescript::language_typescript(),
            SupportedLanguage::JavaScript => tree_sitter_javascript::language(),
            SupportedLanguage::Go => tree_sitter_go::language(),
            SupportedLanguage::C => tree_sitter_c::language(),
            SupportedLanguage::Cpp => tree_sitter_cpp::language(),
            SupportedLanguage::Elixir => tree_sitter_elixir::language(),
        }
    }
}

/// Detects the programming language from a file path
pub fn detect_language(path: &Path) -> Option<SupportedLanguage> {
    let extension = path.extension()?.to_str()?;
    match extension {
        "rs" => Some(SupportedLanguage::Rust),
        "py" => Some(SupportedLanguage::Python),
        "ts" => Some(SupportedLanguage::TypeScript),
        "tsx" => Some(SupportedLanguage::TypeScript),
        "js" => Some(SupportedLanguage::JavaScript),
        "jsx" => Some(SupportedLanguage::JavaScript),
        "go" => Some(SupportedLanguage::Go),
        "c" => Some(SupportedLanguage::C),
        "h" => Some(SupportedLanguage::C),
        "cpp" => Some(SupportedLanguage::Cpp),
        "hpp" => Some(SupportedLanguage::Cpp),
        "cc" => Some(SupportedLanguage::Cpp),
        "cxx" => Some(SupportedLanguage::Cpp),
        "ex" | "exs" => Some(SupportedLanguage::Elixir),
        _ => None,
    }
}

/// Indexer walks the workspace, parses source files, and extracts symbols
pub struct Indexer {
    store: SymbolStore,
    parsers: HashMap<SupportedLanguage, Parser>,
    queries: HashMap<SupportedLanguage, Query>,
}

impl Indexer {
    /// Creates a new Indexer with cached parsers and queries
    pub fn new(store: SymbolStore) -> Result<Self> {
        let mut parsers = HashMap::new();
        let mut queries = HashMap::new();

        // Initialize parsers and queries for each language
        for lang in &[
            SupportedLanguage::Rust,
            SupportedLanguage::Python,
            SupportedLanguage::TypeScript,
            SupportedLanguage::JavaScript,
            SupportedLanguage::Go,
            SupportedLanguage::C,
            SupportedLanguage::Cpp,
            SupportedLanguage::Elixir,
        ] {
            let mut parser = Parser::new();
            parser.set_language(&lang.tree_sitter_language())?;
            parsers.insert(*lang, parser);

            let query_str = get_query_for_language(*lang);
            let query = Query::new(&lang.tree_sitter_language(), query_str)?;
            queries.insert(*lang, query);
        }

        Ok(Indexer {
            store,
            parsers,
            queries,
        })
    }

    /// Indexes the entire workspace, respecting .gitignore
    pub fn index_workspace(&mut self, root: &Path) -> Result<IndexStats> {
        let start = std::time::Instant::now();
        let mut stats = IndexStats::default();

        let walker = WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .follow_links(false)
            .build();

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            let Some(lang) = detect_language(path) else {
                continue;
            };

            // Get relative path from root
            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_path_buf();

            // Check if file needs reindexing
            let current_mtime = fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let needs_reindex = match self.store.get_indexed_mtime(&rel_path)? {
                Some(stored_mtime) => stored_mtime != current_mtime,
                None => true,
            };

            if !needs_reindex {
                stats.files_skipped += 1;
                continue;
            }

            // Parse and extract symbols
            match self.parse_file(path, lang) {
                Ok(symbols) => {
                    // Delete old symbols and upsert new ones
                    self.store.delete_file_symbols(&rel_path)?;
                    self.store.upsert_symbols(&rel_path, &symbols)?;
                    self.store.set_indexed_mtime(&rel_path, current_mtime)?;

                    stats.files_indexed += 1;
                    stats.symbols_total += symbols.len();
                }
                Err(e) => {
                    tracing::warn!("Failed to parse {}: {}", rel_path.display(), e);
                }
            }
        }

        stats.duration_ms = start.elapsed().as_millis() as u64;
        Ok(stats)
    }

    /// Parses a single file and extracts symbols
    fn parse_file(&mut self, path: &Path, lang: SupportedLanguage) -> Result<Vec<Symbol>> {
        let source = fs::read(path)?;
        let parser = self
            .parsers
            .get_mut(&lang)
            .ok_or_else(|| anyhow!("Parser not found for language"))?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| anyhow!("Failed to parse file"))?;

        let rel_path = path.to_string_lossy().to_string();
        let symbols = self.extract_symbols(&tree, &source, &rel_path, lang)?;

        Ok(symbols)
    }

    /// Extracts symbols from a parse tree using tree-sitter queries
    fn extract_symbols(
        &self,
        tree: &Tree,
        source: &[u8],
        path: &str,
        lang: SupportedLanguage,
    ) -> Result<Vec<Symbol>> {
        let query = self
            .queries
            .get(&lang)
            .ok_or_else(|| anyhow!("Query not found for language"))?;

        let capture_names = query.capture_names();

        let mut cursor = QueryCursor::new();
        let mut symbols = Vec::new();

        for m in cursor.matches(query, tree.root_node(), source) {
            // Find definition and name nodes by capture name within this match
            let mut def_node = None;
            let mut name_node = None;
            let mut def_capture_name = "";

            for cap in m.captures {
                let cap_name = capture_names
                    .get(cap.index as usize)
                    .map(|s| s.as_ref())
                    .unwrap_or("");
                if cap_name.starts_with("definition.") {
                    def_node = Some(cap.node);
                    def_capture_name = cap_name;
                } else if cap_name == "name" {
                    name_node = Some(cap.node);
                }
            }

            let (def_node, name_node) = match (def_node, name_node) {
                (Some(d), Some(n)) => (d, n),
                _ => continue,
            };

            let name = node_text(name_node, source);
            let qualified_name = build_qualified_name(def_node, source);
            let signature = first_line(node_text(def_node, source));
            let summary = extract_doc_comment(def_node, source);
            let kind = capture_name_to_kind_by_name(def_capture_name);

            let symbol = Symbol {
                id: 0,
                qualified_name,
                name,
                kind,
                language: lang.as_str().to_string(),
                signature,
                summary,
                file_path: path.to_string(),
                start_byte: def_node.start_byte() as u64,
                end_byte: def_node.end_byte() as u64,
                start_line: def_node.start_position().row as u32,
                end_line: def_node.end_position().row as u32,
            };

            if symbol.validate().is_ok() {
                symbols.push(symbol);
            }
        }

        Ok(symbols)
    }
}

/// Extracts text from a tree-sitter node
pub fn node_text(node: tree_sitter::Node, source: &[u8]) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    String::from_utf8_lossy(&source[start..end]).to_string()
}

/// Builds a qualified name by walking parent nodes upward
/// Builds a qualified name by walking parent nodes upward
pub fn build_qualified_name(node: tree_sitter::Node, source: &[u8]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut current = node;
    let mut is_elixir = false;

    // Walk upward collecting scope names
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "mod_item" | "impl_item" | "trait_item" | "class_definition" | "module" => {
                if let Some(name_child) = parent.child_by_field_name("name") {
                    parts.push(node_text(name_child, source));
                }
            }
            // Elixir: defmodule calls create scope
            "call" => {
                // Check if this is a defmodule call
                if let Some(target) = parent.child(0) {
                    if target.kind() == "identifier" && node_text(target, source) == "defmodule" {
                        is_elixir = true;
                        // The module name is in the arguments node (child 1)
                        if let Some(args) = parent.child(1) {
                            if args.kind() == "arguments" {
                                if let Some(first_arg) = args.named_child(0) {
                                    parts.push(node_text(first_arg, source));
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        current = parent;
    }

    parts.reverse();

    // Append the leaf symbol name
    // For Elixir def/defp/defmacro/defmacrop, the name is in the first argument (a call node)
    if node.kind() == "call" {
        if let Some(target) = node.child(0) {
            if target.kind() == "identifier" {
                let target_text = node_text(target, source);
                if target_text == "def" || target_text == "defp" || target_text == "defmacro" || target_text == "defmacrop" {
                    is_elixir = true;
                    // The function name is in the arguments node (child 1)
                    if let Some(args) = node.child(1) {
                        if args.kind() == "arguments" {
                            if let Some(first_arg) = args.named_child(0) {
                                // first_arg is a call node, get its target (the function name)
                                if let Some(func_target) = first_arg.child(0) {
                                    if func_target.kind() == "identifier" {
                                        parts.push(node_text(func_target, source));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // If no name was found via the above logic, try the standard name field
    if parts.is_empty() || (parts.len() == 1 && !is_elixir) {
        if let Some(name_node) = node.child_by_field_name("name") {
            parts.push(node_text(name_node, source));
        }
    }

    // Use "." separator for Elixir, "::" for everything else
    if is_elixir {
        parts.join(".")
    } else {
        parts.join("::")
    }
}

/// Extracts the first line of a node's source text
pub fn first_line(text: String) -> String {
    text.lines().next().unwrap_or("").to_string()
}

/// Extracts doc-comment summary from preceding comment nodes or Elixir attributes
fn extract_doc_comment(node: tree_sitter::Node, source: &[u8]) -> String {
    // Try to find a preceding sibling (comment or Elixir attribute)
    if let Some(prev_sibling) = node.prev_sibling() {
        let prev_text = node_text(prev_sibling, source);
        
        // Elixir @doc or @moduledoc attributes
        if prev_text.starts_with("@doc") || prev_text.starts_with("@moduledoc") {
            // Extract the string content after the attribute name
            let doc_content = if prev_text.starts_with("@moduledoc") {
                prev_text
                    .trim_start_matches("@moduledoc")
                    .trim()
            } else {
                prev_text
                    .trim_start_matches("@doc")
                    .trim()
            };
            
            // Remove heredoc markers (""") or regular string quotes
            let cleaned = doc_content
                .trim_start_matches("\"\"\"")
                .trim_end_matches("\"\"\"")
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim();
            
            return cleaned.lines().next().unwrap_or("").to_string();
        }
        
        // Standard comment handling for other languages
        if prev_sibling.kind() == "comment" {
            let comment_text = prev_text;
            // Remove comment markers and extract first line
            let cleaned = comment_text
                .trim_start_matches("//")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim();
            return cleaned.lines().next().unwrap_or("").to_string();
        }
    }
    String::new()
}

/// Maps capture index to SymbolKind based on query pattern
fn capture_name_to_kind_by_name(cap_name: &str) -> SymbolKind {
    // Map capture names to symbol kinds
    match cap_name {
        "definition.module" => SymbolKind::Module,
        "definition.function" => SymbolKind::Function,
        "definition.struct" => SymbolKind::Struct,
        "definition.enum" => SymbolKind::Enum,
        "definition.trait" => SymbolKind::Trait,
        "definition.impl" => SymbolKind::Impl,
        "definition.class" => SymbolKind::Class,
        "definition.interface" => SymbolKind::Interface,
        "definition.method" => SymbolKind::Method,
        _ => SymbolKind::Variable,
    }
}

/// Returns the tree-sitter query string for a language
fn get_query_for_language(lang: SupportedLanguage) -> &'static str {
    match lang {
        SupportedLanguage::Rust => RUST_QUERY,
        SupportedLanguage::Python => PYTHON_QUERY,
        SupportedLanguage::TypeScript => TYPESCRIPT_QUERY,
        SupportedLanguage::JavaScript => JAVASCRIPT_QUERY,
        SupportedLanguage::Go => GO_QUERY,
        SupportedLanguage::C => C_QUERY,
        SupportedLanguage::Cpp => CPP_QUERY,
        SupportedLanguage::Elixir => ELIXIR_QUERY,
    }
}

// Tree-sitter query patterns for each language

const RUST_QUERY: &str = r#"
(function_item
  name: (identifier) @name) @definition.function

(struct_item
  name: (type_identifier) @name) @definition.struct

(enum_item
  name: (type_identifier) @name) @definition.enum

(trait_item
  name: (type_identifier) @name) @definition.trait

(impl_item
  type: (type_identifier) @name) @definition.impl

(mod_item
  name: (identifier) @name) @definition.module
"#;

const PYTHON_QUERY: &str = r#"
(function_definition
  name: (identifier) @name) @definition.function

(class_definition
  name: (identifier) @name) @definition.class

(decorated_definition
  definition: (function_definition
    name: (identifier) @name)) @definition.function
"#;

const TYPESCRIPT_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @definition.function

(class_declaration
  name: (type_identifier) @name) @definition.class

(method_definition
  name: (property_identifier) @name) @definition.method

(interface_declaration
  name: (type_identifier) @name) @definition.interface
"#;

const JAVASCRIPT_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @definition.function

(class_declaration
  name: (identifier) @name) @definition.class

(method_definition
  name: (property_identifier) @name) @definition.method
"#;

const GO_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @definition.function

(method_declaration
  name: (field_identifier) @name) @definition.method

(type_declaration
  (type_spec
    name: (type_identifier) @name)) @definition.struct
"#;

const C_QUERY: &str = r#"
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

(struct_specifier
  name: (type_identifier) @name) @definition.struct

(enum_specifier
  name: (type_identifier) @name) @definition.enum
"#;

const CPP_QUERY: &str = r#"
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @definition.function

(class_specifier
  name: (type_identifier) @name) @definition.class

(struct_specifier
  name: (type_identifier) @name) @definition.struct

(enum_specifier
  name: (type_identifier) @name) @definition.enum
"#;

const ELIXIR_QUERY: &str = r#"
; Module definitions: defmodule MyApp.Foo do ... end
(call
  target: (identifier) @_target
  (arguments
    (alias) @name)
  (#eq? @_target "defmodule")) @definition.module

; Public function definitions: def foo(...) do ... end
(call
  target: (identifier) @_target
  (arguments
    (call
      target: (identifier) @name))
  (#eq? @_target "def")) @definition.function

; Private function definitions: defp foo(...) do ... end
(call
  target: (identifier) @_target
  (arguments
    (call
      target: (identifier) @name))
  (#eq? @_target "defp")) @definition.function

; Public macro definitions: defmacro foo(...) do ... end
(call
  target: (identifier) @_target
  (arguments
    (call
      target: (identifier) @name))
  (#eq? @_target "defmacro")) @definition.function

; Private macro definitions: defmacrop foo(...) do ... end
(call
  target: (identifier) @_target
  (arguments
    (call
      target: (identifier) @name))
  (#eq? @_target "defmacrop")) @definition.function

; Struct definitions: defstruct [:field1, :field2]
(call
  target: (identifier) @_target
  (#eq? @_target "defstruct")) @definition.struct

; Protocol definitions: defprotocol Foo do ... end
(call
  target: (identifier) @_target
  (arguments
    (alias) @name)
  (#eq? @_target "defprotocol")) @definition.interface

; Protocol implementations: defimpl Proto, for: Type do ... end
(call
  target: (identifier) @_target
  (arguments
    (alias) @name)
  (#eq? @_target "defimpl")) @definition.impl
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language_rust() {
        assert_eq!(
            detect_language(Path::new("main.rs")),
            Some(SupportedLanguage::Rust)
        );
    }

    #[test]
    fn test_detect_language_python() {
        assert_eq!(
            detect_language(Path::new("script.py")),
            Some(SupportedLanguage::Python)
        );
    }

    #[test]
    fn test_detect_language_typescript() {
        assert_eq!(
            detect_language(Path::new("app.ts")),
            Some(SupportedLanguage::TypeScript)
        );
    }

    #[test]
    fn test_detect_language_unsupported() {
        assert_eq!(detect_language(Path::new("file.txt")), None);
    }

    #[test]
    fn test_first_line() {
        let text = "pub fn foo() {\n    println!(\"hello\");\n}".to_string();
        assert_eq!(first_line(text), "pub fn foo() {");
    }

    #[test]
    fn test_first_line_single() {
        let text = "pub fn foo()".to_string();
        assert_eq!(first_line(text), "pub fn foo()");
    }
}
