/// Property-based tests for SymbolStore functionality
/// Validates: Property 2 — Requirements 7.2
use astrolabe_mcp::models::{Symbol, SymbolKind};
use astrolabe_mcp::store::SymbolStore;
use proptest::prelude::*;
use std::path::Path;
use tempfile::TempDir;

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

// ========================================================================
// 8.2 prop_get_all_file_symbols_ordering
// ========================================================================
// **Validates: Property 2 — Requirements 7.2**

proptest! {
    #[test]
    fn prop_get_all_file_symbols_ordering(
        symbols in prop::collection::vec(arb_symbol(), 1..20)
    ) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = SymbolStore::open(db_path.to_string_lossy().as_ref()).unwrap();

        for symbol in &symbols {
            store.upsert_symbols(
                Path::new(&symbol.file_path),
                std::slice::from_ref(symbol)
            ).unwrap();
        }

        let result = store.get_all_file_symbols().unwrap();

        for i in 0..result.len().saturating_sub(1) {
            let curr = &result[i];
            let next = &result[i + 1];

            if curr.file_path == next.file_path {
                prop_assert!(curr.start_line <= next.start_line);
            } else {
                prop_assert!(curr.file_path < next.file_path);
            }
        }
    }
}

// ========================================================================
// Additional property tests from store_unit_tests
// ========================================================================

use astrolabe_mcp::models::SearchQuery;
use tempfile::NamedTempFile;

fn create_test_store() -> (astrolabe_mcp::store::SymbolStore, String) {
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path().to_string_lossy().to_string();
    let store = SymbolStore::open(&db_path).unwrap();
    (store, db_path)
}

proptest! {
    /// Property 12: search_symbols result limit
    #[test]
    fn prop_search_result_limit(
        symbols_count in 1usize..50,
        limit in prop_oneof![Just(None), (1usize..150).prop_map(Some)]
    ) {
        let (store, _db_path) = create_test_store();
        let file_path = Path::new("test.rs");

        let mut symbols = Vec::new();
        for i in 0..symbols_count {
            symbols.push(Symbol {
                id: 0,
                qualified_name: format!("func_{}", i),
                name: format!("func_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                signature: format!("fn func_{}() {{}}", i),
                summary: String::new(),
                file_path: "test.rs".to_string(),
                start_byte: i as u64 * 20,
                end_byte: (i as u64 + 1) * 20,
                start_line: i as u32,
                end_line: i as u32,
            });
        }

        store.upsert_symbols(file_path, &symbols).unwrap();

        let query = SearchQuery {
            name_pattern: None,
            kind: None,
            language: None,
            file_path: None,
            limit,
        };

        let results = store.search(&query).unwrap();
        let expected_max = limit.map(|l| l.min(100)).unwrap_or(20);
        let actual_max = expected_max.min(symbols_count);

        prop_assert!(
            results.len() <= actual_max,
            "Results count {} exceeds expected max {}",
            results.len(),
            actual_max
        );
    }

    /// Property 13: search_symbols filter correctness
    #[test]
    fn prop_search_filter_correctness(
        kind_filter in prop_oneof![
            Just(None),
            Just(Some(SymbolKind::Function)),
            Just(Some(SymbolKind::Struct)),
        ],
        language_filter in prop_oneof![
            Just(None),
            Just(Some("rust".to_string())),
            Just(Some("python".to_string())),
        ]
    ) {
        let (store, _db_path) = create_test_store();
        let file_path = Path::new("test.rs");

        let symbols = vec![
            Symbol {
                id: 0,
                qualified_name: "func1".to_string(),
                name: "func1".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                signature: "fn func1() {}".to_string(),
                summary: String::new(),
                file_path: "test.rs".to_string(),
                start_byte: 0,
                end_byte: 20,
                start_line: 1,
                end_line: 1,
            },
            Symbol {
                id: 0,
                qualified_name: "MyStruct".to_string(),
                name: "MyStruct".to_string(),
                kind: SymbolKind::Struct,
                language: "rust".to_string(),
                signature: "struct MyStruct {}".to_string(),
                summary: String::new(),
                file_path: "test.rs".to_string(),
                start_byte: 20,
                end_byte: 40,
                start_line: 2,
                end_line: 2,
            },
            Symbol {
                id: 0,
                qualified_name: "func2".to_string(),
                name: "func2".to_string(),
                kind: SymbolKind::Function,
                language: "python".to_string(),
                signature: "def func2(): pass".to_string(),
                summary: String::new(),
                file_path: "test.py".to_string(),
                start_byte: 0,
                end_byte: 20,
                start_line: 1,
                end_line: 1,
            },
        ];

        store.upsert_symbols(file_path, &symbols).unwrap();

        let query = SearchQuery {
            name_pattern: None,
            kind: kind_filter,
            language: language_filter.clone(),
            file_path: None,
            limit: None,
        };

        let results = store.search(&query).unwrap();

        for result in &results {
            if let Some(k) = kind_filter {
                prop_assert_eq!(result.kind, k);
            }
            if let Some(ref l) = language_filter {
                prop_assert_eq!(&result.language, l);
            }
        }
    }

    /// Property 11: Transactional symbol upsert
    #[test]
    fn prop_transactional_upsert(symbols_count in 1usize..20) {
        let (store, _db_path) = create_test_store();
        let file_path = Path::new("test.rs");

        let mut symbols = Vec::new();
        for i in 0..symbols_count {
            symbols.push(Symbol {
                id: 0,
                qualified_name: format!("func_{}", i),
                name: format!("func_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                signature: format!("fn func_{}() {{}}", i),
                summary: String::new(),
                file_path: "test.rs".to_string(),
                start_byte: i as u64 * 20,
                end_byte: (i as u64 + 1) * 20,
                start_line: i as u32,
                end_line: i as u32,
            });
        }

        store.upsert_symbols(file_path, &symbols).unwrap();

        let retrieved = store.get_file_symbols(file_path).unwrap();
        prop_assert_eq!(retrieved.len(), symbols_count);

        let new_symbols = vec![Symbol {
            id: 0,
            qualified_name: "new_func".to_string(),
            name: "new_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            signature: "fn new_func() {}".to_string(),
            summary: String::new(),
            file_path: "test.rs".to_string(),
            start_byte: 0,
            end_byte: 20,
            start_line: 1,
            end_line: 1,
        }];

        store.upsert_symbols(file_path, &new_symbols).unwrap();

        let retrieved = store.get_file_symbols(file_path).unwrap();
        prop_assert_eq!(retrieved.len(), 1);
        prop_assert_eq!(&retrieved[0].name, "new_func");
    }
}
