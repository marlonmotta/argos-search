//! Main search engine combining Tantivy full-text index with filesystem crawling.
//!
//! The engine handles:
//! - Filesystem traversal with `walkdir` + `rayon`
//! - Incremental indexing via mtime/size/hash checks against SQLite
//! - Full-text search with Tantivy and custom re-ranking
//! - JSON-serializable search results

use crate::config::ArgosConfig;
use crate::extractors;
use crate::metadata::{FileRecord, MetadataStore};
use anyhow::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::tokenizer::{SimpleTokenizer, LowerCaser, TextAnalyzer, Token, TokenFilter, Tokenizer, TokenStream};
use tantivy::{Index, IndexReader, ReloadPolicy, TantivyDocument};
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

// ─── ASCII Folding Filter (accent-insensitive search) ──────────────

/// Strips diacritical marks from tokens: "mãe" → "mae", "café" → "cafe".
#[derive(Clone)]
struct AsciiFoldingFilter;

impl TokenFilter for AsciiFoldingFilter {
    type Tokenizer<T: Tokenizer> = AsciiFoldingFilterWrapper<T>;

    fn transform<T: Tokenizer>(self, tokenizer: T) -> Self::Tokenizer<T> {
        AsciiFoldingFilterWrapper(tokenizer)
    }
}

#[derive(Clone)]
struct AsciiFoldingFilterWrapper<T>(T);

impl<T: Tokenizer> Tokenizer for AsciiFoldingFilterWrapper<T> {
    type TokenStream<'a> = AsciiFoldingTokenStream<T::TokenStream<'a>>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        AsciiFoldingTokenStream(self.0.token_stream(text))
    }
}

struct AsciiFoldingTokenStream<T>(T);

impl<T: TokenStream> TokenStream for AsciiFoldingTokenStream<T> {
    fn advance(&mut self) -> bool {
        if !self.0.advance() {
            return false;
        }
        // NFD decomposition strips accents: 'ã' → 'a' + combining '~'
        let folded: String = self.0.token().text
            .nfd()
            .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
            .collect();
        self.0.token_mut().text = folded;
        true
    }

    fn token(&self) -> &Token {
        self.0.token()
    }

    fn token_mut(&mut self) -> &mut Token {
        self.0.token_mut()
    }
}

/// A single search hit with path, score, and optional snippet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub name: String,
    pub score: f32,
    pub snippet: Option<String>,
    pub size_bytes: Option<u64>,
    pub modified: Option<String>,
}

/// Search results container with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub total_hits: usize,
    pub took_ms: u64,
    pub hits: Vec<SearchHit>,
}

/// Search options for customizing query behavior.
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub limit: usize,
    pub json_output: bool,
}

impl SearchOptions {
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// The main search engine.
pub struct ArgosEngine {
    roots: Vec<PathBuf>,
    config: ArgosConfig,
    index: Index,
    reader: IndexReader,
    metadata: MetadataStore,
    // Schema fields
    field_path: Field,
    field_name: Field,
    field_content: Field,
    field_mtime: Field,
}

impl ArgosEngine {
    /// Create a new engine for the given root directory (single-root, backwards compatible).
    pub fn new(root: PathBuf, config: ArgosConfig) -> Result<Self> {
        Self::new_multi(vec![root], config)
    }

    /// Create a new engine using global config and auto-detected user roots.
    /// Index and metadata are stored in ~/.argos/
    pub fn new_global() -> Result<Self> {
        let config = ArgosConfig::load_global().unwrap_or_default();
        let roots = config.effective_roots();
        Self::new_multi(roots, config)
    }

    /// Create a new engine with multiple root directories.
    pub fn new_multi(roots: Vec<PathBuf>, config: ArgosConfig) -> Result<Self> {
        let global_dir = ArgosConfig::global_data_dir();
        let index_dir = global_dir.join("index");
        let db_path = global_dir.join("metadata.db");

        // Create data directories
        std::fs::create_dir_all(&index_dir)?;

        // Build Tantivy schema with custom tokenizer for accent-insensitive search
        let argos_text = TextOptions::default()
            .set_indexing_options(TextFieldIndexing::default()
                .set_tokenizer("argos")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions))
            .set_stored();
        let argos_text_unstored = TextOptions::default()
            .set_indexing_options(TextFieldIndexing::default()
                .set_tokenizer("argos")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions));

        let mut schema_builder = Schema::builder();
        let field_path = schema_builder.add_text_field("path", STRING | STORED);
        let field_name = schema_builder.add_text_field("name", argos_text);
        let field_content = schema_builder.add_text_field("content", argos_text_unstored); // NOT stored — saves space
        let field_mtime = schema_builder.add_i64_field("mtime", FAST | STORED);
        let schema = schema_builder.build();

        // Open or create Tantivy index (delete old index if schema changed)
        let index = if index_dir.join("meta.json").exists() {
            match Index::open_in_dir(&index_dir) {
                Ok(idx) => idx,
                Err(_) => {
                    // Schema mismatch — recreate index
                    std::fs::remove_dir_all(&index_dir)?;
                    std::fs::create_dir_all(&index_dir)?;
                    Index::create_in_dir(&index_dir, schema)?
                }
            }
        } else {
            Index::create_in_dir(&index_dir, schema)?
        };

        // Register custom tokenizer with ASCII folding (accent/diacritic removal)
        let argos_tokenizer = TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(LowerCaser)
            .filter(AsciiFoldingFilter)
            .build();
        index.tokenizers().register("argos", argos_tokenizer);

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        // Open metadata store
        let metadata = MetadataStore::open(&db_path)?;

        Ok(Self {
            roots,
            config,
            index,
            reader,
            metadata,
            field_path,
            field_name,
            field_content,
            field_mtime,
        })
    }

    /// Build/rebuild the full-text index by crawling the filesystem.
    pub fn index_build(&self) -> Result<IndexStats> {
        let start = std::time::Instant::now();

        // Collect all files to process
        let files_to_process = self.crawl_files()?;
        let total_found = files_to_process.len();

        // Phase 1: Collect file metadata + content in parallel (no SQLite access)
        let error_count = AtomicU64::new(0);
        let config = &self.config;

        let file_infos: Vec<Option<FileInfo>> = files_to_process
            .par_iter()
            .map(|file_path| {
                match collect_file_info(file_path, config) {
                    Ok(info) => Some(info),
                    Err(_) => {
                        error_count.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                }
            })
            .collect();

        // Phase 2: Check against SQLite + build Tantivy docs (single-threaded)
        let mut writer = self.index.writer(50_000_000)?;
        let mut indexed = 0u64;
        let mut skipped = 0u64;

        for info in file_infos.into_iter().flatten() {
            // Check if file changed since last index
            let needs_index = match self.metadata.get(&info.path_str)? {
                Some(record) => {
                    if record.mtime == info.mtime && record.size == info.size {
                        // mtime + size match — check hash for small files
                        if let Some(ref new_hash) = info.hash {
                            record.hash.as_deref() != Some(new_hash.as_str())
                        } else {
                            false // Large file, same mtime+size — skip
                        }
                    } else {
                        true // mtime or size changed
                    }
                }
                None => true, // New file
            };

            if !needs_index {
                skipped += 1;
                continue;
            }

            // Build Tantivy document
            let mut doc = TantivyDocument::default();
            doc.add_text(self.field_path, &info.path_str);
            doc.add_text(self.field_name, &info.name);
            doc.add_text(self.field_content, &info.content);
            doc.add_i64(self.field_mtime, info.mtime);

            // Delete old document then add new one
            let path_term = tantivy::Term::from_field_text(self.field_path, &info.path_str);
            writer.delete_term(path_term);
            writer.add_document(doc)?;

            // Update metadata store
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            self.metadata.upsert(&FileRecord {
                path: info.path_str,
                mtime: info.mtime,
                size: info.size,
                hash: info.hash,
                indexed_at: now,
            })?;

            indexed += 1;
        }

        writer.commit()?;
        self.reader.reload()?;

        // Prune deleted files
        let pruned = self.metadata.prune_missing()?;

        let elapsed = start.elapsed();

        Ok(IndexStats {
            total_found: total_found as u64,
            indexed,
            skipped,
            errors: error_count.load(Ordering::Relaxed),
            pruned,
            took_ms: elapsed.as_millis() as u64,
        })
    }

    /// Search the index for the given query.
    pub fn search(&self, query_str: &str, options: &SearchOptions) -> Result<SearchResult> {
        let start = std::time::Instant::now();
        let limit = if options.limit > 0 { options.limit } else { 20 };

        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.field_name, self.field_content],
        );

        let query = query_parser.parse_query(query_str)?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut hits = Vec::new();
        let query_terms: Vec<String> = query_str
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect();

        for (base_score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            let path = retrieved_doc
                .get_first(self.field_path)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let name = retrieved_doc
                .get_first(self.field_name)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Re-ranking: boost name matches and recent files
            let final_score = self.rerank(base_score, &name, &path, &query_terms, &retrieved_doc);

            // Get file metadata for display
            let (size_bytes, modified) = self.get_file_display_info(&path);

            hits.push(SearchHit {
                path,
                name,
                score: final_score,
                snippet: None, // TODO: generate snippets by re-reading file
                size_bytes,
                modified,
            });
        }

        // Sort by final score descending
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let elapsed = start.elapsed();

        Ok(SearchResult {
            query: query_str.to_string(),
            total_hits: hits.len(),
            took_ms: elapsed.as_millis() as u64,
            hits,
        })
    }

    /// Get the root paths this engine is configured for.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Get the total number of indexed files.
    pub fn indexed_count(&self) -> Result<u64> {
        self.metadata.count()
    }

    // --- Private methods ---

    /// Crawl the filesystem and return all eligible file paths.
    fn crawl_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for root in &self.roots {
            if !root.exists() {
                continue;
            }

            for entry in WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    // Skip excluded directories
                    if e.file_type().is_dir() {
                        let name = e.file_name().to_string_lossy();
                        return !self.config.is_excluded(&name);
                    }
                    true
                })
            {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue, // Skip permission errors, etc.
                };

                if entry.file_type().is_file() {
                    files.push(entry.into_path());
                }
            }
        }

        Ok(files)
    }

    /// Apply re-ranking heuristics on top of Tantivy's BM25 score.
    fn rerank(
        &self,
        base_score: f32,
        name: &str,
        path: &str,
        query_terms: &[String],
        _doc: &TantivyDocument,
    ) -> f32 {
        let name_lower = name.to_lowercase();
        let path_lower = path.to_lowercase();

        let mut score = base_score;

        if query_terms.is_empty() {
            return score;
        }

        // Ratio of query terms found in filename
        let name_matches = query_terms
            .iter()
            .filter(|t| name_lower.contains(t.as_str()))
            .count();
        let name_ratio = name_matches as f32 / query_terms.len() as f32;
        score += name_ratio * 2.2;

        // Ratio of query terms found in path
        let path_matches = query_terms
            .iter()
            .filter(|t| path_lower.contains(t.as_str()))
            .count();
        let path_ratio = path_matches as f32 / query_terms.len() as f32;
        score += path_ratio * 1.0;

        // Exact match bonus for filename
        if query_terms.iter().any(|t| name_lower == *t) {
            score += 1.8;
        }

        score
    }

    /// Get file size and modification time for display.
    fn get_file_display_info(&self, path: &str) -> (Option<u64>, Option<String>) {
        let p = PathBuf::from(path);
        match std::fs::metadata(&p) {
            Ok(meta) => {
                let size = Some(meta.len());
                let modified = meta
                    .modified()
                    .ok()
                    .map(|t| {
                        let duration = t.duration_since(UNIX_EPOCH).unwrap_or_default();
                        let dt = chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0);
                        dt.map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_default()
                    });
                (size, modified)
            }
            Err(_) => (None, None),
        }
    }
}

/// Pre-collected file info from the parallel phase (no SQLite access).
struct FileInfo {
    path_str: String,
    name: String,
    content: String,
    mtime: i64,
    size: i64,
    hash: Option<String>,
}

/// Collect file metadata and content in parallel (pure I/O, no shared state).
fn collect_file_info(path: &Path, config: &ArgosConfig) -> Result<FileInfo> {
    let fs_meta = std::fs::metadata(path)?;
    let mtime = fs_meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let size = fs_meta.len() as i64;
    let path_str = path.to_string_lossy().to_string();

    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Extract content if this is a supported text file
    let content = if config.should_extract_content(path) {
        extractors::extract_text(path, config.max_file_size_bytes)?
    } else {
        String::new()
    };

    // Compute hash for small files
    let hash = if (size as u64) < config.hash_small_file_threshold {
        Some(extractors::compute_hash(path)?)
    } else {
        None
    };

    Ok(FileInfo {
        path_str,
        name,
        content,
        mtime,
        size,
        hash,
    })
}

/// Statistics from an indexing operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_found: u64,
    pub indexed: u64,
    pub skipped: u64,
    pub errors: u64,
    pub pruned: u64,
    pub took_ms: u64,
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Indexed: {} | Skipped: {} | Errors: {} | Pruned: {} | Total: {} | Took: {}ms",
            self.indexed, self.skipped, self.errors, self.pruned, self.total_found, self.took_ms
        )
    }
}
