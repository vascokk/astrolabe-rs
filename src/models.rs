use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Represents a code symbol extracted from source files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: i64,
    pub qualified_name: String,  // e.g. "my_mod::MyStruct::my_func"
    pub name: String,            // leaf name: "my_func"
    pub kind: SymbolKind,
    pub language: String,        // "rust", "python", "typescript", ...
    pub signature: String,       // e.g. "pub fn my_func(x: u32) -> String"
    pub summary: String,         // first doc-comment line or empty
    pub file_path: String,       // workspace-relative path
    pub start_byte: u64,
    pub end_byte: u64,
    pub start_line: u32,
    pub end_line: u32,
}

impl Symbol {
    /// Validates that the symbol has valid byte and line ranges
    pub fn validate(&self) -> Result<(), String> {
        if self.qualified_name.is_empty() {
            return Err("qualified_name must be non-empty".to_string());
        }
        if self.start_byte >= self.end_byte {
            return Err("start_byte must be strictly less than end_byte".to_string());
        }
        if self.start_line > self.end_line {
            return Err("start_line must be less than or equal to end_line".to_string());
        }
        Ok(())
    }
}

/// Enumeration of symbol kinds
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Const,
    TypeAlias,
    Method,
    Field,
    Variable,
    Class,
    Interface,
}

/// Query parameters for searching symbols
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub name_pattern: Option<String>,  // substring or glob
    pub kind: Option<SymbolKind>,
    pub language: Option<String>,
    pub file_path: Option<String>,
    pub limit: Option<usize>,  // default 20, max 100
}

/// Represents a text match from full-text search
#[derive(Debug, Serialize)]
pub struct TextMatch {
    pub file_path: String,
    pub line_number: u32,
    pub line_content: String,
    pub column_start: u32,
}

/// Statistics about an indexing run
#[derive(Debug, Serialize, Default)]
pub struct IndexStats {
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub symbols_total: usize,
    pub duration_ms: u64,
}

/// Enumeration of valid Symbol fields for projection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolField {
    Id,
    QualifiedName,
    Name,
    Kind,
    Language,
    Signature,
    Summary,
    FilePath,
    StartByte,
    EndByte,
    StartLine,
    EndLine,
}

impl std::str::FromStr for SymbolField {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "id" => Ok(SymbolField::Id),
            "qualified_name" => Ok(SymbolField::QualifiedName),
            "name" => Ok(SymbolField::Name),
            "kind" => Ok(SymbolField::Kind),
            "language" => Ok(SymbolField::Language),
            "signature" => Ok(SymbolField::Signature),
            "summary" => Ok(SymbolField::Summary),
            "file_path" => Ok(SymbolField::FilePath),
            "start_byte" => Ok(SymbolField::StartByte),
            "end_byte" => Ok(SymbolField::EndByte),
            "start_line" => Ok(SymbolField::StartLine),
            "end_line" => Ok(SymbolField::EndLine),
            _ => Err(format!("Unknown field: {}", s)),
        }
    }
}

impl SymbolField {
    /// Get the JSON key string for this field
    pub fn key(&self) -> &'static str {
        match self {
            SymbolField::Id => "id",
            SymbolField::QualifiedName => "qualified_name",
            SymbolField::Name => "name",
            SymbolField::Kind => "kind",
            SymbolField::Language => "language",
            SymbolField::Signature => "signature",
            SymbolField::Summary => "summary",
            SymbolField::FilePath => "file_path",
            SymbolField::StartByte => "start_byte",
            SymbolField::EndByte => "end_byte",
            SymbolField::StartLine => "start_line",
            SymbolField::EndLine => "end_line",
        }
    }

    /// Get all SymbolField variants
    #[allow(dead_code)]
    pub fn all() -> &'static [Self] {
        &[
            SymbolField::Id,
            SymbolField::QualifiedName,
            SymbolField::Name,
            SymbolField::Kind,
            SymbolField::Language,
            SymbolField::Signature,
            SymbolField::Summary,
            SymbolField::FilePath,
            SymbolField::StartByte,
            SymbolField::EndByte,
            SymbolField::StartLine,
            SymbolField::EndLine,
        ]
    }
}
