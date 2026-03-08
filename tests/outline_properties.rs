/// Property-based tests for file outline formatting
/// Validates: Properties 11, 12 — Requirements 6.2, 6.3, 6.6
use astrolabe_mcp::models::{Symbol, SymbolKind};
use astrolabe_mcp::server::AstrolabeServer;
use astrolabe_mcp::store::SymbolStore;
use astrolabe_mcp::searcher::FullTextSearcher;
use proptest::prelude::*;
use rmcp::model::*;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

// Helper functions
fn extract_text(result: &CallToolResult) -> String {
    result.content.first()
        .and_then(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content")
}

fn to_args(val: serde_json::Value) -> Option<serde_json::Map<String, serde_json::Value>> {
    match val {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

// Strategies
fn arb_symbol_kind() -> impl Strategy<Value = SymbolKind> {
    prop_oneof![
        Just(SymbolKind::Function),
        Just(SymbolKind::Struct),
        Just(SymbolKind::Enum),
        Just(SymbolKind::Trait),
        Just(SymbolKind::Impl),
        Just(SymbolKind::Module),
        Just(SymbolKind::Const),
        Just(SymbolKind::TypeAlias),
        Just(SymbolKind::Method),
        Just(SymbolKind::Field),
        Just(SymbolKind::Variable),
        Just(SymbolKind::Class),
        Just(SymbolKind::Interface),
    ]
}

fn arb_symbol() -> impl Strategy<Value = Symbol> {
    (
        "[a-z_][a-z0-9_]*",
        "[a-z_][a-z0-9_]*",
        "src/[a-z_][a-z0-9_]*\\.rs",
        1u32..1000,
        arb_symbol_kind(),
    )
        .prop_flat_map(|(name, qname, file, start_line, kind)| {
            let end_line = start_line..start_line + 100;
            (
                Just(name),
                Just(qname),
                Just(file),
                Just(start_line),
                end_line,
                Just(kind),
            )
        })
        .prop_map(|(name, qname, file, start_line, end_line, kind)| Symbol {
            id: 0,
            qualified_name: format!("test::{}", qname),
            name,
            kind,
            language: "rust".to_string(),
            signature: format!("fn {}() {{}}", qname),
            summary: format!("Summary for {}", qname),
            file_path: file,
            start_byte: ((start_line as u64) * 100) as i64,
            end_byte: ((end_line as u64) * 100 + 50) as i64,
            start_line,
            end_line,
        })
}

// ========================================================================
// 8.7 prop_compact_line_count
// ========================================================================
// **Validates: Property 11 — Requirements 6.2, 6.6**

proptest! {
    #[test]
    fn prop_compact_line_count(
        mut symbols in prop::collection::vec(arb_symbol(), 0..20)
    ) {
        // Ensure all symbols have the same file_path and unique qualified names and line numbers
        for (i, symbol) in symbols.iter_mut().enumerate() {
            symbol.file_path = "test.rs".to_string();
            symbol.name = format!("sym_{}", i);
            symbol.qualified_name = format!("test::sym_{}", i);
            symbol.start_line = (i as u32) * 10;
            symbol.end_line = (i as u32) * 10 + 5;
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = SymbolStore::open(db_path.to_string_lossy().as_ref()).unwrap();
        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "// test file").unwrap();

        if !symbols.is_empty() {
            server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();
        }

        let args = to_args(json!({
            "file_path": "test.rs",
            "format": "compact"
        }));

        let result = rt.block_on(server.handle_get_file_outline(args)).unwrap();
        let text = extract_text(&result);

        let line_count = if text.is_empty() { 0 } else { text.lines().count() };
        prop_assert_eq!(line_count, symbols.len());
    }
}

// ========================================================================
// 8.8 prop_compact_line_format
// ========================================================================
// **Validates: Property 12 — Requirements 6.3**

proptest! {
    #[test]
    fn prop_compact_line_format(
        symbols in prop::collection::vec(arb_symbol(), 1..20)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = SymbolStore::open(db_path.to_string_lossy().as_ref()).unwrap();
        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "// test file").unwrap();

        server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();

        let args = to_args(json!({
            "file_path": "test.rs",
            "format": "compact"
        }));

        let result = rt.block_on(server.handle_get_file_outline(args)).unwrap();
        let text = extract_text(&result);

        let regex = regex::Regex::new(r"^\S+ \S+ \[\d+-\d+\]$").unwrap();
        for line in text.lines() {
            prop_assert!(regex.is_match(line));
        }
    }
}
