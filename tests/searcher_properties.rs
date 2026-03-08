use astrolabe_mcp::searcher::FullTextSearcher;
use proptest::prelude::*;
use std::fs;
use tempfile::TempDir;

// Property 15: full_text_search result limit
// **Validates: Requirements 8.2, 8.3**
// Returned matches ≤ min(specified_max_results or 50, 200)
proptest! {
    #[test]
    fn prop_full_text_search_result_limit(
        max_results in 0usize..=300,
        num_lines in 1usize..=300,
    ) {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        
        // Create a file with many lines containing "match"
        let content = (0..num_lines)
            .map(|_| "match")
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&test_file, content).unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("match", max_results).unwrap();

        // Calculate expected max
        let effective_max = if max_results == 0 { 50 } else { std::cmp::min(max_results, 200) };
        let expected_max = std::cmp::min(effective_max, num_lines);

        prop_assert_eq!(results.len(), expected_max);
    }
}

// Property 16: full_text_search match validity
// **Validates: Requirement 8.1**
// Every returned TextMatch contains required fields and line_content matches the regex
proptest! {
    #[test]
    fn prop_full_text_search_match_validity(
        pattern in r"[a-z]{2,5}",
        num_lines in 1usize..=50,
    ) {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        
        // Create a file with lines containing the pattern
        let content = (0..num_lines)
            .map(|i| format!("line {} with {} pattern", i, pattern))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&test_file, content).unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search(&pattern, 200).unwrap();

        // Verify all results have required fields
        for result in &results {
            prop_assert!(!result.file_path.is_empty(), "file_path must not be empty");
            prop_assert!(result.line_number > 0, "line_number must be > 0");
            prop_assert!(!result.line_content.is_empty(), "line_content must not be empty");
            
            // Verify line_content matches the pattern
            prop_assert!(
                result.line_content.contains(&pattern),
                "line_content must contain the pattern"
            );
        }
    }
}

// Property 20: Invalid regex error handling
// **Validates: Requirement 12.4**
// Invalid regex returns structured error, does not panic
proptest! {
    #[test]
    fn prop_invalid_regex_error_handling(
        invalid_pattern in r"[\[\(\{].*",  // Patterns that are likely invalid
    ) {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "some content").unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        
        // This should not panic, even with invalid regex
        let result = searcher.search(&invalid_pattern, 50);
        
        // If it's an invalid regex, we should get an error
        // If it's a valid regex, we should get Ok
        // Either way, we should not panic
        match result {
            Ok(_) => {
                // Valid regex, that's fine
            }
            Err(e) => {
                // Invalid regex should have a descriptive error
                prop_assert!(
                    e.to_string().contains("Invalid regex") || e.to_string().contains("regex"),
                    "Error should mention regex issue"
                );
            }
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

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

    #[test]
    fn test_search_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("anything", 50).unwrap();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search("xyz", 50).unwrap();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_regex_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test123\nfoo456\nbar789").unwrap();

        let searcher = FullTextSearcher::new(temp_dir.path().to_path_buf());
        let results = searcher.search(r"\d+", 50).unwrap();

        assert_eq!(results.len(), 3);
    }
}
