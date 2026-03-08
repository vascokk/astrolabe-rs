use crate::models::TextMatch;
use anyhow::{anyhow, Result};
use ignore::WalkBuilder;
use regex::Regex;
use std::path::PathBuf;

/// Full-text searcher that walks files respecting .gitignore and searches with regex
pub struct FullTextSearcher {
    root: PathBuf,
}

impl FullTextSearcher {
    /// Create a new FullTextSearcher for the given root directory
    pub fn new(root: PathBuf) -> Self {
        FullTextSearcher { root }
    }

    /// Search for a regex pattern in files, respecting .gitignore
    ///
    /// # Arguments
    /// * `pattern` - A valid regex pattern to search for
    /// * `max_results` - Maximum number of results to return (capped at 200, default 50)
    ///
    /// # Returns
    /// A vector of TextMatch results, or an error if the regex is invalid
    pub fn search(&self, pattern: &str, max_results: usize) -> Result<Vec<TextMatch>> {
        // Validate and compile the regex
        let regex = Regex::new(pattern)
            .map_err(|e| anyhow!("Invalid regex pattern: {}", e))?;

        // Cap max_results: default 50, absolute max 200
        let effective_max = if max_results == 0 {
            50
        } else {
            std::cmp::min(max_results, 200)
        };

        let mut matches = Vec::new();

        // Walk files respecting .gitignore
        let walker = WalkBuilder::new(&self.root)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.filter_map(|e| e.ok()) {
            if matches.len() >= effective_max {
                break;
            }

            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Try to read the file
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    // Search each line
                    for (line_number, line_content) in content.lines().enumerate() {
                        if matches.len() >= effective_max {
                            break;
                        }

                        // Find all matches in this line
                        for mat in regex.find_iter(line_content) {
                            if matches.len() >= effective_max {
                                break;
                            }

                            matches.push(TextMatch {
                                file_path: path
                                    .strip_prefix(&self.root)
                                    .unwrap_or(path)
                                    .to_string_lossy()
                                    .to_string(),
                                line_number: (line_number + 1) as u32,
                                line_content: line_content.to_string(),
                                column_start: mat.start() as u32,
                            });
                        }
                    }
                }
                Err(_) => {
                    // Skip files that can't be read (binary files, permission errors, etc.)
                    continue;
                }
            }
        }

        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_search_finds_matches() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "hello world\nfoo bar\nhello again").unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("hello", 50).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].line_number, 1);
        assert_eq!(results[0].line_content, "hello world");
        assert_eq!(results[0].column_start, 0);
        assert_eq!(results[1].line_number, 3);
        assert_eq!(results[1].line_content, "hello again");
    }

    #[test]
    fn test_search_respects_max_results() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "a\na\na\na\na").unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("a", 2).unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_defaults_to_50_max() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        let content = (0..100).map(|_| "match").collect::<Vec<_>>().join("\n");
        fs::write(&test_file, content).unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("match", 0).unwrap();

        assert_eq!(results.len(), 50);
    }

    #[test]
    fn test_search_caps_at_200() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        let content = (0..300).map(|_| "match").collect::<Vec<_>>().join("\n");
        fs::write(&test_file, content).unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("match", 300).unwrap();

        assert_eq!(results.len(), 200);
    }

    #[test]
    fn test_search_invalid_regex() {
        let temp_dir = TempDir::new().unwrap();
        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let result = searcher.search("[invalid(regex", 50);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid regex"));
    }

    #[test]
    fn test_search_respects_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        
        // Initialize a git repo so .gitignore is respected
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(temp_dir.path())
            .output();
        
        let gitignore_file = temp_dir.path().join(".gitignore");
        fs::write(&gitignore_file, "ignored.txt").unwrap();

        let ignored_file = temp_dir.path().join("ignored.txt");
        fs::write(&ignored_file, "secret content").unwrap();

        let included_file = temp_dir.path().join("included.txt");
        fs::write(&included_file, "secret content").unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("secret", 50).unwrap();

        // Should only find the match in included.txt, not ignored.txt
        assert_eq!(results.len(), 1);
        assert!(results[0].file_path.contains("included.txt"));
    }

    #[test]
    fn test_search_column_start() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "foo bar baz").unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("bar", 50).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].column_start, 4);
    }

    #[test]
    fn test_search_multiple_matches_per_line() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "foo foo foo").unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("foo", 50).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].column_start, 0);
        assert_eq!(results[1].column_start, 4);
        assert_eq!(results[2].column_start, 8);
    }
}
