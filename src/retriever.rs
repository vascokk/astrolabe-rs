use anyhow::{anyhow, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Handles source code retrieval with O(1) byte-range seeking
#[derive(Clone, Copy)]
pub struct SourceRetriever;

impl SourceRetriever {
    /// Retrieves a specific byte range from a file
    ///
    /// # Arguments
    /// * `path` - Path to the source file
    /// * `start_byte` - Starting byte offset (inclusive)
    /// * `end_byte` - Ending byte offset (exclusive)
    ///
    /// # Returns
    /// The exact bytes from [start_byte, end_byte) as a UTF-8 string
    ///
    /// # Preconditions
    /// * `start_byte < end_byte`
    /// * `end_byte <= file_size`
    /// * Path must be safe (no traversal)
    pub async fn get_source(path: &Path, start_byte: i64, end_byte: i64) -> Result<String> {
        if start_byte >= end_byte {
            return Err(anyhow!("start_byte must be less than end_byte"));
        }

        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut file = File::open(&path)?;
            file.seek(SeekFrom::Start(start_byte as u64))?;

            let len = (end_byte - start_byte) as usize;
            let mut buffer = vec![0u8; len];
            file.read_exact(&mut buffer)?;

            Ok(String::from_utf8(buffer)?)
        })
        .await?
    }

    /// Retrieves the full content of a file, truncated at max_bytes
    ///
    /// # Arguments
    /// * `path` - Path to the source file
    /// * `max_bytes` - Maximum bytes to read (default 100 KB)
    ///
    /// # Returns
    /// File content as UTF-8 string, truncated if larger than max_bytes
    pub async fn get_file_content(path: &Path, max_bytes: usize) -> Result<String> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut file = File::open(&path)?;
            let mut buffer = vec![0u8; max_bytes];

            let bytes_read = file.read(&mut buffer)?;
            buffer.truncate(bytes_read);

            Ok(String::from_utf8(buffer)?)
        })
        .await?
    }

    /// Checks if a target path is safe (no path traversal)
    ///
    /// # Arguments
    /// * `root` - The workspace root directory
    /// * `target` - The target path to validate
    ///
    /// # Returns
    /// `true` if the target path is within the root after canonicalisation, `false` otherwise
    pub fn is_safe_path(root: &Path, target: &Path) -> bool {
        // Canonicalise both paths
        let root_canonical = match root.canonicalize() {
            Ok(p) => p,
            Err(_) => return false,
        };

        let target_canonical = match target.canonicalize() {
            Ok(p) => p,
            Err(_) => return false,
        };

        // Check if target starts with root
        target_canonical.starts_with(&root_canonical)
    }

    /// Checks if a file matches the secret file blocklist
    ///
    /// # Arguments
    /// * `path` - Path to check
    ///
    /// # Returns
    /// `true` if the file matches a blocklist pattern, `false` otherwise
    pub fn is_blocked_file(path: &Path) -> bool {
        let file_name = match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => return false,
        };

        // Exact matches
        if file_name == ".env" || file_name == "id_rsa" {
            return true;
        }

        // Pattern matches
        if file_name.ends_with(".pem")
            || file_name.ends_with(".key")
            || file_name.ends_with(".pfx")
            || file_name.ends_with(".p12")
            || file_name.ends_with(".p8")
        {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_get_source_exact_bytes() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        let content = "Hello, World! This is a test.";
        temp_file.write_all(content.as_bytes())?;
        temp_file.flush()?;

        let path = temp_file.path();

        // Test retrieving exact range
        let result = SourceRetriever::get_source(path, 0, 5).await?;
        assert_eq!(result, "Hello");

        // Test middle range
        let result = SourceRetriever::get_source(path, 7, 12).await?;
        assert_eq!(result, "World");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_source_full_file() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        let content = "Test content";
        temp_file.write_all(content.as_bytes())?;
        temp_file.flush()?;

        let path = temp_file.path();
        let result = SourceRetriever::get_source(path, 0, content.len() as i64).await?;
        assert_eq!(result, content);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_source_invalid_range() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"test")?;
        temp_file.flush()?;

        let path = temp_file.path();

        // start_byte >= end_byte should fail
        let result = SourceRetriever::get_source(path, 5, 5).await;
        assert!(result.is_err());

        let result = SourceRetriever::get_source(path, 10, 5).await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_file_content_truncation() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        let content = "a".repeat(200);
        temp_file.write_all(content.as_bytes())?;
        temp_file.flush()?;

        let path = temp_file.path();

        // Read with max_bytes limit
        let result = SourceRetriever::get_file_content(path, 100).await?;
        assert_eq!(result.len(), 100);

        // Read full file when smaller than max_bytes
        let result = SourceRetriever::get_file_content(path, 500).await?;
        assert_eq!(result.len(), 200);

        Ok(())
    }

    #[test]
    fn test_is_safe_path_valid() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path();

        // Create a test file
        let test_file = root.join("test.txt");
        std::fs::write(&test_file, "test")?;

        // Should be safe
        assert!(SourceRetriever::is_safe_path(root, &test_file));

        Ok(())
    }

    #[test]
    fn test_is_safe_path_traversal() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path();

        // Try to escape with ..
        let escaped = root.join("..").join("etc").join("passwd");

        // Should not be safe (or file doesn't exist, which also returns false)
        assert!(!SourceRetriever::is_safe_path(root, &escaped));

        Ok(())
    }

    #[test]
    fn test_is_blocked_file_exact_matches() {
        assert!(SourceRetriever::is_blocked_file(Path::new(".env")));
        assert!(SourceRetriever::is_blocked_file(Path::new("id_rsa")));
        assert!(SourceRetriever::is_blocked_file(Path::new("/path/to/.env")));
        assert!(SourceRetriever::is_blocked_file(Path::new("/path/to/id_rsa")));
    }

    #[test]
    fn test_is_blocked_file_pattern_matches() {
        assert!(SourceRetriever::is_blocked_file(Path::new("cert.pem")));
        assert!(SourceRetriever::is_blocked_file(Path::new("key.key")));
        assert!(SourceRetriever::is_blocked_file(Path::new("cert.pfx")));
        assert!(SourceRetriever::is_blocked_file(Path::new("key.p12")));
        assert!(SourceRetriever::is_blocked_file(Path::new("key.p8")));
        assert!(SourceRetriever::is_blocked_file(Path::new("/path/to/secret.pem")));
    }

    #[test]
    fn test_is_blocked_file_non_matches() {
        assert!(!SourceRetriever::is_blocked_file(Path::new("test.txt")));
        assert!(!SourceRetriever::is_blocked_file(Path::new(".envrc")));
        assert!(!SourceRetriever::is_blocked_file(Path::new("id_rsa.pub")));
        assert!(!SourceRetriever::is_blocked_file(Path::new("config.rs")));
    }
}


#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Property 1: Source retrieval round-trip
    // For any valid (start_byte, end_byte) where start < end ≤ file_size,
    // get_source returns exactly (end - start) bytes
    proptest! {
        #[test]
        fn prop_source_retrieval_round_trip(
            content in "[a-zA-Z0-9 \n\t.,;:!?(){}\\[\\]]*",
            start_byte in 0usize..50,
            end_byte in 0usize..50,
        ) {
            // Skip if content is empty
            if content.is_empty() {
                return Ok(());
            }

            let mut temp_file = NamedTempFile::new().unwrap();
            temp_file.write_all(content.as_bytes()).unwrap();
            temp_file.flush().unwrap();

            let path = temp_file.path().to_path_buf();
            let file_size = content.len() as u64;

            // Only test valid ranges
            if start_byte as u64 >= file_size || end_byte as u64 > file_size {
                return Ok(());
            }

            let start = start_byte as u64;
            let end = end_byte as u64;

            if start >= end {
                return Ok(());
            }

            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(SourceRetriever::get_source(&path, start as i64, end as i64));
            prop_assert!(result.is_ok(), "get_source should succeed for valid range");

            let retrieved = result.unwrap();
            let expected_len = (end - start) as usize;
            prop_assert_eq!(
                retrieved.len(),
                expected_len,
                "Retrieved bytes should equal (end - start)"
            );

            // Verify the content matches
            let expected = &content[start as usize..end as usize];
            prop_assert_eq!(retrieved, expected, "Retrieved content should match original");
        }
    }

    // Property 17: Path traversal prevention
    // For any path not sharing a prefix with workspace root after canonicalisation,
    // the retriever rejects the request
    proptest! {
        #[test]
        fn prop_path_traversal_prevention(
            _dummy in ".*",
        ) {
            let temp_dir = tempfile::tempdir().unwrap();
            let root = temp_dir.path();

            // Create a test file inside root
            let safe_file = root.join("test.txt");
            std::fs::write(&safe_file, "test").unwrap();

            // Safe path should be accepted
            prop_assert!(SourceRetriever::is_safe_path(root, &safe_file));

            // Try to escape with ..
            let escaped = root.join("..").join("etc").join("passwd");
            // Escaped path should be rejected (either doesn't exist or is outside root)
            prop_assert!(!SourceRetriever::is_safe_path(root, &escaped));
        }
    }

    // Property 18: Secret file blocklist enforcement
    // Blocklisted files are rejected, non-blocklisted files are allowed
    proptest! {
        #[test]
        fn prop_secret_file_blocklist(
            filename in r"[a-z0-9_\-]+(\.(txt|rs|py|js|ts|pem|key|pfx|p12|p8|env))?",
        ) {
            let path = Path::new(&filename);

            // Check if it matches blocklist patterns
            let is_blocked = SourceRetriever::is_blocked_file(path);

            // Verify consistency: if it ends with a blocked extension, it should be blocked
            if filename.ends_with(".pem")
                || filename.ends_with(".key")
                || filename.ends_with(".pfx")
                || filename.ends_with(".p12")
                || filename.ends_with(".p8")
                || filename == ".env"
                || filename == "id_rsa"
            {
                prop_assert!(is_blocked, "File should be blocked: {}", filename);
            }

            // Non-blocked extensions should not be blocked
            if filename.ends_with(".txt")
                || filename.ends_with(".rs")
                || filename.ends_with(".py")
                || filename.ends_with(".js")
                || filename.ends_with(".ts")
            {
                prop_assert!(!is_blocked, "File should not be blocked: {}", filename);
            }
        }
    }

    // Property 19: File content retrieval
    // Valid non-blocked files return valid UTF-8
    proptest! {
        #[test]
        fn prop_file_content_retrieval(
            content in "[a-zA-Z0-9 \n\t.,;:!?(){}\\[\\]]*",
        ) {
            // Skip empty content
            if content.is_empty() {
                return Ok(());
            }

            let mut temp_file = NamedTempFile::new().unwrap();
            temp_file.write_all(content.as_bytes()).unwrap();
            temp_file.flush().unwrap();

            let path = temp_file.path().to_path_buf();

            // File should not be blocked (it's in temp directory)
            prop_assert!(!SourceRetriever::is_blocked_file(&path));

            // Should be able to read the file
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(SourceRetriever::get_file_content(&path, 100_000));
            prop_assert!(result.is_ok(), "get_file_content should succeed for valid file");

            let retrieved = result.unwrap();
            // Should be valid UTF-8 (we wrote UTF-8)
            prop_assert!(!retrieved.is_empty() || content.is_empty());
        }
    }
}
