//! Content extractors for different file types.
//!
//! Extracts plain text from files for full-text indexing.
//! Handles UTF-8 text files, source code, and binary detection.

use anyhow::Result;
use std::path::Path;

/// Maximum bytes to read for binary detection (first 8KB).
const BINARY_CHECK_SIZE: usize = 8192;

/// Extract text content from a file for indexing.
///
/// Returns `Ok(String)` with the content, or empty string if the file
/// is binary or unreadable.
pub fn extract_text(path: &Path, max_size: u64) -> Result<String> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();

    // Skip files that exceed the size limit
    if size > max_size {
        return Ok(String::new());
    }

    // Read the file bytes
    let bytes = std::fs::read(path)?;

    // Check for binary content (null bytes in the first chunk)
    if is_binary(&bytes) {
        return Ok(String::new());
    }

    // Decode as UTF-8 with lossy conversion (handles Windows-1252, Latin-1, etc.)
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Check if file content appears to be binary by looking for null bytes.
fn is_binary(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(BINARY_CHECK_SIZE);
    bytes[..check_len].contains(&0)
}

/// Compute xxh3_64 hash of file content for dedup of small files.
pub fn compute_hash(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let hash = xxhash_rust::xxh3::xxh3_64(&bytes);
    Ok(format!("{:016x}", hash))
}

/// Get the file extension as a lowercase string, if any.
pub fn file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_text_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, Argos Search!").unwrap();

        let content = extract_text(&file_path, 2_097_152).unwrap();
        assert_eq!(content, "Hello, Argos Search!");
    }

    #[test]
    fn test_extract_empty_for_binary() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("binary.bin");
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(&[0x89, 0x50, 0x4E, 0x47, 0x00, 0x00]).unwrap();

        let content = extract_text(&file_path, 2_097_152).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_extract_skips_large_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("large.txt");
        let content_str = "x".repeat(1000);
        std::fs::write(&file_path, &content_str).unwrap();

        // Set max size to 500 bytes — should skip
        let content = extract_text(&file_path, 500).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_compute_hash() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("hashme.txt");
        std::fs::write(&file_path, "consistent content").unwrap();

        let hash1 = compute_hash(&file_path).unwrap();
        let hash2 = compute_hash(&file_path).unwrap();
        assert_eq!(hash1, hash2); // Deterministic
        assert_eq!(hash1.len(), 16); // 64-bit hex
    }

    #[test]
    fn test_file_extension() {
        assert_eq!(file_extension(Path::new("test.RS")), Some("rs".to_string()));
        assert_eq!(file_extension(Path::new("noext")), None);
    }
}
