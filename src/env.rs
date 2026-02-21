use std::path::Path;

use anyhow::{Context, Result};

/// Directories to skip when recursively scanning for env files.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    ".worktrees",
    "dist",
    "build",
    ".next",
    ".turbo",
    "vendor",
];

/// Recursively find all `.env*` files in a directory matching the given patterns.
/// Returns relative paths (e.g. `apps/web/.env`) so they can be preserved when copying.
pub fn find_env_files(dir: &Path, patterns: &[String]) -> Result<Vec<String>> {
    let mut files = Vec::new();
    find_env_recursive(dir, dir, patterns, &mut files)?;
    files.sort();
    Ok(files)
}

fn find_env_recursive(
    root: &Path,
    current: &Path,
    patterns: &[String],
    files: &mut Vec<String>,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(current) else {
        return Ok(()); // skip unreadable dirs
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Skip noise directories
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if SKIP_DIRS.iter().any(|&skip| name_str == skip) {
                    continue;
                }
            }
            find_env_recursive(root, &path, patterns, files)?;
        } else if path.is_file() {
            // Check if file matches any env pattern
            if let Some(file_name) = path.file_name() {
                let file_name_str = file_name.to_string_lossy();
                if matches_env_patterns(&file_name_str, patterns) {
                    // Store relative path from root
                    if let Ok(relative) = path.strip_prefix(root) {
                        let rel_str = relative.to_string_lossy().to_string();
                        if !files.contains(&rel_str) {
                            files.push(rel_str);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Check if a filename matches any of the env patterns.
fn matches_env_patterns(filename: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if pattern.contains('*') {
            // Use glob matching against just the filename
            if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                if glob_pattern.matches(filename) {
                    return true;
                }
            }
        } else if filename == pattern {
            return true;
        }
    }
    false
}

/// Copy env files from source worktree to target worktree, preserving relative paths.
pub fn copy_env_files(
    source_dir: &Path,
    target_dir: &Path,
    patterns: &[String],
) -> Result<Vec<String>> {
    let files = find_env_files(source_dir, patterns)?;
    let mut copied = Vec::new();

    for rel_path in &files {
        let src = source_dir.join(rel_path);
        let dst = target_dir.join(rel_path);

        // Create parent directories in target if needed
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory for {rel_path}"))?;
        }

        std::fs::copy(&src, &dst)
            .with_context(|| format!("Failed to copy {rel_path}"))?;
        copied.push(rel_path.clone());
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
    fn find_env_files_matches_root_patterns() {
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
    fn find_env_files_recursive_in_subdirs() {
        let dir = tempfile::tempdir().unwrap();

        // Root env
        std::fs::write(dir.path().join(".env"), "ROOT=1").unwrap();

        // Nested app envs
        let web_dir = dir.path().join("apps").join("web");
        std::fs::create_dir_all(&web_dir).unwrap();
        std::fs::write(web_dir.join(".env"), "WEB=1").unwrap();
        std::fs::write(web_dir.join(".env.local"), "WEB_LOCAL=1").unwrap();

        let api_dir = dir.path().join("apps").join("api");
        std::fs::create_dir_all(&api_dir).unwrap();
        std::fs::write(api_dir.join(".env"), "API=1").unwrap();

        let patterns = vec![".env".to_owned(), ".env.*".to_owned()];
        let files = find_env_files(dir.path(), &patterns).unwrap();

        assert_eq!(files.len(), 4);
        assert!(files.contains(&".env".to_owned()));
        assert!(files.contains(&"apps/api/.env".to_owned()));
        assert!(files.contains(&"apps/web/.env".to_owned()));
        assert!(files.contains(&"apps/web/.env.local".to_owned()));
    }

    #[test]
    fn find_env_files_skips_node_modules() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join(".env"), "ROOT=1").unwrap();

        let nm_dir = dir.path().join("node_modules").join("some-pkg");
        std::fs::create_dir_all(&nm_dir).unwrap();
        std::fs::write(nm_dir.join(".env"), "SHOULD_SKIP=1").unwrap();

        let patterns = vec![".env".to_owned(), ".env.*".to_owned()];
        let files = find_env_files(dir.path(), &patterns).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files.contains(&".env".to_owned()));
    }

    #[test]
    fn copy_env_files_preserves_nested_paths() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // Create nested structure in source
        std::fs::write(src.path().join(".env"), "ROOT=123").unwrap();
        let web_dir = src.path().join("apps").join("web");
        std::fs::create_dir_all(&web_dir).unwrap();
        std::fs::write(web_dir.join(".env"), "WEB=456").unwrap();

        let patterns = vec![".env".to_owned(), ".env.*".to_owned()];
        let copied = copy_env_files(src.path(), dst.path(), &patterns).unwrap();

        assert_eq!(copied.len(), 2);

        // Check root was copied
        assert!(dst.path().join(".env").exists());
        let root_content = std::fs::read_to_string(dst.path().join(".env")).unwrap();
        assert_eq!(root_content, "ROOT=123");

        // Check nested was copied with correct path
        let nested_path = dst.path().join("apps").join("web").join(".env");
        assert!(nested_path.exists());
        let nested_content = std::fs::read_to_string(nested_path).unwrap();
        assert_eq!(nested_content, "WEB=456");
    }
}
