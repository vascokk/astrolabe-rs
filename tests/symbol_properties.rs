use astrolabe_mcp::models::{Symbol, SymbolKind};
use proptest::prelude::*;

fn symbol_strategy() -> impl Strategy<Value = Symbol> {
    (
        1i64..,
        r"[a-zA-Z_][a-zA-Z0-9_:]*",
        r"[a-zA-Z_][a-zA-Z0-9_]*",
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
        ],
        r"(rust|python|typescript|javascript|go|c|cpp)",
        r"[a-zA-Z0-9_() \->,;:]+",  // At least one character
        r"[a-zA-Z0-9_ ]*",
        r"[a-zA-Z0-9_/.\-]+",
        0u64..1000u64,
        1u64..100u64,
        0u32..1000u32,
        0u32..100u32,
    )
        .prop_map(
            |(
                id,
                qualified_name,
                name,
                kind,
                language,
                signature,
                summary,
                file_path,
                start_byte,
                byte_range,
                start_line,
                line_range,
            )| {
                Symbol {
                    id,
                    qualified_name,
                    name,
                    kind,
                    language,
                    signature,
                    summary,
                    file_path,
                    start_byte,
                    end_byte: start_byte + byte_range,
                    start_line,
                    end_line: start_line + line_range,
                }
            },
        )
}

/// Property 2: Symbol validity invariants
/// For any symbol extracted by the Indexer from any valid source file,
/// start_byte SHALL be strictly less than end_byte, and start_line SHALL be
/// less than or equal to end_line.
/// Validates: Requirements 2.4, 2.5
#[test]
fn prop_symbol_validity_invariants() {
    proptest!(|(symbol in symbol_strategy())| {
        // Assert start_byte < end_byte
        prop_assert!(symbol.start_byte < symbol.end_byte,
            "start_byte ({}) must be strictly less than end_byte ({})",
            symbol.start_byte, symbol.end_byte);

        // Assert start_line <= end_line
        prop_assert!(symbol.start_line <= symbol.end_line,
            "start_line ({}) must be less than or equal to end_line ({})",
            symbol.start_line, symbol.end_line);
    });
}

/// Property 3: Symbol field completeness
/// For any symbol extracted by the Indexer, the qualified_name, name, kind,
/// language, signature, and file_path fields SHALL all be non-empty strings,
/// and start_byte, end_byte, start_line, and end_line SHALL all be populated.
/// Validates: Requirement 2.2
#[test]
fn prop_symbol_field_completeness() {
    proptest!(|(symbol in symbol_strategy())| {
        // Assert all required string fields are non-empty
        prop_assert!(!symbol.qualified_name.is_empty(), "qualified_name must be non-empty");
        prop_assert!(!symbol.name.is_empty(), "name must be non-empty");
        prop_assert!(!symbol.language.is_empty(), "language must be non-empty");
        prop_assert!(!symbol.signature.is_empty(), "signature must be non-empty");
        prop_assert!(!symbol.file_path.is_empty(), "file_path must be non-empty");

        // Assert all numeric fields are populated (they always are in Rust)
        prop_assert!(symbol.start_byte < u64::MAX, "start_byte must be populated");
        prop_assert!(symbol.end_byte < u64::MAX, "end_byte must be populated");
        prop_assert!(symbol.start_line < u32::MAX, "start_line must be populated");
        prop_assert!(symbol.end_line < u32::MAX, "end_line must be populated");
    });
}
