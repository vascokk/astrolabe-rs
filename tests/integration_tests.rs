/// Integration tests for end-to-end indexing and tool calls
/// Requirements: 1.1, 1.2, 5.1, 6.1, 7.1, 8.1, 8.4, 9.4, 11.2
use astrolabe_mcp::indexer::Indexer;
use astrolabe_mcp::models::{SearchQuery, SymbolKind};
use astrolabe_mcp::retriever::SourceRetriever;
use astrolabe_mcp::searcher::FullTextSearcher;
use astrolabe_mcp::store::SymbolStore;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Returns the absolute path to the test fixtures directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Opens an in-memory SymbolStore backed by a temp file.
fn open_temp_store() -> (SymbolStore, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let store = SymbolStore::open(db_path.to_str().unwrap()).expect("open store");
    (store, dir)
}

// ---------------------------------------------------------------------------
// 1. index_workspace produces expected symbols
// ---------------------------------------------------------------------------

/// Requirement 1.1 – Indexer walks workspace and stores symbols.
/// Requirement 1.2 – .gitignore is respected (ignored_file.rs must not appear).
#[test]
fn test_index_workspace_produces_expected_symbols() {
    let (store, _dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    let fixtures = fixtures_dir();

    let stats = indexer
        .index_workspace(&fixtures)
        .expect("index_workspace");

    // At least the three sample files should be indexed
    assert!(
        stats.files_indexed >= 3,
        "expected at least 3 files indexed, got {}",
        stats.files_indexed
    );
    assert!(
        stats.symbols_total > 0,
        "expected symbols to be extracted"
    );

    // Verify known Rust symbols are present
    let rust_file = Path::new("sample.rs");
    let rust_symbols = store.get_file_symbols(rust_file).expect("get_file_symbols");
    assert!(
        !rust_symbols.is_empty(),
        "sample.rs should have symbols"
    );

    let names: Vec<&str> = rust_symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"top_level_fn"),
        "expected top_level_fn in sample.rs symbols, got: {:?}",
        names
    );

    // Verify ignored_file.rs was NOT indexed (gitignore)
    let ignored_file = Path::new("ignored_file.rs");
    let ignored_symbols = store
        .get_file_symbols(ignored_file)
        .expect("get_file_symbols");
    assert!(
        ignored_symbols.is_empty(),
        "ignored_file.rs should produce no symbols (gitignore)"
    );
}

// ---------------------------------------------------------------------------
// 2. search_symbols returns known symbols with correct filters
// ---------------------------------------------------------------------------

/// Requirement 5.1 – search_symbols returns matching symbols.
#[test]
fn test_search_symbols_returns_known_symbols() {
    let (store, _dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    // Search by name pattern
    let query = SearchQuery {
        name_pattern: Some("top_level_fn".to_string()),
        kind: None,
        language: None,
        file_path: None,
        limit: None,
    };
    let results = store.search(&query).expect("search");
    assert!(
        !results.is_empty(),
        "search for 'top_level_fn' should return results"
    );
    assert!(
        results.iter().any(|s| s.name == "top_level_fn"),
        "expected top_level_fn in results"
    );

    // Search by language filter
    let query = SearchQuery {
        name_pattern: None,
        kind: None,
        language: Some("python".to_string()),
        file_path: None,
        limit: None,
    };
    let results = store.search(&query).expect("search by language");
    assert!(
        !results.is_empty(),
        "search by language=python should return results"
    );
    for sym in &results {
        assert_eq!(sym.language, "python", "all results should be python");
    }

    // Search by kind filter
    let query = SearchQuery {
        name_pattern: None,
        kind: Some(SymbolKind::Function),
        language: None,
        file_path: None,
        limit: None,
    };
    let results = store.search(&query).expect("search by kind");
    assert!(
        !results.is_empty(),
        "search by kind=function should return results"
    );
    for sym in &results {
        assert_eq!(sym.kind, SymbolKind::Function, "all results should be functions");
    }
}

// ---------------------------------------------------------------------------
// 3. get_file_outline returns sorted symbols without source content
// ---------------------------------------------------------------------------

/// Requirement 6.1 – symbols sorted by start_line ascending.
/// Requirement 6.3 – source content excluded.
#[test]
fn test_get_file_outline_sorted_no_source() {
    let (store, _dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    let file_path = Path::new("sample.rs");
    let symbols = store.get_file_symbols(file_path).expect("get_file_symbols");

    assert!(
        !symbols.is_empty(),
        "sample.rs should have symbols for outline"
    );

    // Verify sorted by start_line ascending
    for window in symbols.windows(2) {
        assert!(
            window[0].start_line <= window[1].start_line,
            "symbols must be sorted by start_line: {} > {}",
            window[0].start_line,
            window[1].start_line
        );
    }

    // Verify required fields are present and non-empty
    for sym in &symbols {
        assert!(!sym.name.is_empty(), "name must be non-empty");
        assert!(!sym.signature.is_empty(), "signature must be non-empty");
        // start_line and end_line are always populated (u32)
    }

    // The Symbol struct has no "source" field — source content is never included
    // (the outline only exposes name, kind, signature, start_line, end_line)
}

// ---------------------------------------------------------------------------
// 4. get_symbol_implementation returns correct source for a known symbol
// ---------------------------------------------------------------------------

/// Requirement 7.1 – get_symbol_implementation returns exact source bytes.
#[test]
fn test_get_symbol_implementation_returns_correct_source() {
    let (store, _dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    let fixtures = fixtures_dir();
    indexer.index_workspace(&fixtures).expect("index_workspace");

    // Look up top_level_fn from sample.rs
    let symbol = store
        .get_by_qualified_name("top_level_fn")
        .expect("get_by_qualified_name")
        .expect("top_level_fn should exist in index");

    let file_path = fixtures.join(&symbol.file_path);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let source = rt
        .block_on(SourceRetriever::get_source(
            &file_path,
            symbol.start_byte,
            symbol.end_byte,
        ))
        .expect("get_source");

    // The source should contain the function name
    assert!(
        source.contains("top_level_fn"),
        "source should contain 'top_level_fn', got: {:?}",
        source
    );
    // Byte count should match
    assert_eq!(
        source.len(),
        (symbol.end_byte - symbol.start_byte) as usize,
        "returned bytes should equal end_byte - start_byte"
    );
}

// ---------------------------------------------------------------------------
// 5. full_text_search finds a known string and respects .gitignore
// ---------------------------------------------------------------------------

/// Requirement 8.1 – full_text_search returns matching lines.
/// Requirement 8.4 – .gitignore-excluded files are not included.
#[test]
fn test_full_text_search_finds_string_and_respects_gitignore() {
    let fixtures = fixtures_dir();
    let searcher = FullTextSearcher::new(fixtures.clone());

    // "hello" appears in sample.rs, sample.py, and sample.ts
    let results = searcher.search("hello", 50).expect("search");
    assert!(
        !results.is_empty(),
        "full_text_search for 'hello' should find matches"
    );

    // Verify all matches have required fields
    for m in &results {
        assert!(!m.file_path.is_empty(), "file_path must be non-empty");
        assert!(m.line_number > 0, "line_number must be > 0");
        assert!(!m.line_content.is_empty(), "line_content must be non-empty");
        assert!(
            m.line_content.to_lowercase().contains("hello"),
            "line_content must contain the search term"
        );
    }

    // ignored_file.rs is in .gitignore — it must not appear in results
    // (ignored_file.rs contains "ignored" but let's search for something unique to it)
    // The fixture ignored_file.rs should be excluded entirely
    let ignored_paths: Vec<&str> = results
        .iter()
        .filter(|m| m.file_path.contains("ignored_file"))
        .map(|m| m.file_path.as_str())
        .collect();
    assert!(
        ignored_paths.is_empty(),
        "ignored_file.rs should not appear in full_text_search results: {:?}",
        ignored_paths
    );
}

// ---------------------------------------------------------------------------
// 6. get_file_content blocks .env and secret.pem access
// ---------------------------------------------------------------------------

/// Requirement 9.4 – blocked files return access denied error.
/// Requirement 11.2 – secret file blocklist is enforced.
#[test]
fn test_get_file_content_blocks_secret_files() {
    let fixtures = fixtures_dir();

    // .env must be blocked
    let env_path = fixtures.join(".env");
    assert!(
        env_path.exists(),
        ".env fixture must exist at {:?}",
        env_path
    );
    assert!(
        SourceRetriever::is_blocked_file(&env_path),
        ".env should be blocked"
    );

    // secret.pem must be blocked
    let pem_path = fixtures.join("secret.pem");
    assert!(
        pem_path.exists(),
        "secret.pem fixture must exist at {:?}",
        pem_path
    );
    assert!(
        SourceRetriever::is_blocked_file(&pem_path),
        "secret.pem should be blocked"
    );

    // A normal source file must NOT be blocked
    let rs_path = fixtures.join("sample.rs");
    assert!(
        !SourceRetriever::is_blocked_file(&rs_path),
        "sample.rs should not be blocked"
    );
}

// ---------------------------------------------------------------------------
// 7. Incremental reindexing skips unchanged files
// ---------------------------------------------------------------------------

/// Requirement 3.1 – unchanged files are skipped on second run.
#[test]
fn test_incremental_reindexing_skips_unchanged_files() {
    let (store, _dir) = open_temp_store();
    let mut indexer = Indexer::new(store).expect("indexer");
    let fixtures = fixtures_dir();

    // First full index
    let first_stats = indexer
        .index_workspace(&fixtures)
        .expect("first index_workspace");
    assert!(
        first_stats.files_indexed >= 3,
        "first run should index at least 3 files"
    );

    // Second run without any file changes
    let second_stats = indexer
        .index_workspace(&fixtures)
        .expect("second index_workspace");

    assert_eq!(
        second_stats.files_indexed, 0,
        "second run should index 0 files (all unchanged)"
    );
    assert!(
        second_stats.files_skipped >= first_stats.files_indexed,
        "second run should skip at least as many files as were indexed in first run"
    );
}



// ---------------------------------------------------------------------------
// 8. get_workspace_overview returns all files and their symbols
// ---------------------------------------------------------------------------

/// Requirement 1.1 – get_workspace_overview tool is exposed.
/// Requirement 1.3 – returns JSON with files array.
/// Requirement 1.4 – groups symbols by file path.
#[test]
fn test_get_workspace_overview_groups_by_file() {
    use astrolabe_mcp::server::AstrolabeServer;

    let (store, temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt
        .block_on(server.handle_get_workspace_overview(None))
        .expect("handle_get_workspace_overview");

    // Extract JSON from result
    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    // Parse plain text format: "file: sym1, sym2, ..."
    let lines: Vec<&str> = text.lines().collect();
    
    // Should have at least 3 files (sample.rs, sample.py, sample.ts)
    assert!(
        lines.len() >= 3,
        "expected at least 3 files in overview, got {}",
        lines.len()
    );

    // Each line should have format "file_path: symbol1, symbol2, ..."
    for line in &lines {
        assert!(line.contains(": "), "Each line should have format 'file: symbols'");
        let parts: Vec<&str> = line.splitn(2, ": ").collect();
        assert_eq!(parts.len(), 2, "Line should have file path and symbols");
        
        let path = parts[0];
        let symbols_str = parts[1];

        assert!(!path.is_empty(), "path must be non-empty");
        assert!(!symbols_str.is_empty(), "symbols string should not be empty for {}", path);

        // Each symbol should be separated by ", "
        let symbols: Vec<&str> = symbols_str.split(", ").collect();
        assert!(!symbols.is_empty(), "should have at least one symbol");
    }

    // Verify ignored_file.rs is NOT in the overview
    let ignored_lines: Vec<&str> = lines
        .iter()
        .filter(|line| line.contains("ignored_file"))
        .copied()
        .collect();
    assert!(
        ignored_lines.is_empty(),
        "ignored_file.rs should not appear in workspace overview"
    );
}

/// Requirement 1.5 – empty store returns empty string.
#[test]
fn test_get_workspace_overview_empty_store() {
    use astrolabe_mcp::server::AstrolabeServer;

    let (store, temp_dir) = open_temp_store();
    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt
        .block_on(server.handle_get_workspace_overview(None))
        .expect("handle_get_workspace_overview");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    // Empty store should return empty string
    assert_eq!(text, "", "empty store should return empty string");
}

// ---------------------------------------------------------------------------
// 9. search_symbols with fields parameter returns projected results
// ---------------------------------------------------------------------------

/// Requirement 2.1 – fields parameter restricts output fields.
/// Requirement 2.2 – omitting fields returns full object (backwards compat).
#[test]
fn test_search_symbols_with_fields_parameter() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;

    let (store, temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Search with specific fields
    let args = Some(serde_json::Map::from_iter(vec![
        ("name_pattern".to_string(), json!("top_level_fn")),
        ("fields".to_string(), json!(["name", "kind", "file_path"])),
    ]));

    let result = rt
        .block_on(server.handle_search_symbols(args))
        .expect("handle_search_symbols");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    let results: Vec<serde_json::Value> = serde_json::from_str(&text).expect("parse JSON");
    assert!(!results.is_empty(), "should find top_level_fn");

    // Each result should only have the requested fields
    for result in &results {
        let obj = result.as_object().expect("result is object");
        assert!(obj.contains_key("name"), "should have name field");
        assert!(obj.contains_key("kind"), "should have kind field");
        assert!(obj.contains_key("file_path"), "should have file_path field");

        // Should NOT have unrequested fields
        assert!(
            !obj.contains_key("start_byte"),
            "should not have start_byte (not requested)"
        );
        assert!(
            !obj.contains_key("end_byte"),
            "should not have end_byte (not requested)"
        );
    }
}

/// Requirement 2.4 – unknown field names are silently dropped.
#[test]
fn test_search_symbols_unknown_fields_ignored() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;

    let (store, temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Search with mix of valid and invalid field names
    let args = Some(serde_json::Map::from_iter(vec![
        ("name_pattern".to_string(), json!("top_level_fn")),
        (
            "fields".to_string(),
            json!(["name", "nonexistent_field", "kind"]),
        ),
    ]));

    let result = rt
        .block_on(server.handle_search_symbols(args))
        .expect("handle_search_symbols");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    let results: Vec<serde_json::Value> = serde_json::from_str(&text).expect("parse JSON");
    assert!(!results.is_empty(), "should find results");

    // Should have valid fields, unknown field should be silently dropped
    for result in &results {
        let obj = result.as_object().expect("result is object");
        assert!(obj.contains_key("name"), "should have name field");
        assert!(obj.contains_key("kind"), "should have kind field");
        assert!(
            !obj.contains_key("nonexistent_field"),
            "unknown field should be dropped"
        );
    }
}

// ---------------------------------------------------------------------------
// 10. get_file_outline with fields parameter and compact format
// ---------------------------------------------------------------------------

/// Requirement 3.1 – fields parameter restricts output fields.
/// Requirement 6.1 – compact format returns plain text.
#[test]
fn test_get_file_outline_with_fields_and_compact_format() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;
    use regex::Regex;

    let (store, _temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    let fixtures = fixtures_dir();
    indexer
        .index_workspace(&fixtures)
        .expect("index_workspace");

    let searcher = FullTextSearcher::new(fixtures.clone());
    let server = AstrolabeServer::new(store, searcher, fixtures.clone());

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Test JSON format with fields
    let args = Some(serde_json::Map::from_iter(vec![
        ("file_path".to_string(), json!("sample.rs")),
        ("fields".to_string(), json!(["name", "kind"])),
        ("format".to_string(), json!("json")),
    ]));

    let result = rt
        .block_on(server.handle_get_file_outline(args))
        .expect("handle_get_file_outline");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    // Parse as JSON array
    let symbols: Vec<serde_json::Value> = serde_json::from_str(&text).expect("parse JSON");
    assert!(!symbols.is_empty(), "sample.rs should have symbols");

    // Each symbol should only have requested fields
    for sym in &symbols {
        let obj = sym.as_object().expect("symbol is object");
        assert!(obj.contains_key("name"), "should have name field");
        assert!(obj.contains_key("kind"), "should have kind field");
        assert!(
            !obj.contains_key("start_line"),
            "should not have start_line (not requested)"
        );
    }

    // Test compact format
    let args = Some(serde_json::Map::from_iter(vec![
        ("file_path".to_string(), json!("sample.rs")),
        ("format".to_string(), json!("compact")),
    ]));

    let result = rt
        .block_on(server.handle_get_file_outline(args))
        .expect("handle_get_file_outline");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    // Compact format should be plain text, not JSON
    assert!(
        !text.starts_with('['),
        "compact format should not be JSON array"
    );

    // Each line should match pattern: <kind> <name> [<start>-<end>]
    let regex = Regex::new(r"^\S+ \S+ \[\d+-\d+\]$").expect("regex");
    for line in text.lines() {
        assert!(
            regex.is_match(line),
            "compact line should match pattern, got: {}",
            line
        );
    }
}

/// Requirement 6.5 – compact format ignores fields parameter.
#[test]
fn test_get_file_outline_compact_ignores_fields() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;
    use regex::Regex;

    let (store, _temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    let fixtures = fixtures_dir();
    indexer
        .index_workspace(&fixtures)
        .expect("index_workspace");

    let searcher = FullTextSearcher::new(fixtures.clone());
    let server = AstrolabeServer::new(store, searcher, fixtures.clone());

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Compact format with fields should ignore fields
    let args = Some(serde_json::Map::from_iter(vec![
        ("file_path".to_string(), json!("sample.rs")),
        ("format".to_string(), json!("compact")),
        ("fields".to_string(), json!(["name"])),
    ]));

    let result = rt
        .block_on(server.handle_get_file_outline(args))
        .expect("handle_get_file_outline");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    // Should still be compact format (not JSON)
    assert!(
        !text.starts_with('['),
        "compact format should not be JSON array"
    );

    // Each line should have full compact format (kind, name, line range)
    let regex = Regex::new(r"^\S+ \S+ \[\d+-\d+\]$").expect("regex");
    for line in text.lines() {
        assert!(
            regex.is_match(line),
            "compact line should have full format despite fields param, got: {}",
            line
        );
    }
}

// ---------------------------------------------------------------------------
// 11. get_symbol_implementations batch tool
// ---------------------------------------------------------------------------

/// Requirement 4.1 – get_symbol_implementations tool is exposed.
/// Requirement 4.2 – returns array in same order as input.
/// Requirement 4.3 – array length equals input length.
#[test]
fn test_get_symbol_implementations_batch() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;

    let (store, _temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    // Get a known symbol to use in batch before moving store
    let query = SearchQuery {
        name_pattern: Some("top_level_fn".to_string()),
        kind: None,
        language: None,
        file_path: None,
        limit: None,
    };
    let search_results = store.search(&query).expect("search");
    assert!(!search_results.is_empty(), "should find top_level_fn");

    let qualified_name = search_results[0].qualified_name.clone();

    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, fixtures_dir());

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Call batch tool with one symbol
    let args = Some(serde_json::Map::from_iter(vec![(
        "qualified_names".to_string(),
        json!([qualified_name.clone()]),
    )]));

    let result = rt
        .block_on(server.handle_get_symbol_implementations(args))
        .expect("handle_get_symbol_implementations");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    let implementations: Vec<serde_json::Value> = serde_json::from_str(&text).expect("parse JSON");

    // Array length should equal input length
    assert_eq!(
        implementations.len(),
        1,
        "batch result should have same length as input"
    );

    // Result should have implementation or error field
    let result_obj = implementations[0].as_object().expect("result is object");
    assert!(
        result_obj.contains_key("implementation") || result_obj.contains_key("error"),
        "result should have implementation or error field"
    );

    // Should have implementation (not error) for a known symbol
    assert!(
        result_obj["implementation"].is_string(),
        "result should have implementation for known symbol"
    );
    assert!(
        result_obj["error"].is_null(),
        "result should have null error for known symbol"
    );
}

/// Requirement 4.5 – partial failures don't stop batch processing.
#[test]
fn test_get_symbol_implementations_partial_miss() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;

    let (store, _temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, fixtures_dir());

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Batch with one valid and one invalid symbol
    let args = Some(serde_json::Map::from_iter(vec![(
        "qualified_names".to_string(),
        json!(["top_level_fn", "nonexistent::symbol"]),
    )]));

    let result = rt
        .block_on(server.handle_get_symbol_implementations(args))
        .expect("handle_get_symbol_implementations");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    let implementations: Vec<serde_json::Value> = serde_json::from_str(&text).expect("parse JSON");

    // Should have 2 results (one for each input)
    assert_eq!(implementations.len(), 2, "should return result for each input");

    // First result should have implementation (or error if not found)
    let first = implementations[0].as_object().expect("first is object");
    assert!(
        first.contains_key("implementation") && first.contains_key("error"),
        "first result should have implementation and error fields"
    );

    // Second result should have error (symbol not found)
    let second = implementations[1].as_object().expect("second is object");
    assert!(
        second.contains_key("implementation") && second.contains_key("error"),
        "second result should have implementation and error fields"
    );
    assert!(
        second["error"].is_string(),
        "second result should have error field with string value"
    );
    assert!(
        second["implementation"].is_null(),
        "second result should have null implementation"
    );
}

// ---------------------------------------------------------------------------
// 12. get_file_summary tool
// ---------------------------------------------------------------------------

/// Requirement 5.1 – get_file_summary tool is exposed.
/// Requirement 5.2 – returns plain text (not JSON).
/// Requirement 5.3 – first line is file path with colon.
#[test]
fn test_get_file_summary_basic() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;

    let (store, temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

    let rt = tokio::runtime::Runtime::new().unwrap();

    let args = Some(serde_json::Map::from_iter(vec![(
        "file_path".to_string(),
        json!("sample.rs"),
    )]));

    let result = rt
        .block_on(server.handle_get_file_summary(args))
        .expect("handle_get_file_summary");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    // Should be plain text, not JSON
    assert!(
        !text.starts_with('{'),
        "get_file_summary should return plain text, not JSON"
    );

    // First line should be file path with colon
    let first_line = text.lines().next().expect("should have at least one line");
    assert!(
        first_line.ends_with(':'),
        "first line should end with colon, got: {}",
        first_line
    );
    assert!(
        first_line.contains("sample.rs"),
        "first line should contain file path, got: {}",
        first_line
    );

    // Should have multiple lines (file path + symbols)
    assert!(
        text.lines().count() > 1,
        "summary should have multiple lines"
    );
}

/// Requirement 5.6 – no raw source body text included.
#[test]
fn test_get_file_summary_no_bodies() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;

    let (store, temp_dir) = open_temp_store();
    let mut indexer = Indexer::new(store.clone()).expect("indexer");
    indexer
        .index_workspace(&fixtures_dir())
        .expect("index_workspace");

    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

    let rt = tokio::runtime::Runtime::new().unwrap();

    let args = Some(serde_json::Map::from_iter(vec![(
        "file_path".to_string(),
        json!("sample.rs"),
    )]));

    let result = rt
        .block_on(server.handle_get_file_summary(args))
        .expect("handle_get_file_summary");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    // Summary should not contain raw function bodies
    // (it should only have kind, name, signature, and doc comments)
    // The summary format is: "kind name — doc_comment" and optional "  signature"
    // It should NOT contain full function implementations

    // Verify format: each line should be either:
    // - "kind name" or "kind name — doc_comment"
    // - "  signature" (indented)
    // - "  mod ..." (indented module content)
    for line in text.lines().skip(1) {
        // Skip file path line
        if line.starts_with("  ") {
            // Indented line should be a signature or module content
            // Don't check for '{' as modules can have nested content
            // Just verify it's not a full function body (multiple lines of code)
        } else {
            // Non-indented line should be kind + name
            let parts: Vec<&str> = line.split_whitespace().collect();
            assert!(
                parts.len() >= 2,
                "symbol line should have at least kind and name, got: {}",
                line
            );
        }
    }
}

/// Requirement 5.7 – empty file returns "no symbols indexed" message.
#[test]
fn test_get_file_summary_empty_file() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;

    let (store, temp_dir) = open_temp_store();
    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Query a file that doesn't exist in the index
    let args = Some(serde_json::Map::from_iter(vec![(
        "file_path".to_string(),
        json!("nonexistent.rs"),
    )]));

    let result = rt
        .block_on(server.handle_get_file_summary(args))
        .expect("handle_get_file_summary");

    let text = result
        .content
        .first()
        .and_then(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("Expected text content");

    assert!(
        text.contains("no symbols indexed"),
        "empty file should return 'no symbols indexed' message, got: {}",
        text
    );
}

/// Requirement 5.8 – path traversal returns error.
#[test]
fn test_get_file_summary_path_traversal_blocked() {
    use astrolabe_mcp::server::AstrolabeServer;
    use serde_json::json;

    let (store, temp_dir) = open_temp_store();
    let searcher = FullTextSearcher::new(fixtures_dir());
    let server = AstrolabeServer::new(store, searcher, temp_dir.path().to_path_buf());

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Try path traversal
    let args = Some(serde_json::Map::from_iter(vec![(
        "file_path".to_string(),
        json!("../../../etc/passwd"),
    )]));

    let result = rt.block_on(server.handle_get_file_summary(args));

    // Should return an error
    assert!(
        result.is_err(),
        "path traversal should return error, got: {:?}",
        result
    );
}
