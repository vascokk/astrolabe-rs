use crate::models::{SearchQuery, SymbolKind, Symbol, SymbolField};
use crate::store::SymbolStore;
use crate::retriever::SourceRetriever;
use crate::searcher::FullTextSearcher;
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    model::*,
    service::RequestContext,
    RoleServer,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use indexmap::IndexMap;

/// Parameter struct for search_symbols tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSymbolsParams {
    /// Substring or glob pattern to match symbol names
    pub name_pattern: Option<String>,
    /// Filter by symbol kind (function, struct, class, etc.)
    pub kind: Option<SymbolKind>,
    /// Filter by programming language
    pub language: Option<String>,
    /// Filter by file path
    pub file_path: Option<String>,
    /// Max results to return (default 20, max 100)
    pub limit: Option<usize>,
    /// Optional subset of fields to include in each result
    pub fields: Option<Vec<String>>,
}

/// Outline format for get_file_outline
#[derive(Debug, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutlineFormat {
    #[default]
    Json,
    Compact,
}

/// Parameter struct for get_file_outline tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFileOutlineParams {
    /// Workspace-relative file path
    pub file_path: String,
    /// Optional subset of fields to include in each result
    pub fields: Option<Vec<String>>,
    /// Output format: "json" (default) or "compact" (terse text lines)
    pub format: Option<OutlineFormat>,
}

/// Parameter struct for get_symbol_implementation tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSymbolImplParams {
    /// Fully qualified symbol name (e.g. "my_mod::MyStruct::my_func")
    pub qualified_name: String,
}

/// Parameter struct for full_text_search tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FullTextSearchParams {
    /// Regex pattern to search for
    pub pattern: String,
    /// Max results to return (default 50, max 200)
    pub max_results: Option<usize>,
}

/// Parameter struct for get_file_content tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFileContentParams {
    /// Workspace-relative file path
    pub file_path: String,
}

/// Parameter struct for get_workspace_overview tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetWorkspaceOverviewParams {
    // intentionally empty; reserved for future filters (e.g. language)
}

/// Parameter struct for get_file_summary tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFileSummaryParams {
    /// Workspace-relative file path
    pub file_path: String,
}

/// Parameter struct for get_symbol_implementations batch tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSymbolImplementationsParams {
    /// One or more fully-qualified symbol names to fetch
    pub qualified_names: Vec<String>,
}

/// Result entry for get_symbol_implementations
#[derive(Debug, Serialize)]
pub struct SymbolImplementationResult {
    /// The full source text of the symbol body, or null if not found
    pub implementation: Option<String>,
    /// Human-readable error message when implementation is null
    pub error: Option<String>,
}



/// MCP Server implementation for astrolabe-mcp
#[derive(Clone)]
pub struct AstrolabeServer {
    pub store: SymbolStore,
    pub searcher: Arc<FullTextSearcher>,
    pub workspace_root: PathBuf,
}

impl AstrolabeServer {
    /// Creates a new AstrolabeServer instance
    pub fn new(
        store: SymbolStore,
        searcher: FullTextSearcher,
        workspace_root: PathBuf,
    ) -> Self {
        AstrolabeServer {
            store,
            searcher: Arc::new(searcher),
            workspace_root,
        }
    }

    /// Handle search_symbols tool call
    pub async fn handle_search_symbols(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        let params: SearchSymbolsParams = parse_args(args)?;

        if params.name_pattern.is_none()
            && params.kind.is_none()
            && params.language.is_none()
            && params.file_path.is_none()
        {
            return Ok(CallToolResult::error(vec![Content::text(
                "At least one filter (name_pattern, kind, language, or file_path) must be provided",
            )]));
        }

        let query = SearchQuery {
            name_pattern: params.name_pattern,
            kind: params.kind,
            language: params.language,
            file_path: params.file_path,
            limit: params.limit,
        };

        let results = self.store.search(&query)
            .map_err(|e| McpError::internal_error(format!("Search failed: {}", e), None))?;

        let projected: Vec<serde_json::Value> = results
            .iter()
            .map(|s| project_symbol(s, params.fields.as_deref()))
            .collect();

        let json = serde_json::to_string(&projected)
            .map_err(|e| McpError::internal_error(format!("Serialization failed: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Handle get_file_outline tool call
    pub async fn handle_get_file_outline(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        let params: GetFileOutlineParams = parse_args(args)?;

        let file_path = self.workspace_root.join(&params.file_path);
        if !SourceRetriever::is_safe_path(&self.workspace_root, &file_path) {
            return Ok(CallToolResult::error(vec![Content::text(
                r#"{"error":"path_traversal","message":"Access denied"}"#,
            )]));
        }

        let relative_path = Path::new(&params.file_path);
        let symbols = self.store.get_file_symbols(relative_path)
            .map_err(|e| McpError::internal_error(format!("Query failed: {}", e), None))?;

        // Check format parameter
        match params.format {
            Some(OutlineFormat::Compact) => {
                // Compact format: return plain text, ignore fields
                let text = render_compact_outline(&symbols);
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None | Some(OutlineFormat::Json) => {
                // JSON format (default): project symbols with optional field filtering
                let outline: Vec<serde_json::Value> = symbols
                    .iter()
                    .map(|s| project_symbol(s, params.fields.as_deref()))
                    .collect();

                let json = serde_json::to_string(&outline)
                    .map_err(|e| McpError::internal_error(format!("Serialization failed: {}", e), None))?;

                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
        }
    }

    /// Handle get_symbol_implementation tool call
    pub async fn handle_get_symbol_implementation(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        let params: GetSymbolImplParams = parse_args(args)?;

        let symbol = self.store.get_by_qualified_name(&params.qualified_name)
            .map_err(|e| McpError::internal_error(format!("Lookup failed: {}", e), None))?;

        let symbol = match symbol {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Symbol '{}' not found. Try using search_symbols to find available symbols.",
                    params.qualified_name
                ))]));
            }
        };

        let file_path = self.workspace_root.join(&symbol.file_path);
        if !SourceRetriever::is_safe_path(&self.workspace_root, &file_path) {
            return Ok(CallToolResult::error(vec![Content::text(
                r#"{"error":"path_traversal","message":"Access denied"}"#,
            )]));
        }

        let source = SourceRetriever::get_source(&file_path, symbol.start_byte, symbol.end_byte)
            .await
            .map_err(|e| McpError::internal_error(format!("Source retrieval failed: {}", e), None))?;

        let result = SymbolImplementationResult {
            implementation: Some(source),
            error: None,
        };

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string(&result)
                .map_err(|e| McpError::internal_error(format!("Serialization failed: {}", e), None))?,
        )]))
    }

    /// Handle full_text_search tool call
    pub async fn handle_full_text_search(
            &self,
            args: Option<serde_json::Map<String, serde_json::Value>>,
        ) -> Result<CallToolResult, McpError> {
            let params: FullTextSearchParams = parse_args(args)?;
            let max_results = params.max_results.unwrap_or(50);

            match self.searcher.search(&params.pattern, max_results) {
                Ok(matches) => {
                    let text = matches
                        .iter()
                        .map(|m| format!("{}:{}: {}", m.file_path, m.line_number, m.line_content))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(CallToolResult::success(vec![Content::text(text)]))
                }
                Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid regex pattern: {}", e
                ))])),
            }
        }

    /// Handle get_file_content tool call
    pub async fn handle_get_file_content(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        let params: GetFileContentParams = parse_args(args)?;

        let file_path = self.workspace_root.join(&params.file_path);

        if !SourceRetriever::is_safe_path(&self.workspace_root, &file_path) {
            return Ok(CallToolResult::error(vec![Content::text(
                r#"{"error":"path_traversal","message":"Access denied"}"#,
            )]));
        }

        if SourceRetriever::is_blocked_file(&file_path) {
            return Ok(CallToolResult::error(vec![Content::text(
                r#"{"error":"access_denied","message":"File type is restricted"}"#,
            )]));
        }

        let content = SourceRetriever::get_file_content(&file_path, 100_000)
            .await
            .map_err(|e| McpError::internal_error(format!("File read failed: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Handle get_workspace_overview tool call
    pub async fn handle_get_workspace_overview(
        &self,
        _args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        let symbols = self.store.get_all_file_symbols()
            .map_err(|e| McpError::internal_error(format!("Query failed: {}", e), None))?;

        // Group by file_path preserving insertion order
        let mut file_map: IndexMap<String, Vec<String>> = IndexMap::new();
        for symbol in symbols {
            file_map.entry(symbol.file_path).or_default().push(symbol.name);
        }

        if file_map.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(String::new())]));
        }

        let text = file_map
            .into_iter()
            .map(|(path, syms)| format!("{}: {}", path, syms.join(", ")))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Handle get_symbol_implementations batch tool call
    pub async fn handle_get_symbol_implementations(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        let params: GetSymbolImplementationsParams = parse_args(args)?;

        if params.qualified_names.is_empty() {
            return Err(McpError::invalid_params("qualified_names must be non-empty", None));
        }

        let mut results = Vec::new();

        for name in params.qualified_names {
            let result = match self.store.get_by_qualified_name(&name) {
                Ok(Some(symbol)) => {
                    let file_path = self.workspace_root.join(&symbol.file_path);
                    match SourceRetriever::get_source(&file_path, symbol.start_byte, symbol.end_byte).await {
                        Ok(implementation) => SymbolImplementationResult {
                            implementation: Some(implementation),
                            error: None,
                        },
                        Err(e) => SymbolImplementationResult {
                            implementation: None,
                            error: Some(e.to_string()),
                        },
                    }
                }
                Ok(None) => SymbolImplementationResult {
                    implementation: None,
                    error: Some(format!("Symbol not found: {}", name)),
                },
                Err(e) => SymbolImplementationResult {
                    implementation: None,
                    error: Some(e.to_string()),
                },
            };
            results.push(result);
        }

        let json = serde_json::to_string(&results)
            .map_err(|e| McpError::internal_error(format!("Serialization failed: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Handle get_file_summary tool call
    pub async fn handle_get_file_summary(
        &self,
        args: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult, McpError> {
        let params: GetFileSummaryParams = parse_args(args)?;

        // Path traversal check
        if params.file_path.contains("..") {
            return Err(McpError::invalid_params("Invalid file path", None));
        }

        // Get symbols for the file
        let symbols = self.store.get_file_symbols(Path::new(&params.file_path))
            .map_err(|e| McpError::internal_error(format!("Query failed: {}", e), None))?;

        // If no symbols, return the "no symbols indexed" message
        if symbols.is_empty() {
            let text = format!("<no symbols indexed for {}>", params.file_path);
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        // Build the summary text
        let mut lines = vec![format!("{}:", params.file_path)];
        for symbol in &symbols {
            // Format kind as snake_case (matching JSON serialization)
            let kind_str = format!("{:?}", symbol.kind).to_lowercase();
            
            // Build descriptor line: "kind name" + optional " — summary"
            let mut descriptor = format!("{} {}", kind_str, symbol.name);
            if !symbol.summary.is_empty() {
                descriptor.push_str(&format!(" — {}", symbol.summary));
            }
            lines.push(descriptor);

            // Add indented signature line if non-empty
            if !symbol.signature.is_empty() {
                lines.push(format!("  {}", symbol.signature));
            }
        }

        let text = lines.join("\n");
        Ok(CallToolResult::success(vec![Content::text(text)]))

    }
}

impl ServerHandler for AstrolabeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "astrolabe-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        use rmcp::handler::server::tool::schema_for_type;

        Ok(ListToolsResult {
            tools: vec![
                Tool::new(
                    "search_symbols",
                    "Search code symbols by name, kind, or language across the indexed codebase",
                    schema_for_type::<SearchSymbolsParams>(),
                ),
                Tool::new(
                    "get_file_outline",
                    "Get a structural outline of symbols in a file without loading source code",
                    schema_for_type::<GetFileOutlineParams>(),
                ),
                Tool::new(
                    "get_symbol_implementation",
                    "Retrieve the exact source code for a symbol by its qualified name",
                    schema_for_type::<GetSymbolImplParams>(),
                ),
                Tool::new(
                    "get_symbol_implementations",
                    "Retrieve the exact source code for multiple symbols by their qualified names in a single call",
                    schema_for_type::<GetSymbolImplementationsParams>(),
                ),
                Tool::new(
                    "full_text_search",
                    "Regex-based text search across workspace files (fallback when symbol search is insufficient)",
                    schema_for_type::<FullTextSearchParams>(),
                ),
                Tool::new(
                    "get_file_content",
                    "Read file content with path traversal and secret file safety checks",
                    schema_for_type::<GetFileContentParams>(),
                ),
                Tool::new(
                    "get_workspace_overview",
                    "Retrieve all indexed files and their top-level symbol names in a single call",
                    schema_for_type::<GetWorkspaceOverviewParams>(),
                ),
                Tool::new(
                    "get_file_summary",
                    "Get a dense plain-text summary of a file's symbols including doc comments and signatures",
                    schema_for_type::<GetFileSummaryParams>(),
                ),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        match request.name.as_ref() {
            "search_symbols" => self.handle_search_symbols(request.arguments).await,
            "get_file_outline" => self.handle_get_file_outline(request.arguments).await,
            "get_symbol_implementation" => self.handle_get_symbol_implementation(request.arguments).await,
            "get_symbol_implementations" => self.handle_get_symbol_implementations(request.arguments).await,
            "full_text_search" => self.handle_full_text_search(request.arguments).await,
            "get_file_content" => self.handle_get_file_content(request.arguments).await,
            "get_workspace_overview" => self.handle_get_workspace_overview(request.arguments).await,
            "get_file_summary" => self.handle_get_file_summary(request.arguments).await,
            _ => Err(McpError::invalid_params(
                format!("Unknown tool: {}", request.name),
                None,
            )),
        }
    }
}

/// Render symbols in compact outline format
fn render_compact_outline(symbols: &[Symbol]) -> String {
    symbols
        .iter()
        .map(|s| format!("{} {} [{}-{}]", format!("{:?}", s.kind).to_lowercase(), s.name, s.start_line, s.end_line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert a Symbol to a full JSON object with all fields
pub fn full_symbol_to_json(symbol: &Symbol) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "id": symbol.id,
        "qualified_name": symbol.qualified_name,
        "name": symbol.name,
        "kind": symbol.kind,
        "language": symbol.language,
        "signature": symbol.signature,
        "file_path": symbol.file_path,
        "start_byte": symbol.start_byte,
        "end_byte": symbol.end_byte,
        "start_line": symbol.start_line,
        "end_line": symbol.end_line,
    });
    if !symbol.summary.is_empty() {
        obj["summary"] = serde_json::json!(symbol.summary);
    }
    obj
}

/// Project a Symbol to only the requested fields
pub fn project_symbol(symbol: &Symbol, fields: Option<&[String]>) -> serde_json::Value {
    // If no fields specified, return the default set (3 fields: name, kind, signature)
    let Some(fields) = fields else {
        return serde_json::json!({
            "name": symbol.name,
            "kind": symbol.kind,
            "signature": symbol.signature,
        });
    };

    // Parse field names, silently dropping unknowns
    let parsed: Vec<SymbolField> = fields
        .iter()
        .filter_map(|f| f.parse().ok())
        .collect();

    // If no valid fields were parsed, return full object
    if parsed.is_empty() {
        return full_symbol_to_json(symbol);
    }

    // Build a map with only the requested fields
    let mut obj = serde_json::Map::new();
    for field in &parsed {
        let value = match field {
            SymbolField::Id => serde_json::json!(symbol.id),
            SymbolField::QualifiedName => serde_json::json!(symbol.qualified_name),
            SymbolField::Name => serde_json::json!(symbol.name),
            SymbolField::Kind => serde_json::json!(symbol.kind),
            SymbolField::Language => serde_json::json!(symbol.language),
            SymbolField::Signature => serde_json::json!(symbol.signature),
            SymbolField::Summary => serde_json::json!(symbol.summary),
            SymbolField::FilePath => serde_json::json!(symbol.file_path),
            SymbolField::StartByte => serde_json::json!(symbol.start_byte),
            SymbolField::EndByte => serde_json::json!(symbol.end_byte),
            SymbolField::StartLine => serde_json::json!(symbol.start_line),
            SymbolField::EndLine => serde_json::json!(symbol.end_line),
        };
        obj.insert(field.key().to_string(), value);
    }

    serde_json::Value::Object(obj)
}

/// Parse tool arguments from optional JSON object into a typed struct
fn parse_args<T: serde::de::DeserializeOwned>(args: Option<serde_json::Map<String, serde_json::Value>>) -> Result<T, McpError> {
    let value = serde_json::Value::Object(args.unwrap_or_default());
    serde_json::from_value(value).map_err(|e| {
        McpError::invalid_params(format!("Invalid parameters: {}", e), None)
    })
}

#[cfg(test)]
mod tests {
    use crate::models::{Symbol, SymbolKind};
    use crate::server::AstrolabeServer;
    use crate::store::SymbolStore;
    use crate::searcher::FullTextSearcher;
    use rmcp::model::*;
    use serde_json::json;
    use std::path::Path;
    use tempfile::TempDir;

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

    fn create_symbol(
        name: &str,
        kind: SymbolKind,
        file_path: &str,
        start_line: u32,
        end_line: u32,
    ) -> Symbol {
        Symbol {
            id: 0,
            qualified_name: format!("test::{}", name),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            signature: format!("fn {}() {{}}", name),
            summary: format!("Summary for {}", name),
            file_path: file_path.to_string(),
            start_byte: 0,
            end_byte: 1000,
            start_line,
            end_line,
        }
    }

    #[test]
    fn test_get_all_file_symbols_empty() {
        let (server, _temp_dir) = setup_test_server();
        let result = server.store.get_all_file_symbols().unwrap();
        assert_eq!(result.len(), 0, "Empty store should return empty vec");
    }

    #[test]
    fn test_get_all_file_symbols_ordering() {
        let (server, _temp_dir) = setup_test_server();

        let symbols = [
            create_symbol("func_b", SymbolKind::Function, "file_b.rs", 10, 15),
            create_symbol("func_a", SymbolKind::Function, "file_a.rs", 5, 8),
            create_symbol("func_c", SymbolKind::Function, "file_b.rs", 1, 3),
            create_symbol("func_d", SymbolKind::Function, "file_a.rs", 20, 25),
        ];

        server.store.upsert_symbols(Path::new("file_a.rs"), &[symbols[1].clone(), symbols[3].clone()]).unwrap();
        server.store.upsert_symbols(Path::new("file_b.rs"), &[symbols[0].clone(), symbols[2].clone()]).unwrap();

        let result = server.store.get_all_file_symbols().unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].file_path, "file_a.rs");
        assert_eq!(result[0].start_line, 5);
        assert_eq!(result[1].file_path, "file_a.rs");
        assert_eq!(result[1].start_line, 20);
        assert_eq!(result[2].file_path, "file_b.rs");
        assert_eq!(result[2].start_line, 1);
        assert_eq!(result[3].file_path, "file_b.rs");
        assert_eq!(result[3].start_line, 10);
    }

    #[test]
    fn test_project_symbol_known_fields() {
        let symbol = create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 5);
        let fields = vec!["name".to_string(), "kind".to_string()];
        let projected = crate::server::project_symbol(&symbol, Some(&fields));
        let obj = projected.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("kind"));
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["name"].as_str().unwrap(), "test_func");
    }

    #[test]
    fn test_project_symbol_unknown_fields_ignored() {
        let symbol = create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 5);
        let fields = vec!["name".to_string(), "nonexistent_field".to_string(), "kind".to_string()];
        let projected = crate::server::project_symbol(&symbol, Some(&fields));
        let obj = projected.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("kind"));
        assert!(!obj.contains_key("nonexistent_field"));
        assert_eq!(obj.len(), 2);
    }

    #[test]
    fn test_project_symbol_empty_fields_returns_all() {
        let symbol = create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 5);
        let fields: Vec<String> = vec![];
        let projected = crate::server::project_symbol(&symbol, Some(&fields));
        let obj = projected.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("kind"));
        assert!(obj.contains_key("qualified_name"));
        assert!(obj.contains_key("signature"));
        assert!(obj.contains_key("summary"));
        assert!(obj.contains_key("file_path"));
        assert!(obj.contains_key("start_line"));
        assert!(obj.contains_key("end_line"));
    }

    #[test]
    fn test_backwards_compat_no_fields() {
        let symbol = create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 5);
        let projected = crate::server::project_symbol(&symbol, None);
        let obj = projected.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("kind"));
        assert!(obj.contains_key("signature"));
        assert!(!obj.contains_key("start_line"));
        assert!(!obj.contains_key("end_line"));
        assert!(!obj.contains_key("qualified_name"));
        assert!(!obj.contains_key("summary"));
        assert!(!obj.contains_key("file_path"));
        assert_eq!(obj.len(), 3, "Default projection should have exactly 3 fields");
    }

    #[test]
    fn test_full_symbol_to_json_omits_empty_summary() {
        let mut symbol = create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 5);
        symbol.summary = String::new();
        let json = crate::server::full_symbol_to_json(&symbol);
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("summary"), "Empty summary should be omitted from JSON");
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("kind"));
    }

    #[test]
    fn test_full_symbol_to_json_includes_nonempty_summary() {
        let symbol = create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 5);
        let json = crate::server::full_symbol_to_json(&symbol);
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("summary"), "Non-empty summary should be included in JSON");
        assert_eq!(obj["summary"].as_str().unwrap(), "Summary for test_func");
    }

    #[test]
    fn test_project_symbol_explicit_summary_field_when_empty() {
        let mut symbol = create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 5);
        symbol.summary = String::new();
        let fields = vec!["summary".to_string()];
        let projected = crate::server::project_symbol(&symbol, Some(&fields));
        let obj = projected.as_object().unwrap();
        assert!(obj.contains_key("summary"), "Explicitly requested summary should be included even when empty");
        assert_eq!(obj["summary"].as_str().unwrap(), "");
    }

    #[test]
    fn test_project_symbol_explicit_line_numbers() {
        let symbol = create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 5);
        let fields = vec![
            "name".to_string(),
            "kind".to_string(),
            "signature".to_string(),
            "start_line".to_string(),
            "end_line".to_string(),
        ];
        let projected = crate::server::project_symbol(&symbol, Some(&fields));
        let obj = projected.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("kind"));
        assert!(obj.contains_key("signature"));
        assert!(obj.contains_key("start_line"));
        assert!(obj.contains_key("end_line"));
        assert_eq!(obj.len(), 5, "Explicitly requesting all 5 fields should return all 5");
        assert_eq!(obj["start_line"].as_u64().unwrap(), 1);
        assert_eq!(obj["end_line"].as_u64().unwrap(), 5);
    }

    #[test]
    fn test_workspace_overview_groups_by_file() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let symbols_a = vec![
            create_symbol("func_a1", SymbolKind::Function, "file_a.rs", 1, 5),
            create_symbol("struct_a", SymbolKind::Struct, "file_a.rs", 10, 20),
        ];
        let symbols_b = vec![
            create_symbol("func_b1", SymbolKind::Function, "file_b.rs", 1, 5),
        ];

        server.store.upsert_symbols(Path::new("file_a.rs"), &symbols_a).unwrap();
        server.store.upsert_symbols(Path::new("file_b.rs"), &symbols_b).unwrap();

        let result = rt.block_on(server.handle_get_workspace_overview(None)).unwrap();
        let text = extract_text(&result);

        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "file_a.rs: func_a1, struct_a");
        assert_eq!(lines[1], "file_b.rs: func_b1");
    }

    #[test]
    fn test_workspace_overview_empty_store() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = rt.block_on(server.handle_get_workspace_overview(None)).unwrap();
        let text = extract_text(&result);
        assert_eq!(text, "");
    }

    #[test]
    fn test_get_symbol_implementations_single() {
        let (server, temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let test_file = temp_dir.path().join("test.rs");
        let content = "pub fn test_func() {\n    println!(\"hello\");\n}";
        std::fs::write(&test_file, content).unwrap();

        let symbol = Symbol {
            id: 0,
            qualified_name: "test::test_func".to_string(),
            name: "test_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            signature: "fn test_func() {}".to_string(),
            summary: "Summary for test_func".to_string(),
            file_path: "test.rs".to_string(),
            start_byte: 0,
            end_byte: content.len() as u64,
            start_line: 1,
            end_line: 3,
        };
        server.store.upsert_symbols(Path::new("test.rs"), &[symbol]).unwrap();

        let args = to_args(json!({ "qualified_names": ["test::test_func"] }));
        let result = rt.block_on(server.handle_get_symbol_implementations(args)).unwrap();
        let text = extract_text(&result);
        let implementations: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();

        assert_eq!(implementations.len(), 1);
        assert!(implementations[0]["implementation"].is_string());
        assert!(implementations[0]["error"].is_null());
    }

    #[test]
    fn test_get_symbol_implementations_multiple() {
        let (server, temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let test_file = temp_dir.path().join("test.rs");
        let content = "pub fn func1() {}\npub fn func2() {}\npub fn func3() {}";
        std::fs::write(&test_file, content).unwrap();

        let symbols = vec![
            Symbol {
                id: 0,
                qualified_name: "test::func1".to_string(),
                name: "func1".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                signature: "fn func1() {}".to_string(),
                summary: "Summary for func1".to_string(),
                file_path: "test.rs".to_string(),
                start_byte: 0,
                end_byte: 17,
                start_line: 1,
                end_line: 1,
            },
            Symbol {
                id: 0,
                qualified_name: "test::func2".to_string(),
                name: "func2".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                signature: "fn func2() {}".to_string(),
                summary: "Summary for func2".to_string(),
                file_path: "test.rs".to_string(),
                start_byte: 18,
                end_byte: 35,
                start_line: 2,
                end_line: 2,
            },
            Symbol {
                id: 0,
                qualified_name: "test::func3".to_string(),
                name: "func3".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                signature: "fn func3() {}".to_string(),
                summary: "Summary for func3".to_string(),
                file_path: "test.rs".to_string(),
                start_byte: 36,
                end_byte: 53,
                start_line: 3,
                end_line: 3,
            },
        ];
        server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();

        let args = to_args(json!({ "qualified_names": ["test::func1", "test::func2", "test::func3"] }));
        let result = rt.block_on(server.handle_get_symbol_implementations(args)).unwrap();
        let text = extract_text(&result);
        let implementations: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();

        assert_eq!(implementations.len(), 3);
        assert!(implementations[0]["implementation"].is_string());
        assert!(implementations[0]["error"].is_null());
        assert!(implementations[1]["implementation"].is_string());
        assert!(implementations[1]["error"].is_null());
        assert!(implementations[2]["implementation"].is_string());
        assert!(implementations[2]["error"].is_null());
    }

    #[test]
    fn test_get_symbol_implementations_partial_miss() {
        let (server, temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let test_file = temp_dir.path().join("test.rs");
        let content = "pub fn func1() {}";
        std::fs::write(&test_file, content).unwrap();

        let symbol = Symbol {
            id: 0,
            qualified_name: "test::func1".to_string(),
            name: "func1".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            signature: "fn func1() {}".to_string(),
            summary: "Summary for func1".to_string(),
            file_path: "test.rs".to_string(),
            start_byte: 0,
            end_byte: content.len() as u64,
            start_line: 1,
            end_line: 1,
        };
        server.store.upsert_symbols(Path::new("test.rs"), &[symbol]).unwrap();

        let args = to_args(json!({ "qualified_names": ["test::func1", "test::nonexistent"] }));
        let result = rt.block_on(server.handle_get_symbol_implementations(args)).unwrap();
        let text = extract_text(&result);
        let implementations: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();

        assert_eq!(implementations.len(), 2);
        assert!(implementations[0]["implementation"].is_string());
        assert!(implementations[0]["error"].is_null());
        assert!(implementations[1]["implementation"].is_null());
        assert!(implementations[1]["error"].is_string());
    }

    #[test]
    fn test_get_symbol_implementations_empty_names() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let args = to_args(json!({ "qualified_names": [] }));
        let result = rt.block_on(server.handle_get_symbol_implementations(args));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_file_summary_basic() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let symbols = vec![
            create_symbol("MyStruct", SymbolKind::Struct, "test.rs", 1, 5),
            create_symbol("my_func", SymbolKind::Function, "test.rs", 10, 15),
        ];
        server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();

        let args = to_args(json!({ "file_path": "test.rs" }));
        let result = rt.block_on(server.handle_get_file_summary(args)).unwrap();
        let text = extract_text(&result);

        assert!(text.starts_with("test.rs:"));
        assert!(text.contains("struct MyStruct"));
        assert!(text.contains("function my_func"));
        assert!(text.contains("Summary for MyStruct"));
        assert!(text.contains("Summary for my_func"));
    }

    #[test]
    fn test_get_file_summary_no_bodies() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let symbols = vec![create_symbol("test_func", SymbolKind::Function, "test.rs", 1, 10)];
        server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();

        let args = to_args(json!({ "file_path": "test.rs" }));
        let result = rt.block_on(server.handle_get_file_summary(args)).unwrap();
        let text = extract_text(&result);

        assert!(text.contains("test.rs:"));
        assert!(text.contains("test_func"));
        assert!(!text.contains("println!"));
    }

    #[test]
    fn test_get_file_summary_empty_file() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let args = to_args(json!({ "file_path": "nonexistent.rs" }));
        let result = rt.block_on(server.handle_get_file_summary(args)).unwrap();
        let text = extract_text(&result);

        assert!(text.contains("no symbols indexed"));
        assert!(text.contains("nonexistent.rs"));
    }

    #[test]
    fn test_get_file_summary_path_traversal() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let args = to_args(json!({ "file_path": "../../etc/passwd" }));
        let result = rt.block_on(server.handle_get_file_summary(args));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_file_outline_compact_format() {
        let (server, temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "// test file").unwrap();

        let symbols = vec![
            create_symbol("MyStruct", SymbolKind::Struct, "test.rs", 5, 10),
            create_symbol("my_func", SymbolKind::Function, "test.rs", 15, 20),
            create_symbol("MyEnum", SymbolKind::Enum, "test.rs", 25, 30),
        ];
        server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();

        let args = to_args(json!({ "file_path": "test.rs", "format": "compact" }));
        let result = rt.block_on(server.handle_get_file_outline(args)).unwrap();
        let text = extract_text(&result);

        assert!(text.contains("struct MyStruct [5-10]"));
        assert!(text.contains("function my_func [15-20]"));
        assert!(text.contains("enum MyEnum [25-30]"));
        assert!(!text.starts_with("["));
        assert!(!text.starts_with("{"));
    }

    #[test]
    fn test_get_file_outline_compact_ignores_fields() {
        let (server, temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "// test file").unwrap();

        let symbols = vec![create_symbol("MyStruct", SymbolKind::Struct, "test.rs", 5, 10)];
        server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();

        let args = to_args(json!({ "file_path": "test.rs", "format": "compact", "fields": ["name", "kind"] }));
        let result = rt.block_on(server.handle_get_file_outline(args)).unwrap();
        let text = extract_text(&result);

        assert!(text.contains("struct MyStruct [5-10]"));
        assert!(!text.starts_with("["));
    }

    #[test]
    fn test_get_file_outline_json_format_default() {
        let (server, temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "// test file").unwrap();

        let symbols = vec![create_symbol("MyStruct", SymbolKind::Struct, "test.rs", 5, 10)];
        server.store.upsert_symbols(Path::new("test.rs"), &symbols).unwrap();

        let args = to_args(json!({ "file_path": "test.rs" }));
        let result = rt.block_on(server.handle_get_file_outline(args)).unwrap();
        let text = extract_text(&result);

        let outline: Vec<serde_json::Value> = serde_json::from_str(&text).expect("Should be valid JSON array");
        assert_eq!(outline.len(), 1);
        assert_eq!(outline[0]["name"].as_str().unwrap(), "MyStruct");
        assert_eq!(outline[0]["kind"].as_str().unwrap(), "struct");
    }

    #[test]
    fn test_malformed_json_parameters_return_error() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = rt.block_on(server.handle_get_file_outline(
            to_args(json!({"file_path": 123})),
        ));
        if let Ok(tool_result) = result {
            assert!(tool_result.is_error.unwrap_or(false));
        }
    }

    #[test]
    fn test_missing_required_parameters_return_error() {
        let (server, _temp_dir) = setup_test_server();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let result = rt.block_on(server.handle_get_file_outline(
            to_args(json!({})),
        ));
        if let Ok(tool_result) = result {
            assert!(tool_result.is_error.unwrap_or(false));
        }
    }
}
