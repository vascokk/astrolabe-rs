use rmcp::model::{CallToolResult, Content};
use thiserror::Error;

/// Structured error types for astrolabe-mcp
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum AstrolabeError {
    #[error("Path traversal attempt detected")]
    PathTraversal,

    #[error("Access denied")]
    AccessDenied,

    #[error("Symbol not found")]
    SymbolNotFound,

    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),
}

impl AstrolabeError {
    /// Converts the error to an MCP CallToolResult error format
    #[allow(dead_code)]
    pub fn to_call_tool_result(&self) -> CallToolResult {
        let message = match self {
            AstrolabeError::PathTraversal => r#"{"error":"path_traversal","message":"Access denied"}"#.to_string(),
            AstrolabeError::AccessDenied => r#"{"error":"access_denied","message":"File type is restricted"}"#.to_string(),
            AstrolabeError::SymbolNotFound => r#"{"error":"symbol_not_found","message":"Symbol not found. Try using search_symbols to find available symbols."}"#.to_string(),
            AstrolabeError::InvalidRegex(msg) => format!(r#"{{"error":"invalid_regex","message":"Invalid regex pattern: {}"}}"#, msg),
            AstrolabeError::ParseError(msg) => format!(r#"{{"error":"parse_error","message":"Parse error: {}"}}"#, msg),
            AstrolabeError::DatabaseError(msg) => format!(r#"{{"error":"database_error","message":"Database error: {}"}}"#, msg),
        };

        CallToolResult::error(vec![Content::text(message)])
    }
}

impl From<rusqlite::Error> for AstrolabeError {
    fn from(err: rusqlite::Error) -> Self {
        AstrolabeError::DatabaseError(err.to_string())
    }
}

impl From<regex::Error> for AstrolabeError {
    fn from(err: regex::Error) -> Self {
        AstrolabeError::InvalidRegex(err.to_string())
    }
}

impl From<std::io::Error> for AstrolabeError {
    fn from(err: std::io::Error) -> Self {
        AstrolabeError::ParseError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for AstrolabeError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        AstrolabeError::ParseError(err.to_string())
    }
}
