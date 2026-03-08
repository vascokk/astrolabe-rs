use crate::models::{Symbol, SymbolKind, SearchQuery};
use crate::retry::{retry_with_backoff_sync, RetryConfig};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// SQLite-backed persistence layer for symbol metadata
#[derive(Clone)]
pub struct SymbolStore {
    conn: Arc<Mutex<Connection>>,
}

impl SymbolStore {
    /// Opens or creates a SQLite database at the given path
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = SymbolStore {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.run_migrations()?;
        Ok(store)
    }

    /// Creates a SymbolStore from an existing connection
    #[allow(dead_code)]
    pub fn from_connection(conn: Connection) -> Self {
        SymbolStore {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Creates the database schema and runs migrations
    pub fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Enable WAL mode for concurrent read access
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        // Create files table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                path        TEXT PRIMARY KEY,
                mtime       INTEGER NOT NULL,
                indexed_at  INTEGER NOT NULL
            )",
            [],
        )?;

        // Create symbols table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS symbols (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                qualified_name TEXT NOT NULL,
                name           TEXT NOT NULL,
                kind           TEXT NOT NULL,
                language       TEXT NOT NULL,
                signature      TEXT NOT NULL,
                summary        TEXT NOT NULL DEFAULT '',
                file_path      TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                start_byte     INTEGER NOT NULL,
                end_byte       INTEGER NOT NULL,
                start_line     INTEGER NOT NULL,
                end_line       INTEGER NOT NULL
            )",
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_language ON symbols(language)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path)",
            [],
        )?;
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_symbols_qname_file
                ON symbols(qualified_name, file_path)",
            [],
        )?;

        // Create FTS5 virtual table for fast name search
        // Note: FTS5 doesn't support IF NOT EXISTS, so we check first
        let fts_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='symbols_fts'",
                [],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        if !fts_exists {
            conn.execute(
                "CREATE VIRTUAL TABLE symbols_fts USING fts5(
                    qualified_name,
                    signature,
                    summary,
                    content='symbols',
                    content_rowid='id'
                )",
                [],
            )?;
        }

        Ok(())
    }

    /// Upserts symbols for a file in a single transaction
    /// Upserts symbols for a file in a single transaction
    pub fn upsert_symbols(&self, file_path: &Path, symbols: &[Symbol]) -> Result<()> {
        let file_path_str = file_path.to_string_lossy().to_string();
        let symbols_clone: Vec<Symbol> = symbols.to_vec();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        retry_with_backoff_sync(
            move || {
                let mut conn = self.conn.lock().unwrap();
                let tx = conn.transaction()?;

                // Ensure file entry exists
                tx.execute(
                    "INSERT OR IGNORE INTO files (path, mtime, indexed_at) VALUES (?1, ?2, ?3)",
                    params![&file_path_str, 0, now],
                )?;

                // Delete old symbols for this file
                tx.execute(
                    "DELETE FROM symbols WHERE file_path = ?1",
                    params![&file_path_str],
                )?;

                // Insert new symbols
                for symbol in &symbols_clone {
                    symbol.validate().map_err(|e| {
                        rusqlite::Error::ToSqlConversionFailure(
                            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                        )
                    })?;

                    tx.execute(
                        "INSERT OR IGNORE INTO symbols (
                            qualified_name, name, kind, language, signature, summary,
                            file_path, start_byte, end_byte, start_line, end_line
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                        params![
                            &symbol.qualified_name,
                            &symbol.name,
                            format!("{:?}", symbol.kind).to_lowercase(),
                            &symbol.language,
                            &symbol.signature,
                            &symbol.summary,
                            &file_path_str,
                            symbol.start_byte,
                            symbol.end_byte,
                            symbol.start_line,
                            symbol.end_line,
                        ],
                    )?;
                }

                tx.commit()?;
                Ok(())
            },
            RetryConfig::default(),
        )?;

        Ok(())
    }

    /// Deletes all symbols for a file
    pub fn delete_file_symbols(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();

        retry_with_backoff_sync(
            || {
                let conn = self.conn.lock().unwrap();
                conn.execute(
                    "DELETE FROM symbols WHERE file_path = ?1",
                    params![&path_str],
                )?;
                Ok(())
            },
            RetryConfig::default(),
        )?;

        Ok(())
    }

    /// Gets the stored mtime for a file, if it exists
    pub fn get_indexed_mtime(&self, path: &Path) -> Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let path_str = path.to_string_lossy().to_string();

        let mtime: Option<u64> = conn
            .query_row(
                "SELECT mtime FROM files WHERE path = ?1",
                params![&path_str],
                |row| row.get(0),
            )
            .optional()?;

        Ok(mtime)
    }

    /// Sets the mtime for a file
    /// Sets the mtime for a file
    pub fn set_indexed_mtime(&self, path: &Path, mtime: u64) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        retry_with_backoff_sync(
            move || {
                let conn = self.conn.lock().unwrap();

                // Use INSERT OR IGNORE + UPDATE to avoid triggering ON DELETE CASCADE
                // which would wipe symbols if we used INSERT OR REPLACE
                conn.execute(
                    "INSERT OR IGNORE INTO files (path, mtime, indexed_at) VALUES (?1, ?2, ?3)",
                    params![&path_str, mtime, now],
                )?;
                conn.execute(
                    "UPDATE files SET mtime = ?1, indexed_at = ?2 WHERE path = ?3",
                    params![mtime, now, &path_str],
                )?;

                Ok(())
            },
            RetryConfig::default(),
        )?;

        Ok(())
    }

    /// Searches for symbols matching the query
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();

        // Determine limit (default 20, max 100)
        let limit = query
            .limit
            .map(|l| l.min(100))
            .unwrap_or(20)
            .max(1);

        let mut sql = String::from(
            "SELECT id, qualified_name, name, kind, language, signature, summary,
                    file_path, start_byte, end_byte, start_line, end_line
             FROM symbols WHERE 1=1",
        );

        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Add name_pattern filter using LIKE for substring matching
        if let Some(ref pattern) = query.name_pattern {
            sql.push_str(" AND (qualified_name LIKE ? OR name LIKE ?)");
            let like_pattern = format!("%{}%", pattern);
            params.push(Box::new(like_pattern.clone()));
            params.push(Box::new(like_pattern));
        }

        // Add kind filter
        if let Some(kind) = query.kind {
            sql.push_str(" AND kind = ?");
            params.push(Box::new(format!("{:?}", kind).to_lowercase()));
        }

        // Add language filter
        if let Some(ref lang) = query.language {
            sql.push_str(" AND language = ?");
            params.push(Box::new(lang.clone()));
        }

        // Add file_path filter
        if let Some(ref file_path) = query.file_path {
            sql.push_str(" AND file_path = ?");
            params.push(Box::new(file_path.clone()));
        }

        sql.push_str(" LIMIT ?");
        params.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&sql)?;

        // Convert params to rusqlite::params
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let symbols = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(Symbol {
                id: row.get(0)?,
                qualified_name: row.get(1)?,
                name: row.get(2)?,
                kind: parse_symbol_kind(&row.get::<_, String>(3)?),
                language: row.get(4)?,
                signature: row.get(5)?,
                summary: row.get(6)?,
                file_path: row.get(7)?,
                start_byte: row.get(8)?,
                end_byte: row.get(9)?,
                start_line: row.get(10)?,
                end_line: row.get(11)?,
            })
        })?;

        let mut result = Vec::new();
        for symbol in symbols {
            result.push(symbol?);
        }

        Ok(result)
    }

    /// Gets a symbol by its qualified name
    pub fn get_by_qualified_name(&self, name: &str) -> Result<Option<Symbol>> {
        let conn = self.conn.lock().unwrap();

        let symbol = conn
            .query_row(
                "SELECT id, qualified_name, name, kind, language, signature, summary,
                        file_path, start_byte, end_byte, start_line, end_line
                 FROM symbols WHERE qualified_name = ?1 LIMIT 1",
                params![name],
                |row| {
                    Ok(Symbol {
                        id: row.get(0)?,
                        qualified_name: row.get(1)?,
                        name: row.get(2)?,
                        kind: parse_symbol_kind(&row.get::<_, String>(3)?),
                        language: row.get(4)?,
                        signature: row.get(5)?,
                        summary: row.get(6)?,
                        file_path: row.get(7)?,
                        start_byte: row.get(8)?,
                        end_byte: row.get(9)?,
                        start_line: row.get(10)?,
                        end_line: row.get(11)?,
                    })
                },
            )
            .optional()?;

        Ok(symbol)
    }

    /// Gets all symbols for a file, sorted by start_line ascending
    pub fn get_file_symbols(&self, path: &Path) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();
        let path_str = path.to_string_lossy().to_string();

        let mut stmt = conn.prepare(
            "SELECT id, qualified_name, name, kind, language, signature, summary,
                    file_path, start_byte, end_byte, start_line, end_line
             FROM symbols WHERE file_path = ?1 ORDER BY start_line ASC",
        )?;

        let symbols = stmt.query_map(params![&path_str], |row| {
            Ok(Symbol {
                id: row.get(0)?,
                qualified_name: row.get(1)?,
                name: row.get(2)?,
                kind: parse_symbol_kind(&row.get::<_, String>(3)?),
                language: row.get(4)?,
                signature: row.get(5)?,
                summary: row.get(6)?,
                file_path: row.get(7)?,
                start_byte: row.get(8)?,
                end_byte: row.get(9)?,
                start_line: row.get(10)?,
                end_line: row.get(11)?,
            })
        })?;

        let mut result = Vec::new();
        for symbol in symbols {
            result.push(symbol?);
        }

        Ok(result)
    }

    pub fn get_all_file_symbols(&self) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, qualified_name, name, kind, language, signature, summary,
                    file_path, start_byte, end_byte, start_line, end_line
             FROM symbols ORDER BY file_path ASC, start_line ASC",
        )?;

        let symbols = stmt.query_map([], |row| {
            Ok(Symbol {
                id: row.get(0)?,
                qualified_name: row.get(1)?,
                name: row.get(2)?,
                kind: parse_symbol_kind(&row.get::<_, String>(3)?),
                language: row.get(4)?,
                signature: row.get(5)?,
                summary: row.get(6)?,
                file_path: row.get(7)?,
                start_byte: row.get(8)?,
                end_byte: row.get(9)?,
                start_line: row.get(10)?,
                end_line: row.get(11)?,
            })
        })?;

        let mut result = Vec::new();
        for symbol in symbols {
            result.push(symbol?);
        }

        Ok(result)
    }
}

/// Helper function to parse SymbolKind from string
fn parse_symbol_kind(s: &str) -> SymbolKind {
    match s {
        "function" => SymbolKind::Function,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "impl" => SymbolKind::Impl,
        "module" => SymbolKind::Module,
        "const" => SymbolKind::Const,
        "typealias" => SymbolKind::TypeAlias,
        "method" => SymbolKind::Method,
        "field" => SymbolKind::Field,
        "variable" => SymbolKind::Variable,
        "class" => SymbolKind::Class,
        "interface" => SymbolKind::Interface,
        _ => SymbolKind::Function, // default fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_symbol_store_open_and_migrations() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_string_lossy().to_string();

        let store = SymbolStore::open(&db_path)?;

        // Verify tables exist by querying them
        let conn = store.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('files', 'symbols')")?;
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        assert!(tables.contains(&"files".to_string()));
        assert!(tables.contains(&"symbols".to_string()));

        Ok(())
    }

    #[test]
    fn test_upsert_and_retrieve_symbols() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_string_lossy().to_string();
        let store = SymbolStore::open(&db_path)?;

        let file_path = Path::new("test.rs");
        let symbols = vec![Symbol {
            id: 0,
            qualified_name: "my_func".to_string(),
            name: "my_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            signature: "fn my_func() {}".to_string(),
            summary: "A test function".to_string(),
            file_path: "test.rs".to_string(),
            start_byte: 0,
            end_byte: 20,
            start_line: 1,
            end_line: 1,
        }];

        store.upsert_symbols(file_path, &symbols)?;

        let retrieved = store.get_file_symbols(file_path)?;
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].name, "my_func");

        Ok(())
    }

    #[test]
    fn test_search_with_limit() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_string_lossy().to_string();
        let store = SymbolStore::open(&db_path)?;

        let file_path = Path::new("test.rs");
        let mut symbols = Vec::new();
        for i in 0..30 {
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

        store.upsert_symbols(file_path, &symbols)?;

        // Test default limit (20)
        let query = SearchQuery {
            name_pattern: None,
            kind: None,
            language: None,
            file_path: None,
            limit: None,
        };
        let results = store.search(&query)?;
        assert_eq!(results.len(), 20);

        // Test custom limit
        let query = SearchQuery {
            name_pattern: None,
            kind: None,
            language: None,
            file_path: None,
            limit: Some(10),
        };
        let results = store.search(&query)?;
        assert_eq!(results.len(), 10);

        // Test limit capping at 100
        let query = SearchQuery {
            name_pattern: None,
            kind: None,
            language: None,
            file_path: None,
            limit: Some(200),
        };
        let results = store.search(&query)?;
        assert_eq!(results.len(), 30); // Only 30 symbols total

        Ok(())
    }

    #[test]
    fn test_search_with_filters() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_string_lossy().to_string();
        let store = SymbolStore::open(&db_path)?;

        let file_path = Path::new("test.rs");
        let symbols = vec![
            Symbol {
                id: 0,
                qualified_name: "my_func".to_string(),
                name: "my_func".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                signature: "fn my_func() {}".to_string(),
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
        ];

        store.upsert_symbols(file_path, &symbols)?;

        // Filter by kind
        let query = SearchQuery {
            name_pattern: None,
            kind: Some(SymbolKind::Function),
            language: None,
            file_path: None,
            limit: None,
        };
        let results = store.search(&query)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, SymbolKind::Function);

        // Filter by language
        let query = SearchQuery {
            name_pattern: None,
            kind: None,
            language: Some("rust".to_string()),
            file_path: None,
            limit: None,
        };
        let results = store.search(&query)?;
        assert_eq!(results.len(), 2);

        Ok(())
    }

    #[test]
    fn test_get_by_qualified_name() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_string_lossy().to_string();
        let store = SymbolStore::open(&db_path)?;

        let file_path = Path::new("test.rs");
        let symbols = vec![Symbol {
            id: 0,
            qualified_name: "my_mod::MyStruct::my_func".to_string(),
            name: "my_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            signature: "fn my_func() {}".to_string(),
            summary: String::new(),
            file_path: "test.rs".to_string(),
            start_byte: 0,
            end_byte: 20,
            start_line: 1,
            end_line: 1,
        }];

        store.upsert_symbols(file_path, &symbols)?;

        let symbol = store.get_by_qualified_name("my_mod::MyStruct::my_func")?;
        assert!(symbol.is_some());
        assert_eq!(symbol.unwrap().name, "my_func");

        let not_found = store.get_by_qualified_name("nonexistent")?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[test]
    fn test_mtime_tracking() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_string_lossy().to_string();
        let store = SymbolStore::open(&db_path)?;

        let file_path = Path::new("test.rs");

        // Initially no mtime
        let mtime = store.get_indexed_mtime(file_path)?;
        assert!(mtime.is_none());

        // Set mtime
        store.set_indexed_mtime(file_path, 12345)?;

        // Retrieve mtime
        let mtime = store.get_indexed_mtime(file_path)?;
        assert_eq!(mtime, Some(12345));

        Ok(())
    }

    #[test]
    fn test_delete_file_symbols() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_string_lossy().to_string();
        let store = SymbolStore::open(&db_path)?;

        let file_path = Path::new("test.rs");
        let symbols = vec![Symbol {
            id: 0,
            qualified_name: "my_func".to_string(),
            name: "my_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            signature: "fn my_func() {}".to_string(),
            summary: String::new(),
            file_path: "test.rs".to_string(),
            start_byte: 0,
            end_byte: 20,
            start_line: 1,
            end_line: 1,
        }];

        store.upsert_symbols(file_path, &symbols)?;
        let retrieved = store.get_file_symbols(file_path)?;
        assert_eq!(retrieved.len(), 1);

        store.delete_file_symbols(file_path)?;
        let retrieved = store.get_file_symbols(file_path)?;
        assert_eq!(retrieved.len(), 0);

        Ok(())
    }
}
