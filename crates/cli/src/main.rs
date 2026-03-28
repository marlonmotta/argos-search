//! Argos Search — CLI Interface
//!
//! Usage:
//!   argos-search index build --root "C:\Users\You\Documents"
//!   argos-search search "query" --root "C:\Users\You\Documents" --json --limit 20

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use argos_core::{ArgosConfig, ArgosEngine, SearchResult};
use argos_core::engine::SearchOptions;

#[derive(Parser)]
#[command(
    name = "argos-search",
    version,
    about = "🔍 Argos Search — Fast cross-platform file search (Windows + macOS)",
    long_about = "Search files by name and content across your local filesystem.\nPowered by Tantivy full-text engine + SQLite metadata."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build or rebuild the search index for a directory
    Index {
        #[command(subcommand)]
        action: IndexAction,
    },
    /// Search for files matching a query
    Search {
        /// The search query
        query: String,

        /// Root directory to search in
        #[arg(long)]
        root: PathBuf,

        /// Output results as JSON
        #[arg(long, default_value_t = false)]
        json: bool,

        /// Maximum number of results to return
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Show index statistics
    Stats {
        /// Root directory of the index
        #[arg(long)]
        root: PathBuf,
    },
}

#[derive(Subcommand)]
enum IndexAction {
    /// Build the index (full scan, incremental by default)
    Build {
        /// Root directory to index
        #[arg(long)]
        root: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Index { action } => match action {
            IndexAction::Build { root } => cmd_index_build(&root)?,
        },
        Commands::Search { query, root, json, limit } => {
            cmd_search(&root, &query, json, limit)?;
        }
        Commands::Stats { root } => cmd_stats(&root)?,
    }

    Ok(())
}

fn cmd_index_build(root: &PathBuf) -> Result<()> {
    let root = std::fs::canonicalize(root)?;
    println!("🔍 Argos Search — Indexing: {}", root.display());
    println!();

    let config = ArgosConfig::load_from_root(&root)?;
    let engine = ArgosEngine::new(root, config)?;

    let stats = engine.index_build()?;

    println!("✅ Index build complete!");
    println!("   📁 Files found:   {}", stats.total_found);
    println!("   📝 Indexed:       {}", stats.indexed);
    println!("   ⏭️  Skipped:       {}", stats.skipped);
    println!("   ❌ Errors:        {}", stats.errors);
    println!("   🗑️  Pruned:        {}", stats.pruned);
    println!("   ⏱️  Took:          {}ms", stats.took_ms);

    Ok(())
}

fn cmd_search(root: &PathBuf, query: &str, json: bool, limit: usize) -> Result<()> {
    let root = std::fs::canonicalize(root)?;
    let config = ArgosConfig::load_from_root(&root)?;
    let engine = ArgosEngine::new(root, config)?;

    let options = SearchOptions::default().with_limit(limit);
    let result = engine.search(query, &options)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_search_results(&result);
    }

    Ok(())
}

fn cmd_stats(root: &PathBuf) -> Result<()> {
    let root = std::fs::canonicalize(root)?;
    let config = ArgosConfig::load_from_root(&root)?;
    let engine = ArgosEngine::new(root, config)?;

    let count = engine.indexed_count()?;
    println!("🔍 Argos Search — Index Stats");
    for r in engine.roots() {
        println!("   📂 Root:    {}", r.display());
    }
    println!("   📄 Files:   {}", count);

    Ok(())
}

fn print_search_results(result: &SearchResult) {
    println!(
        "🔍 \"{}\" — {} results in {}ms",
        result.query, result.total_hits, result.took_ms
    );
    println!();

    if result.hits.is_empty() {
        println!("   No results found.");
        return;
    }

    for (i, hit) in result.hits.iter().enumerate() {
        let size_str = hit
            .size_bytes
            .map(|s| format_size(s))
            .unwrap_or_default();
        let date_str = hit
            .modified
            .as_deref()
            .unwrap_or("");

        println!(
            "  {}. 📄 {} (score: {:.1})",
            i + 1,
            hit.name,
            hit.score
        );
        println!("     📂 {}", hit.path);
        if !size_str.is_empty() || !date_str.is_empty() {
            println!("     📊 {} | {}", size_str, date_str);
        }
        if let Some(snippet) = &hit.snippet {
            println!("     💬 ...{}...", snippet);
        }
        println!();
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
