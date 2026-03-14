# Code Navigation with astrolabe-mcp

This workspace has an astrolabe MCP server running and indexed. Always prefer its tools over reading files directly.

## Tool preference order

1. `get_workspace_overview` — get a full map of all files and their top-level symbols in one call; start here
2. `search_symbols` — find symbols by name, kind, or language
3. `get_file_summary` — get a file's symbols with doc comments and signatures, without full source bodies
4. `get_file_outline` — get a file's symbol list; use `format: "compact"` for a terse single-line-per-symbol view
5. `get_symbol_implementations` — fetch multiple symbol bodies in one round-trip (prefer over repeated single calls)
6. `get_symbol_implementation` — retrieve source for a single known qualified name
7. `full_text_search` — regex search across the workspace as a fallback
8. `get_file_content` — read a full file only when none of the above suffice

## Rules

- Do NOT use `readFile`, `readMultipleFiles`, or `readCode` to explore code structure when astrolabe tools can answer the question.
- Start with `get_workspace_overview` to orient yourself before diving into specific files.
- Use `get_file_summary` before `get_file_outline` — it includes doc comments and signatures, which are more useful.
- When fetching multiple symbol bodies, always use `get_symbol_implementations` (plural) instead of multiple `get_symbol_implementation` calls.
- Use the `fields` parameter on `search_symbols` and `get_file_outline` to request only the fields you need — this reduces payload size significantly.
- Use `format: "compact"` on `get_file_outline` when you only need a quick structural overview and don't need JSON.
- Only fall back to direct file reads when astrolabe cannot answer (e.g. config files, non-indexed file types, or when the server is unavailable).

## fields parameter

Both `search_symbols` and `get_file_outline` accept an optional `fields` array to limit which symbol fields are returned:

```json
{ "name_pattern": "open", "fields": ["name", "kind", "file_path", "signature"] }
```

Valid values: `id`, `qualified_name`, `name`, `kind`, `language`, `signature`, `summary`, `file_path`, `start_byte`, `end_byte`, `start_line`, `end_line`.

Unknown field names are silently ignored. Omitting `fields` returns all fields.

## get_file_outline formats

```json
{ "file_path": "src/store.rs", "format": "json" }   // default — JSON array of symbol objects
{ "file_path": "src/store.rs", "format": "compact" } // plain text, one line per symbol: "kind name [start-end]"
```

Compact format ignores the `fields` parameter.

## get_symbol_implementations

Accepts a list of qualified names and returns all implementations in one call. Missing symbols get a null `implementation` and a non-null `error` — the call itself always succeeds.

```json
{ "qualified_names": ["store::SymbolStore::open", "store::SymbolStore::search"] }
```
