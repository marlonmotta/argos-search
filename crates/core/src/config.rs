//! Configuration management for Argos Search.
//!
//! Loads settings from `config.toml` — roots, excludes, extensions, limits, threads.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Main configuration struct, parsed from `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgosConfig {
    /// Root directories to index. Empty = auto-detect user directories.
    #[serde(default)]
    pub roots: Vec<PathBuf>,

    /// Folder/pattern names to exclude from indexing (case-insensitive).
    #[serde(default = "default_excludes")]
    pub excludes: Vec<String>,

    /// File extensions to index with full content extraction.
    #[serde(default = "default_include_extensions")]
    pub include_extensions: Vec<String>,

    /// Max file size in bytes for plain text extraction (default: 2 MB).
    #[serde(default = "default_max_file_size")]
    pub max_file_size_bytes: u64,

    /// Max file size for structured files like PDF/DOCX (default: 10 MB, 0 = disabled).
    #[serde(default = "default_max_structured_file_size")]
    pub max_structured_file_size_bytes: u64,

    /// Hash threshold for small file dedup in bytes (default: 64 KB).
    #[serde(default = "default_hash_threshold")]
    pub hash_small_file_threshold: u64,

    /// Number of threads (0 = use all available CPU cores).
    #[serde(default)]
    pub threads: usize,
}

impl Default for ArgosConfig {
    fn default() -> Self {
        Self {
            roots: Vec::new(), // empty = auto-detect
            excludes: default_excludes(),
            include_extensions: default_include_extensions(),
            max_file_size_bytes: default_max_file_size(),
            max_structured_file_size_bytes: default_max_structured_file_size(),
            hash_small_file_threshold: default_hash_threshold(),
            threads: 0,
        }
    }
}

impl ArgosConfig {
    /// Load config from a TOML file. Falls back to defaults if file doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: Self = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Load config from a root directory, looking for `config.toml` inside it.
    pub fn load_from_root(root: &Path) -> Result<Self> {
        let config_path = root.join("config.toml");
        Self::load(&config_path)
    }

    /// Load global config from `~/.argos/config.toml`.
    pub fn load_global() -> Result<Self> {
        let config_path = Self::global_data_dir().join("config.toml");
        Self::load(&config_path)
    }

    /// Get the effective roots: configured roots, scope-based, or auto-detected.
    pub fn effective_roots(&self) -> Vec<PathBuf> {
        if self.roots.is_empty() {
            SearchScope::Personal.roots()
        } else {
            self.roots.iter().filter(|r| r.exists()).cloned().collect()
        }
    }
}

/// Search scope presets — controls how deep Argos digs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SearchScope {
    /// Projects, Desktop, Downloads only
    Personal,
    /// + Documents, other drives/SSDs
    Extended,
    /// + Program Files, installed software
    Full,
    /// Everything including Windows system dirs
    System,
}

impl SearchScope {
    /// Get root directories for this scope.
    pub fn roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\Default"));

        match self {
            SearchScope::Personal => {
                // Work/dev + personal content only
                let dirs = [
                    "Projects", "Projetos", "Code", "Dev", "Work",
                    "Repos", "repos", "src",
                    "Developer", // macOS
                    "Desktop", "Downloads",
                ];
                for name in &dirs {
                    let dir = home.join(name);
                    if dir.exists() && dir.is_dir() {
                        roots.push(dir);
                    }
                }
                if roots.is_empty() {
                    roots.push(home);
                }
            }
            SearchScope::Extended => {
                // Personal + Documents + other drives
                roots = SearchScope::Personal.roots();
                let extra = ["Documents", "Documentos", "Music", "Videos", "Pictures", "OneDrive"];
                for name in &extra {
                    let dir = home.join(name);
                    if dir.exists() && dir.is_dir() && !roots.contains(&dir) {
                        roots.push(dir);
                    }
                }
                // Scan for other drives (D:, E:, etc.)
                #[cfg(target_os = "windows")]
                for letter in b'D'..=b'Z' {
                    let drive = PathBuf::from(format!("{}:\\", letter as char));
                    if drive.exists() && !roots.contains(&drive) {
                        roots.push(drive);
                    }
                }
                // macOS: /Volumes
                #[cfg(target_os = "macos")]
                {
                    if let Ok(entries) = std::fs::read_dir("/Volumes") {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_dir() && !roots.contains(&path) {
                                roots.push(path);
                            }
                        }
                    }
                }
            }
            SearchScope::Full => {
                // Extended + Program Files
                roots = SearchScope::Extended.roots();
                #[cfg(target_os = "windows")]
                {
                    let prog_dirs = [
                        "C:\\Program Files",
                        "C:\\Program Files (x86)",
                        "C:\\ProgramData",
                    ];
                    for dir in &prog_dirs {
                        let path = PathBuf::from(dir);
                        if path.exists() && !roots.contains(&path) {
                            roots.push(path);
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    let app_dirs = ["/Applications", "/usr/local"];
                    for dir in &app_dirs {
                        let path = PathBuf::from(dir);
                        if path.exists() && !roots.contains(&path) {
                            roots.push(path);
                        }
                    }
                }
            }
            SearchScope::System => {
                // Everything
                roots = SearchScope::Full.roots();
                #[cfg(target_os = "windows")]
                {
                    let sys = PathBuf::from("C:\\Windows");
                    if sys.exists() && !roots.contains(&sys) {
                        roots.push(sys);
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    let sys_dirs = ["/System", "/Library"];
                    for dir in &sys_dirs {
                        let path = PathBuf::from(dir);
                        if path.exists() && !roots.contains(&path) {
                            roots.push(path);
                        }
                    }
                }
            }
        }

        roots
    }

    /// Get the excludes that should be REMOVED for this scope level.
    /// Higher scopes exclude fewer directories.
    pub fn excludes_override(&self) -> Vec<&'static str> {
        match self {
            SearchScope::Personal => vec![],  // Use all default excludes
            SearchScope::Extended => vec![],  // Same excludes, just more roots
            SearchScope::Full => vec!["Program Files", "Program Files (x86)", "ProgramData"],
            SearchScope::System => vec!["Program Files", "Program Files (x86)", "ProgramData", "Windows", "System32", "SysWOW64"],
        }
    }

    /// Display name for UI.
    pub fn label(&self) -> &'static str {
        match self {
            SearchScope::Personal => "🏠 Personal",
            SearchScope::Extended => "💿 Extended",
            SearchScope::Full => "📦 Full",
            SearchScope::System => "🖥️ System",
        }
    }

    /// Description for UI tooltip.
    pub fn description(&self) -> &'static str {
        match self {
            SearchScope::Personal => "Projects, Desktop, Downloads",
            SearchScope::Extended => "Personal + Documents + other drives",
            SearchScope::Full => "Extended + installed programs",
            SearchScope::System => "Everything including Windows/macOS system",
        }
    }

    /// From string (for IPC).
    pub fn from_str_name(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "extended" => SearchScope::Extended,
            "full" => SearchScope::Full,
            "system" => SearchScope::System,
            _ => SearchScope::Personal,
        }
    }
}

// Additional ArgosConfig methods (split because SearchScope enum is defined between)
impl ArgosConfig {
    /// Check if a directory name should be excluded.
    pub fn is_excluded(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        self.excludes
            .iter()
            .any(|ex| name_lower == ex.to_lowercase())
    }

    /// Check if a file extension should be indexed with content extraction.
    pub fn should_extract_content(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext_lower = ext.to_lowercase();
                self.include_extensions
                    .iter()
                    .any(|inc| inc.to_lowercase() == ext_lower)
            })
            .unwrap_or(false)
    }

    /// Global data directory: ~/.argos/
    pub fn global_data_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".argos")
    }

    /// Get the data directory path for index and metadata storage.
    /// Uses global ~/.argos/ for system-wide search.
    pub fn data_dir(_root: &Path) -> PathBuf {
        Self::global_data_dir()
    }

    /// Get the Tantivy index directory path.
    pub fn index_dir(root: &Path) -> PathBuf {
        Self::data_dir(root).join("index")
    }

    /// Get the SQLite database path.
    pub fn db_path(root: &Path) -> PathBuf {
        Self::data_dir(root).join("metadata.db")
    }
}

// --- Default value functions for serde ---

fn default_excludes() -> Vec<String> {
    vec![
        // Build & deps
        "node_modules", ".git", "target", ".argos", "dist", "build",
        "__pycache__", ".mypy_cache", ".tox", ".eggs", "*.egg-info",
        ".gradle", ".m2", "vendor", "Pods",
        // IDEs
        ".vscode", ".idea", ".vs", ".eclipse",
        // Windows system
        "AppData", "Application Data", "Local Settings",
        "Program Files", "Program Files (x86)", "ProgramData",
        "Windows", "System32", "SysWOW64",
        "$RECYCLE.BIN", "System Volume Information",
        "Recovery", "PerfLog",
        // Windows user junk
        "My Games", "Saved Games", "Favorites",
        "Links", "Contacts", "Searches", "3D Objects",
        ".thumbnails", "Thumbs.db",
        // Temp & cache
        "Temp", "tmp", "Cache", "cache", "CachedData",
        "GPUCache", "ShaderCache", "Code Cache",
        "Service Worker", "ScriptCache",
        // Package managers
        ".npm", ".yarn", ".pnpm-store", ".cargo",
        ".rustup", ".nuget", ".conda", ".pip",
        // macOS system
        ".Trash", "Library", ".DS_Store",
        // Logs
        "logs", "log",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_include_extensions() -> Vec<String> {
    vec![
        "txt", "md", "rst", "log",
        "rs", "py", "js", "ts", "go", "cs", "cpp", "c", "h", "java", "rb", "php",
        "toml", "yaml", "yml", "json", "xml", "ini", "env", "cfg", "conf",
        "sh", "bash", "ps1", "bat", "cmd",
        "html", "css", "svg",
        "sql",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_max_file_size() -> u64 {
    2_097_152 // 2 MB
}

fn default_max_structured_file_size() -> u64 {
    10_485_760 // 10 MB
}

fn default_hash_threshold() -> u64 {
    65_536 // 64 KB
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ArgosConfig::default();
        assert!(!config.excludes.is_empty());
        assert!(!config.include_extensions.is_empty());
        assert_eq!(config.max_file_size_bytes, 2_097_152);
        assert_eq!(config.threads, 0);
    }

    #[test]
    fn test_is_excluded() {
        let config = ArgosConfig::default();
        assert!(config.is_excluded("node_modules"));
        assert!(config.is_excluded("Node_Modules")); // case-insensitive
        assert!(config.is_excluded(".git"));
        assert!(!config.is_excluded("src"));
    }

    #[test]
    fn test_should_extract_content() {
        let config = ArgosConfig::default();
        assert!(config.should_extract_content(Path::new("test.rs")));
        assert!(config.should_extract_content(Path::new("readme.md")));
        assert!(config.should_extract_content(Path::new("data.json")));
        assert!(!config.should_extract_content(Path::new("image.png")));
        assert!(!config.should_extract_content(Path::new("binary.exe")));
    }
}
