---
name: astrolabe-mcp
description: Query and explore code symbols in the astrolabe-mcp workspace. Use this skill when the user wants to search for functions, classes, modules, find symbol definitions, get file outlines, search code text, or explore the workspace structure. Triggers include "search for", "find symbol", "get outline", "search code", "what functions", "list symbols", or any code exploration task.
---

# astrolabe-mcp

Query and explore code symbols across the astrolabe-mcp workspace using tree-sitter-based indexing.

## Setup

The astrolabe-mcp server is configured to run via stdio transport:

```bash
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" --list
```

## Available Tools

### search-symbols
Search code symbols by name, kind, or language.

```bash
# Find all functions named "parse"
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  search-symbols --name-pattern "parse"

# Find all Rust structs
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  search-symbols --language rust --kind struct

# Find functions in a specific file
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  search-symbols --file-path "src/indexer.rs" --kind function

# Search with limit
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  search-symbols --name-pattern "build" --limit 10

# Get specific fields only
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  search-symbols --name-pattern "index" --fields "name,qualified_name,kind,language"
```

**Parameters:**
- `--name-pattern`: Substring or glob pattern to match symbol names
- `--language`: Filter by programming language (rust, python, typescript, javascript, go, c, cpp, elixir)
- `--kind`: Filter by symbol kind (function, struct, class, enum, trait, impl, module, interface, method, etc.)
- `--file-path`: Filter by file path
- `--limit`: Max results (default 20, max 100)
- `--fields`: Comma-separated fields to include (name, qualified_name, kind, language, signature, summary, file_path, start_line, end_line)

### get-file-outline
Get a structural outline of symbols in a file.

```bash
# Get JSON outline of a file
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-file-outline --file-path "src/indexer.rs"

# Get compact text outline
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-file-outline --file-path "src/indexer.rs" --format compact

# Get specific fields only
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-file-outline --file-path "src/indexer.rs" --fields "name,kind,start_line"
```

**Parameters:**
- `--file-path`: Workspace-relative file path (required)
- `--format`: Output format: "json" (default) or "compact"
- `--fields`: Comma-separated fields to include (ignored in compact format)

### get-symbol-implementation
Retrieve the exact source code for a symbol by its qualified name.

```bash
# Get source for a specific function
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-symbol-implementation --qualified-name "indexer::Indexer::new"

# Get source for an Elixir function
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-symbol-implementation --qualified-name "MyApp.Accounts.create_user"
```

**Parameters:**
- `--qualified-name`: Fully qualified symbol name (e.g., "module::Class::method" for Rust, "Module.function" for Elixir)

### get-symbol-implementations
Retrieve source code for multiple symbols in one call.

```bash
# Get source for multiple symbols
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-symbol-implementations --qualified-names "indexer::Indexer::new,indexer::detect_language" --stdin <<EOF
["indexer::Indexer::new", "indexer::detect_language"]
EOF
```

**Parameters:**
- `--qualified-names`: Comma-separated list of qualified names, or use `--stdin` for JSON array

### get-file-summary
Get a dense summary of a file's symbols with doc comments and signatures.

```bash
# Get file summary
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-file-summary --file-path "src/indexer.rs"
```

**Parameters:**
- `--file-path`: Workspace-relative file path (required)

### get-file-content
Read file content with path traversal and secret file safety checks.

```bash
# Read a file
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-file-content --file-path "src/main.rs"
```

**Parameters:**
- `--file-path`: Workspace-relative file path (required)

### get-workspace-overview
Retrieve all indexed files and their top-level symbol names.

```bash
# Get workspace overview
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-workspace-overview
```

### full-text-search
Regex-based text search across workspace files.

```bash
# Search for a pattern
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  full-text-search --pattern "defmodule"

# Search with max results
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  full-text-search --pattern "fn.*parse" --max-results 50
```

**Parameters:**
- `--pattern`: Regex pattern to search for
- `--max-results`: Max results to return (default 50, max 200)

## Common Workflows

### Explore the workspace structure
```bash
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-workspace-overview
```

### Find all functions in a file
```bash
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-file-outline --file-path "src/indexer.rs" --format compact
```

### Search for a specific symbol
```bash
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  search-symbols --name-pattern "build_qualified_name"
```

### Get the implementation of a symbol
```bash
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  get-symbol-implementation --qualified-name "indexer::build_qualified_name"
```

### Find all Elixir symbols
```bash
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  search-symbols --language elixir
```

### Search code text
```bash
uvx mcp2cli --mcp-stdio "astrolabe-mcp /home/vasco/projects/astrolabe-mcp" \
  full-text-search --pattern "extract_doc_comment"
```

## Symbol Kinds

Supported symbol kinds for filtering:
- `function` - Functions and methods
- `struct` - Struct definitions
- `class` - Class definitions
- `enum` - Enum definitions
- `trait` - Trait definitions
- `impl` - Implementation blocks
- `module` - Modules and namespaces
- `interface` - Interfaces and protocols
- `method` - Class/struct methods
- `variable` - Variables and constants

## Languages

Supported languages:
- `rust` - Rust source files (.rs)
- `python` - Python source files (.py)
- `typescript` - TypeScript source files (.ts, .tsx)
- `javascript` - JavaScript source files (.js, .jsx)
- `go` - Go source files (.go)
- `c` - C source files (.c, .h)
- `cpp` - C++ source files (.cpp, .hpp, .cc, .cxx)
- `elixir` - Elixir source files (.ex, .exs)

## Tips

- Use `--fields` to reduce output size and focus on relevant information
- Use `--limit` to cap results when searching large codebases
- Use `--format compact` for quick file outlines
- Combine `search-symbols` with `get-symbol-implementation` to find and inspect code
- Use `full-text-search` for regex patterns when symbol search isn't specific enough
