use std::path::Path;

use anyhow::{Context, Result};
use glob::glob;

/// Find all `.env*` files in a directory matching the given patterns.
pub fn find_env_files(dir: &Path, patterns: &[String]) -> Result<Vec<String>> {
    let mut files = Vec::new();

    for pattern in patterns {
        let full_pattern = dir.join(pattern).to_string_lossy().to_string();
        for path in glob(&full_pattern).context("Invalid glob pattern")?.flatten() {
            if path.is_file() {
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy().to_string();
                    if !files.contains(&name_str) {
                        files.push(name_str);
                    }
                }
            }
        }
    }

    files.sort();
    Ok(files)
}

/// Copy env files from source worktree to target worktree.
pub fn copy_env_files(
    source_dir: &Path,
    target_dir: &Path,
    patterns: &[String],
) -> Result<Vec<String>> {
    let files = find_env_files(source_dir, patterns)?;
    let mut copied = Vec::new();

    for file_name in &files {
        let src = source_dir.join(file_name);
        let dst = target_dir.join(file_name);

        std::fs::copy(&src, &dst)
            .with_context(|| format!("Failed to copy {file_name}"))?;
        copied.push(file_name.clone());
    }

    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_env_files_in_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let patterns = vec![".env".to_owned(), ".env.*".to_owned()];
        let files = find_env_files(dir.path(), &patterns).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn find_env_files_matches_patterns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "KEY=val").unwrap();
        std::fs::write(dir.path().join(".env.local"), "KEY=val").unwrap();
        std::fs::write(dir.path().join(".env.production"), "KEY=val").unwrap();
        std::fs::write(dir.path().join("not-env.txt"), "nope").unwrap();

        let patterns = vec![
            ".env".to_owned(),
            ".env.local".to_owned(),
            ".env.*".to_owned(),
        ];
        let files = find_env_files(dir.path(), &patterns).unwrap();
        assert_eq!(files.len(), 3);
        assert!(files.contains(&".env".to_owned()));
        assert!(files.contains(&".env.local".to_owned()));
        assert!(files.contains(&".env.production".to_owned()));
    }

    #[test]
    fn copy_env_files_works() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        std::fs::write(src.path().join(".env"), "SECRET=123").unwrap();
        std::fs::write(src.path().join(".env.local"), "LOCAL=456").unwrap();

        let patterns = vec![".env".to_owned(), ".env.*".to_owned()];
        let copied = copy_env_files(src.path(), dst.path(), &patterns).unwrap();

        assert_eq!(copied.len(), 2);
        assert!(dst.path().join(".env").exists());
        assert!(dst.path().join(".env.local").exists());

        let content = std::fs::read_to_string(dst.path().join(".env")).unwrap();
        assert_eq!(content, "SECRET=123");
    }
}
