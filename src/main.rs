mod models;
mod store;
mod indexer;
mod retriever;
mod searcher;
mod server;
mod error;
mod retry;

use anyhow::Result;
use clap::Parser;
use indexer::Indexer;
use server::AstrolabeServer;
use searcher::FullTextSearcher;
use std::path::PathBuf;
use store::SymbolStore;

/// astrolabe-mcp: A Rust MCP server for code indexing and retrieval
#[derive(Parser, Debug)]
#[command(name = "astrolabe-mcp")]
#[command(about = "MCP server for code indexing and retrieval", long_about = None)]
struct Args {
    /// Workspace root directory to index (defaults to current directory)
    #[arg(value_name = "PATH", default_value = ".")]
    workspace_root: PathBuf,

    /// Path to SQLite database (defaults to workspace_root/.astrolabe.db)
    #[arg(short, long, value_name = "PATH")]
    db_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Parse CLI arguments
    let args = Args::parse();

    // Determine database path
    let db_path = args.db_path.unwrap_or_else(|| {
        args.workspace_root.join(".astrolabe.db")
    });

    tracing::info!(
        "Starting astrolabe-mcp server for workspace: {}",
        args.workspace_root.display()
    );
    tracing::info!("Using database: {}", db_path.display());

    // Open/create SQLite database and run migrations
    let store = SymbolStore::open(db_path.to_string_lossy().as_ref())?;
    tracing::info!("Database initialized and migrations completed");

    // Create indexer and index the workspace
    let mut indexer = Indexer::new(store.clone())?;
    tracing::info!("Starting workspace indexing...");

    let stats = indexer.index_workspace(&args.workspace_root)?;

    // Log IndexStats results
    tracing::info!(
        "Indexing complete: {} files indexed, {} files skipped, {} symbols total, {}ms",
        stats.files_indexed,
        stats.files_skipped,
        stats.symbols_total,
        stats.duration_ms
    );

    // Create the MCP server
    let searcher = FullTextSearcher::new(args.workspace_root.clone());
    let server = AstrolabeServer::new(store, searcher, args.workspace_root);

    tracing::info!("Starting MCP server on stdio transport");

    // Start MCP server on stdio transport using rmcp v0.16 pattern
    use rmcp::ServiceExt;
    let transport = rmcp::transport::stdio();
    let server = server.serve(transport).await?;
    server.waiting().await?;

    Ok(())
}
