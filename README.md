# astrolabe-mcp

A Rust MCP server that indexes your codebase using tree-sitter AST parsing and exposes tools for AI coding assistants to discover and retrieve code by symbol — without reading entire files.

Symbols and byte offsets are stored in SQLite. Source is retrieved on demand via O(1) file seeking, so memory usage stays flat regardless of repository size.

## Supported Languages

Rust, Python, TypeScript, JavaScript, Go, C, C++

## Why?

AI coding assistants like Kiro don't maintain a persistent index of your codebase. Without a tool like this, they operate reactively — reading files on demand, running regex searches, and piecing together structure from raw source. This works, but it's slow and imprecise on large repos.

With astrolabe-mcp, the codebase is indexed once at startup into SQLite. The AI can then ask structured questions like "find all functions named `handle_*` in Rust files" and get an instant, precise answer — without reading a single source file. It's the difference between grep-ing through code and querying a database.

| | Without astrolabe-mcp | With astrolabe-mcp |
|---|---|---|
| Symbol lookup | Read files, grep for patterns | Query pre-built SQLite index |
| Codebase awareness | Rebuilt each conversation | Persistent across sessions |
| Large repo performance | Degrades with repo size | Flat — O(1) source retrieval via byte offsets |
| Precision | Approximate (text matching) | Exact (AST-parsed symbols) |

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
# binary at: ./target/release/astrolabe-mcp
```

## Usage

```bash
astrolabe-mcp [workspace-root] [--db-path <path-to-db>]
```

The workspace root defaults to the current directory:

```bash
# Index and serve the current directory
astrolabe-mcp

# Explicit path
astrolabe-mcp /path/to/project

# Custom database location
astrolabe-mcp --db-path /tmp/myproject.db
```

The SQLite database defaults to `<workspace-root>/.astrolabe.db`.

## Configuring with Kiro

Add the server to your Kiro MCP config at `.kiro/settings/mcp.json` (workspace) or `~/.kiro/settings/mcp.json` (user-level).

```json
{
  "mcpServers": {
    "astrolabe": {
      "command": "astrolabe-mcp",
      "args": [],
      "cwd": "/path/to/your/project",
      "disabled": false,
      "autoApprove": [
        "search_symbols",
        "get_file_outline",
        "get_file_summary",
        "get_symbol_implementation",
        "get_symbol_implementations",
        "get_workspace_overview",
        "full_text_search",
        "get_file_content"
      ]
    }
  }
}
```

## Available Tools

### `get_workspace_overview`

Returns all indexed files and their top-level symbol names in a single call. The fastest way to get a full map of the codebase.

```json
{}
```

Response:
```json
{
  "files": [
    { "path": "src/store.rs", "symbols": ["SymbolStore", "open", "search"] },
    { "path": "src/server.rs", "symbols": ["AstrolabeServer", "handle_search_symbols"] }
  ]
}
```

---

### `search_symbols`

Search for code symbols by name, kind, or language. Use the optional `fields` parameter to request only the fields you need, reducing payload size.

```json
{
  "name_pattern": "handle_auth",
  "kind": "function",
  "language": "rust",
  "limit": 10,
  "fields": ["name", "kind", "file_path", "signature"]
}
```

Valid `fields` values: `id`, `qualified_name`, `name`, `kind`, `language`, `signature`, `summary`, `file_path`, `start_byte`, `end_byte`, `start_line`, `end_line`. Unknown field names are silently ignored. Omitting `fields` returns all fields (backwards compatible).

---

### `get_file_outline`

Get a structural overview of a file's symbols. Supports field filtering and two output formats.

```json
{
  "file_path": "src/server.rs",
  "fields": ["name", "kind", "signature"],
  "format": "json"
}
```

Set `"format": "compact"` for a terse plain-text representation — one line per symbol, no JSON overhead:

```
struct AstrolabeServer [25-30]
fn handle_search_symbols [45-80]
fn handle_get_file_outline [82-110]
```

In compact mode the `fields` parameter is ignored.

---

### `get_file_summary`

Returns a dense plain-text summary of a file's symbols — kind, name, doc comment, and signature — without full source bodies. More informative than an outline, cheaper than reading the file.

```json
{
  "file_path": "src/store.rs"
}
```

Response (plain text):
```
src/store.rs:
struct SymbolStore — wraps rusqlite::Connection
  pub struct SymbolStore
fn open — opens or creates the SQLite database
  pub fn open(path: &str) -> Result<Self>
fn search — searches symbols by query parameters
  pub fn search(query: &SearchQuery) -> Result<Vec<Symbol>>
```

---

### `get_symbol_implementation`

Retrieve the exact source code of a single symbol by its qualified name.

```json
{
  "qualified_name": "AstrolabeServer::handle_search_symbols"
}
```

---

### `get_symbol_implementations`

Batch variant of `get_symbol_implementation`. Fetches multiple symbol bodies in a single round-trip. Per-symbol errors are embedded in the result; the call itself never fails due to missing symbols.

```json
{
  "qualified_names": [
    "store::SymbolStore::open",
    "store::SymbolStore::search"
  ]
}
```

Response:
```json
[
  { "qualified_name": "store::SymbolStore::open", "implementation": "pub fn open(...) { ... }", "error": null },
  { "qualified_name": "store::SymbolStore::search", "implementation": "pub fn search(...) { ... }", "error": null }
]
```

---

### `full_text_search`

Regex-based search across all workspace files. Useful when symbol indexing isn't enough.

```json
{
  "pattern": "TODO|FIXME",
  "max_results": 30
}
```

---

### `get_file_content`

Read a file's full content (capped at 100 KB). Blocked for secret files (`.env`, `*.pem`, `*.key`, etc.).

```json
{
  "file_path": "src/models.rs"
}
```

---

## Example Workflow

A typical session navigating a large codebase:

1. **Get a full codebase map**
   Call `get_workspace_overview` — one call returns every file and its top-level symbols.

2. **Understand a file**
   Call `get_file_summary` on a file of interest to get kinds, names, doc comments, and signatures without loading full source.

3. **Read specific implementations**
   Call `get_symbol_implementations` with a list of qualified names to fetch multiple bodies in one round-trip.

4. **Search for patterns**
   Call `full_text_search` with a regex like `retry|backoff|sqlite.*error` when you need to find something that isn't a named symbol.

## Security

- Path traversal is prevented by canonicalising all paths and verifying they stay within the workspace root.
- Secret files are blocked: `.env`, `*.pem`, `*.key`, `id_rsa`, `*.pfx`, `*.p12`, `*.p8`.
- `.gitignore` rules are respected — ignored files are never indexed or searched.

## Environment Variables

| Variable | Description |
|---|---|
| `RUST_LOG` | Log level (`info`, `debug`, `warn`, `error`). Defaults to `info`. |

```bash
RUST_LOG=debug astrolabe-mcp .
```
