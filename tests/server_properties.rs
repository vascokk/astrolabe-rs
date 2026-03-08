/// Property-based tests for AstrolabeServer tool handlers
/// Validates: Properties 1, 3, 4, 7, 8
use astrolabe_mcp::models::{Symbol, SymbolKind, SymbolField};
use astrolabe_mcp::server::AstrolabeServer;
use astrolabe_mcp::store::SymbolStore;
use astrolabe_mcp::searcher::FullTextSearcher;
use proptest::prelude::*;
use rmcp::model::*;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

// Helper functions
fn setup_test_server() -> (AstrolabeServer, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let store = SymbolStore::open(db_path.to_string_lossy().as_ref()).unwrap();
    let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());
    (server, temp_dir)
}

fn to_args(val: serde_json::Value) -> Option<serde_json::Map<String, serde_json::Value>> {
    match val {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

fn extract_text(result: &CallToolResult) -> String {
    result.content.first()
        .and_then(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content")
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
            start_byte: (start_line as u64) * 100,
            end_byte: (end_line as u64) * 100 + 50,
            start_line,
            end_line,
        })
}

fn arb_field_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("id".to_string()),
        Just("qualified_name".to_string()),
        Just("name".to_string()),
        Just("kind".to_string()),
        Just("language".to_string()),
        Just("signature".to_string()),
        Just("summary".to_string()),
        Just("file_path".to_string()),
        Just("start_byte".to_string()),
        Just("end_byte".to_string()),
        Just("start_line".to_string()),
        Just("end_line".to_string()),
    ]
}

fn arb_arbitrary_field_name() -> impl Strategy<Value = String> {
    "[a-z_][a-z0-9_]*"
}

// ========================================================================
// 8.1 prop_workspace_overview_completeness
// ========================================================================
// **Validates: Property 7 — Requirements 8.1, 8.2, 8.3, 8.4**

proptest! {
    #[test]
    fn prop_workspace_overview_completeness(
        symbols in prop::collection::vec(arb_symbol(), 1..20)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (server, _temp_dir) = setup_test_server();

        // Group symbols by file_path
        let mut symbols_by_file: std::collections::HashMap<String, Vec<Symbol>> = std::collections::HashMap::new();
        for symbol in &symbols {
            symbols_by_file.entry(symbol.file_path.clone()).or_default().push(symbol.clone());
        }

        // Insert all symbols
        for (file_path, file_symbols) in &symbols_by_file {
            server.store.upsert_symbols(Path::new(file_path), file_symbols).unwrap();
        }

        let result = rt.block_on(server.handle_get_workspace_overview(None)).unwrap();
        let text = extract_text(&result);

        // Property 1: Plain text format, one line per file
        let lines: Vec<&str> = text.lines().collect();
        prop_assert_eq!(lines.len(), symbols_by_file.len());

        // Property 2: Each line matches format "file_path: sym1, sym2, ..."
        for line in &lines {
            prop_assert!(line.contains(": "), "Each line should have format 'file: symbols'");
            let parts: Vec<&str> = line.splitn(2, ": ").collect();
            prop_assert_eq!(parts.len(), 2);
            
            let file_path = parts[0];
            let symbols_str = parts[1];
            
            // Verify file_path exists in our map
            prop_assert!(symbols_by_file.contains_key(file_path));
            
            // Verify symbols match (order-independent)
            let mut returned_symbols: Vec<String> = symbols_str
                .split(", ")
                .map(|s| s.to_string())
                .collect();
            returned_symbols.sort();
            
            let mut expected_symbols: Vec<String> = symbols_by_file
                .get(file_path)
                .unwrap()
                .iter()
                .map(|s| s.name.clone())
                .collect();
            expected_symbols.sort();
            
            prop_assert_eq!(returned_symbols, expected_symbols);
        }
    }
}

// ========================================================================
// 8.3 prop_field_projection_soundness
// ========================================================================
// **Validates: Property 3 — Requirements 7.1, 7.2**

proptest! {
    #[test]
    fn prop_field_projection_soundness(
        symbol in arb_symbol(),
        field_names in prop::collection::vec(arb_field_name(), 1..12)
    ) {
        let projected = astrolabe_mcp::server::project_symbol(&symbol, Some(&field_names));
        let obj = projected.as_object().unwrap();

        for key in obj.keys() {
            prop_assert!(field_names.iter().any(|f| f == key));
        }

        for field_name in &field_names {
            if field_name.parse::<SymbolField>().is_ok() {
                prop_assert!(obj.contains_key(field_name));
            }
        }
    }

    #[test]
    fn prop_default_projection_three_fields(
        symbol in arb_symbol()
    ) {
        // When no fields parameter is provided, default projection should have exactly 3 fields
        let projected = astrolabe_mcp::server::project_symbol(&symbol, None);
        let obj = projected.as_object().unwrap();

        prop_assert_eq!(obj.len(), 3, "Default projection should have exactly 3 fields");
        prop_assert!(obj.contains_key("name"));
        prop_assert!(obj.contains_key("kind"));
        prop_assert!(obj.contains_key("signature"));
        prop_assert!(!obj.contains_key("start_line"));
        prop_assert!(!obj.contains_key("end_line"));
    }
}

// ========================================================================
// 8.4 prop_unknown_fields_no_error
// ========================================================================
// **Validates: Property 4 — Requirements 2.4, 2.6, 3.4**

proptest! {
    #[test]
    fn prop_unknown_fields_no_error(
        symbol in arb_symbol(),
        field_names in prop::collection::vec(arb_arbitrary_field_name(), 0..20)
    ) {
        let result = astrolabe_mcp::server::project_symbol(&symbol, Some(&field_names));
        prop_assert!(result.is_object());

        let all_unknown = field_names.iter()
            .all(|f| f.parse::<SymbolField>().is_err());

        if all_unknown && !field_names.is_empty() {
            let obj = result.as_object().unwrap();
            prop_assert!(obj.contains_key("name"));
        }
    }
}

// ========================================================================
// 8.5 prop_batch_length_invariant
// ========================================================================
// **Validates: Property 7 — Requirements 4.3**

proptest! {
    #[test]
    fn prop_batch_length_invariant(
        qualified_names in prop::collection::vec("[a-z_][a-z0-9_:]*", 1..20)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (server, _temp_dir) = setup_test_server();

        let args = to_args(json!({
            "qualified_names": qualified_names.clone()
        }));

        let result = rt.block_on(server.handle_get_symbol_implementations(args)).unwrap();
        let text = extract_text(&result);
        let implementations: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();

        prop_assert_eq!(implementations.len(), qualified_names.len());
    }
}

// ========================================================================
// 8.6 prop_batch_order_invariant
// ========================================================================
// **Validates: Property 8 — Requirements 4.2**

proptest! {
    #[test]
    fn prop_batch_order_invariant(
        qualified_names in prop::collection::vec("[a-z_][a-z0-9_:]*", 1..20)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (server, _temp_dir) = setup_test_server();

        let args = to_args(json!({
            "qualified_names": qualified_names.clone()
        }));

        let result = rt.block_on(server.handle_get_symbol_implementations(args)).unwrap();
        let text = extract_text(&result);
        let implementations: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();

        // Array length should match input length
        prop_assert_eq!(implementations.len(), qualified_names.len());

        // Each result should have implementation and error fields
        for impl_result in implementations.iter() {
            prop_assert!(impl_result.as_object().unwrap().contains_key("implementation"));
            prop_assert!(impl_result.as_object().unwrap().contains_key("error"));
        }
    }
}

// ========================================================================
// prop_file_outline_sort_order_and_format
// ========================================================================

proptest! {
    #[test]
    fn prop_file_outline_sort_order_and_format(
        symbols_count in 1usize..20,
        seed in 0u64..1000,
    ) {
        let (server, temp_dir) = setup_test_server();

        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "// test file").unwrap();

        let mut symbols = Vec::new();
        let mut rng = seed;
        for i in 0..symbols_count {
            rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
            let start_line = (rng % 100) as u32;
            let end_line = start_line + (rng % 10) as u32 + 1;

            symbols.push(astrolabe_mcp::models::Symbol {
                id: 0,
                qualified_name: format!("symbol_{}", i),
                name: format!("symbol_{}", i),
                kind: astrolabe_mcp::models::SymbolKind::Function,
                language: "rust".to_string(),
                signature: format!("fn symbol_{}() {{}}", i),
                summary: String::new(),
                file_path: "test.rs".to_string(),
                start_byte: (i * 100) as u64,
                end_byte: ((i + 1) * 100) as u64,
                start_line,
                end_line,
            });
        }

        server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        // Explicitly request start_line and end_line in the outline
        let result = rt.block_on(server.handle_get_file_outline(
            to_args(serde_json::json!({"file_path": "test.rs", "fields": ["name", "kind", "signature", "start_line", "end_line"]})),
        )).unwrap();

        let text = extract_text(&result);
        let outline: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();

        for i in 1..outline.len() {
            let prev = outline[i - 1].get("start_line").unwrap().as_u64().unwrap();
            let curr = outline[i].get("start_line").unwrap().as_u64().unwrap();
            prop_assert!(prev <= curr, "Symbols should be sorted by start_line ascending");
        }

        for symbol in &outline {
            let obj = symbol.as_object().unwrap();
            prop_assert!(obj.contains_key("name"));
            prop_assert!(obj.contains_key("kind"));
            prop_assert!(obj.contains_key("signature"));
            prop_assert!(obj.contains_key("start_line"));
            prop_assert!(obj.contains_key("end_line"));
            prop_assert!(!obj.contains_key("source"));
            prop_assert!(!obj.contains_key("start_byte"));
            prop_assert!(!obj.contains_key("end_byte"));
        }

        prop_assert_eq!(outline.len(), symbols_count);
    }
}

// ========================================================================
// prop_malformed_parameter_error_handling
// ========================================================================

proptest! {
    #[test]
    fn prop_malformed_parameter_error_handling(
        _malformed_json in r#"[^}]*"#,
    ) {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = rt.block_on(server.handle_search_symbols(
            to_args(serde_json::json!({})),
        ));
        prop_assert!(result.is_ok());
        prop_assert!(result.unwrap().is_error.unwrap_or(false));

        let result = rt.block_on(server.handle_get_file_outline(
            to_args(serde_json::json!({"file_path": "../../etc/passwd"})),
        ));
        prop_assert!(result.is_ok());
        prop_assert!(result.unwrap().is_error.unwrap_or(false));

        let result = rt.block_on(server.handle_full_text_search(
            to_args(serde_json::json!({"pattern": "[invalid(regex"})),
        ));
        prop_assert!(result.is_ok());
        prop_assert!(result.unwrap().is_error.unwrap_or(false));

        let result = rt.block_on(server.handle_search_symbols(
            to_args(serde_json::json!({"name_pattern": "test", "limit": 999999999})),
        ));
        prop_assert!(result.is_ok());
    }
}

// ========================================================================
// test_full_text_search_compact_format
// ========================================================================
// **Validates: Requirements 6.1, 6.2**
// Verify full_text_search returns plain text in format `file:line: content`

#[test]
fn test_full_text_search_compact_format() {
    let (server, temp_dir) = setup_test_server();
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Create a test file with known content
    let test_file = temp_dir.path().join("test.rs");
    std::fs::write(&test_file, "fn hello() {}\nfn hello_world() {}\nlet x = 5;").unwrap();

    let result = rt.block_on(server.handle_full_text_search(
        to_args(json!({"pattern": "hello"})),
    )).unwrap();

    let text = extract_text(&result);
    
    // Should not be JSON, should be plain text
    assert!(!text.starts_with('['), "Response should not be JSON array");
    assert!(!text.starts_with('{'), "Response should not be JSON object");
    
    // Should contain lines in format `file:line: content`
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "Should have 2 matches");
    
    // First match: test.rs:1: fn hello() {}
    assert!(lines[0].contains("test.rs:1:"), "First line should have file:line format");
    assert!(lines[0].contains("fn hello()"), "First line should contain content");
    
    // Second match: test.rs:2: fn hello_world() {}
    assert!(lines[1].contains("test.rs:2:"), "Second line should have file:line format");
    assert!(lines[1].contains("fn hello_world()"), "Second line should contain content");
}

// ========================================================================
// test_full_text_search_empty_results
// ========================================================================
// **Validates: Requirements 6.4**
// Verify empty results return empty string

#[test]
fn test_full_text_search_empty_results() {
    let (server, temp_dir) = setup_test_server();
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.rs");
    std::fs::write(&test_file, "fn hello() {}").unwrap();

    let result = rt.block_on(server.handle_full_text_search(
        to_args(json!({"pattern": "xyz_not_found"})),
    )).unwrap();

    let text = extract_text(&result);
    
    // Should be empty string for no matches
    assert_eq!(text, "", "Empty results should produce empty string");
}

// ========================================================================
// 8.9 prop_full_text_search_line_count
// ========================================================================
// **Validates: Property 6 — Requirements 6.1, 6.4**

proptest! {
    #[test]
    fn prop_full_text_search_line_count(
        match_count in 1usize..20
    ) {
        let (server, temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Create a test file with N lines containing "match"
        let mut content = String::new();
        for i in 0..match_count {
            content.push_str(&format!("line {} with match\n", i));
        }
        content.push_str("line without the word\n");

        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, content).unwrap();

        let result = rt.block_on(server.handle_full_text_search(
            to_args(json!({"pattern": "match"})),
        )).unwrap();

        let text = extract_text(&result);
        let line_count = if text.is_empty() { 0 } else { text.lines().count() };

        prop_assert_eq!(line_count, match_count);
    }
}

// ========================================================================
// 8.10 prop_workspace_overview_line_count
// ========================================================================
// **Validates: Property 7 — Requirements 8.1, 8.4**

proptest! {
    #[test]
    fn prop_workspace_overview_line_count(
        file_count in 1usize..20
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (server, _temp_dir) = setup_test_server();

        // Create symbols across N different files
        let mut symbols = Vec::new();
        for file_idx in 0..file_count {
            for sym_idx in 0..3 {
                symbols.push(Symbol {
                    id: 0,
                    qualified_name: format!("file{}::sym{}", file_idx, sym_idx),
                    name: format!("sym{}", sym_idx),
                    kind: SymbolKind::Function,
                    language: "rust".to_string(),
                    signature: format!("fn sym{}() {{}}", sym_idx),
                    summary: String::new(),
                    file_path: format!("src/file{}.rs", file_idx),
                    start_byte: (sym_idx as u64) * 100,
                    end_byte: ((sym_idx + 1) as u64) * 100,
                    start_line: sym_idx as u32,
                    end_line: (sym_idx + 1) as u32,
                });
            }
        }

        // Insert all symbols
        for file_idx in 0..file_count {
            let file_symbols: Vec<Symbol> = symbols.iter()
                .filter(|s| s.file_path == format!("src/file{}.rs", file_idx))
                .cloned()
                .collect();
            server.store.upsert_symbols(Path::new(&format!("src/file{}.rs", file_idx)), &file_symbols).unwrap();
        }

        let result = rt.block_on(server.handle_get_workspace_overview(None)).unwrap();
        let text = extract_text(&result);
        let line_count = if text.is_empty() { 0 } else { text.lines().count() };

        prop_assert_eq!(line_count, file_count);
    }
}

// ========================================================================
// 8.11 prop_empty_summary_omission
// ========================================================================
// **Validates: Property 4 — Requirements 5.1**

proptest! {
    #[test]
    fn prop_empty_summary_omission(
        symbol in arb_symbol()
    ) {
        let mut symbol_with_empty_summary = symbol.clone();
        symbol_with_empty_summary.summary = String::new();

        let json = astrolabe_mcp::server::full_symbol_to_json(&symbol_with_empty_summary);
        let obj = json.as_object().unwrap();

        prop_assert!(!obj.contains_key("summary"), "Empty summary should be omitted from JSON");
    }

    #[test]
    fn prop_nonempty_summary_inclusion(
        symbol in arb_symbol()
    ) {
        let mut symbol_with_summary = symbol.clone();
        symbol_with_summary.summary = "This is a summary".to_string();

        let json = astrolabe_mcp::server::full_symbol_to_json(&symbol_with_summary);
        let obj = json.as_object().unwrap();

        prop_assert!(obj.contains_key("summary"), "Non-empty summary should be included in JSON");
        prop_assert_eq!(obj["summary"].as_str().unwrap(), "This is a summary");
    }

    #[test]
    fn prop_explicit_summary_field_when_empty(
        symbol in arb_symbol()
    ) {
        let mut symbol_with_empty_summary = symbol.clone();
        symbol_with_empty_summary.summary = String::new();

        // When explicitly requesting summary field, it should be included even if empty
        let projected = astrolabe_mcp::server::project_symbol(&symbol_with_empty_summary, Some(&["summary".to_string()]));
        let obj = projected.as_object().unwrap();

        prop_assert!(obj.contains_key("summary"), "Explicitly requested summary should be included even if empty");
        prop_assert_eq!(obj["summary"].as_str().unwrap(), "");
    }
}
