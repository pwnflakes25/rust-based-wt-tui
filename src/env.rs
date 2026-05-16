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

/// Copy root-relative paths (files or directories) from source to target worktree.
/// Returns the list of paths that were successfully copied.
pub fn copy_path_entries(
    source_dir: &Path,
    target_dir: &Path,
    paths: &[String],
) -> Result<Vec<String>> {
    let mut copied = Vec::new();

    for rel_path in paths {
        let src = source_dir.join(rel_path);
        if !src.exists() {
            continue;
        }

        let dst = target_dir.join(rel_path);

        if src.is_dir() {
            copy_dir_recursive(&src, &dst)
                .with_context(|| format!("Failed to copy directory {rel_path}"))?;
        } else {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory for {rel_path}"))?;
            }
            std::fs::copy(&src, &dst)
                .with_context(|| format!("Failed to copy {rel_path}"))?;
        }
        copied.push(rel_path.clone());
    }

    Ok(copied)
}

/// Recursively copy a directory and all its contents.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Recursively collect all file and directory paths under `root`.
    fn walkdir(root: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        walkdir_recursive(root, &mut paths);
        paths
    }

    fn walkdir_recursive(dir: &Path, paths: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            paths.push(path.clone());
            if path.is_dir() {
                walkdir_recursive(&path, paths);
            }
        }
    }

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

    #[test]
    fn copy_path_entries_copies_directories() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // Create .claude directory with nested files
        let claude_dir = src.path().join(".claude");
        std::fs::create_dir_all(claude_dir.join("sub")).unwrap();
        std::fs::write(claude_dir.join("config.json"), r#"{"key":"val"}"#).unwrap();
        std::fs::write(claude_dir.join("sub").join("data.txt"), "nested").unwrap();

        let paths = vec![".claude".to_owned()];
        let copied = copy_path_entries(src.path(), dst.path(), &paths).unwrap();

        assert_eq!(copied, vec![".claude"]);
        assert!(dst.path().join(".claude").join("config.json").exists());
        assert!(dst.path().join(".claude").join("sub").join("data.txt").exists());

        let content =
            std::fs::read_to_string(dst.path().join(".claude").join("config.json")).unwrap();
        assert_eq!(content, r#"{"key":"val"}"#);
    }

    #[test]
    fn copy_path_entries_copies_individual_files() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        let vscode_dir = src.path().join(".vscode");
        std::fs::create_dir_all(&vscode_dir).unwrap();
        std::fs::write(vscode_dir.join("settings.json"), "{}").unwrap();

        let paths = vec![".vscode/settings.json".to_owned()];
        let copied = copy_path_entries(src.path(), dst.path(), &paths).unwrap();

        assert_eq!(copied, vec![".vscode/settings.json"]);
        assert!(dst.path().join(".vscode").join("settings.json").exists());
    }

    #[test]
    fn copy_path_entries_merges_directories() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // Source has .claude/settings.json and .claude/memory/a.md
        let src_claude = src.path().join(".claude");
        std::fs::create_dir_all(src_claude.join("memory")).unwrap();
        std::fs::write(src_claude.join("settings.json"), r#"{"src":true}"#).unwrap();
        std::fs::write(src_claude.join("memory").join("a.md"), "memory-a").unwrap();

        // Destination already has .claude/CLAUDE.md and .claude/memory/b.md
        let dst_claude = dst.path().join(".claude");
        std::fs::create_dir_all(dst_claude.join("memory")).unwrap();
        std::fs::write(dst_claude.join("CLAUDE.md"), "existing-claude").unwrap();
        std::fs::write(dst_claude.join("memory").join("b.md"), "memory-b").unwrap();

        let paths = vec![".claude".to_owned()];
        copy_path_entries(src.path(), dst.path(), &paths).unwrap();

        // Source files are copied
        assert!(dst_claude.join("settings.json").exists());
        assert!(dst_claude.join("memory").join("a.md").exists());

        // Destination-only files are preserved (merge, not replace)
        assert!(dst_claude.join("CLAUDE.md").exists());
        assert!(dst_claude.join("memory").join("b.md").exists());

        // Verify content integrity
        assert_eq!(
            std::fs::read_to_string(dst_claude.join("CLAUDE.md")).unwrap(),
            "existing-claude"
        );
        assert_eq!(
            std::fs::read_to_string(dst_claude.join("memory").join("a.md")).unwrap(),
            "memory-a"
        );
        assert_eq!(
            std::fs::read_to_string(dst_claude.join("memory").join("b.md")).unwrap(),
            "memory-b"
        );
    }

    #[test]
    fn copy_path_entries_overwrites_shared_files() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // Both have .claude/settings.json with different content
        let src_claude = src.path().join(".claude");
        std::fs::create_dir_all(&src_claude).unwrap();
        std::fs::write(src_claude.join("settings.json"), "from-source").unwrap();

        let dst_claude = dst.path().join(".claude");
        std::fs::create_dir_all(&dst_claude).unwrap();
        std::fs::write(dst_claude.join("settings.json"), "from-dest").unwrap();

        let paths = vec![".claude".to_owned()];
        copy_path_entries(src.path(), dst.path(), &paths).unwrap();

        // Source wins for shared files
        assert_eq!(
            std::fs::read_to_string(dst_claude.join("settings.json")).unwrap(),
            "from-source"
        );
    }

    #[test]
    fn copy_path_entries_merges_deeply_nested() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // Source: .claude/a/b/c/deep.txt
        let deep_src = src.path().join(".claude").join("a").join("b").join("c");
        std::fs::create_dir_all(&deep_src).unwrap();
        std::fs::write(deep_src.join("deep.txt"), "deep-source").unwrap();

        // Dest: .claude/a/b/existing.txt and .claude/x/other.txt
        let dst_ab = dst.path().join(".claude").join("a").join("b");
        std::fs::create_dir_all(&dst_ab).unwrap();
        std::fs::write(dst_ab.join("existing.txt"), "existing").unwrap();
        let dst_x = dst.path().join(".claude").join("x");
        std::fs::create_dir_all(&dst_x).unwrap();
        std::fs::write(dst_x.join("other.txt"), "other").unwrap();

        let paths = vec![".claude".to_owned()];
        copy_path_entries(src.path(), dst.path(), &paths).unwrap();

        // New deep file copied
        assert!(dst.path().join(".claude/a/b/c/deep.txt").exists());
        // Existing files preserved
        assert_eq!(
            std::fs::read_to_string(dst.path().join(".claude/a/b/existing.txt")).unwrap(),
            "existing"
        );
        assert_eq!(
            std::fs::read_to_string(dst.path().join(".claude/x/other.txt")).unwrap(),
            "other"
        );
    }

    #[test]
    fn copy_path_entries_no_nested_duplication() {
        // Verify .claude doesn't end up as .claude/.claude
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // Source: complex .claude structure
        let src_claude = src.path().join(".claude");
        std::fs::create_dir_all(src_claude.join("memory")).unwrap();
        std::fs::create_dir_all(src_claude.join("projects").join("myproj")).unwrap();
        std::fs::write(src_claude.join("settings.json"), "settings").unwrap();
        std::fs::write(src_claude.join("CLAUDE.md"), "claude-md").unwrap();
        std::fs::write(src_claude.join("memory").join("a.md"), "mem-a").unwrap();
        std::fs::write(
            src_claude.join("projects").join("myproj").join("MEMORY.md"),
            "proj-mem",
        )
        .unwrap();

        // Dest: already has .claude with some overlapping structure
        let dst_claude = dst.path().join(".claude");
        std::fs::create_dir_all(dst_claude.join("memory")).unwrap();
        std::fs::write(dst_claude.join("memory").join("b.md"), "mem-b").unwrap();

        let paths = vec![".claude".to_owned()];
        copy_path_entries(src.path(), dst.path(), &paths).unwrap();

        // CRITICAL: .claude/.claude must NOT exist — that would mean nesting bug
        assert!(
            !dst.path().join(".claude/.claude").exists(),
            "BUG: .claude is nested inside .claude"
        );

        // Correct flat structure under .claude
        assert!(dst_claude.join("settings.json").exists());
        assert!(dst_claude.join("CLAUDE.md").exists());
        assert!(dst_claude.join("memory/a.md").exists());
        assert!(dst_claude.join("memory/b.md").exists()); // dest-only preserved
        assert!(dst_claude.join("projects/myproj/MEMORY.md").exists());

        // Double-check no nesting at any level
        for entry in walkdir(dst.path()) {
            let rel = entry.strip_prefix(dst.path()).unwrap();
            let components: Vec<_> = rel.components().collect();
            // Count how many times ".claude" appears in the path
            let claude_count = components
                .iter()
                .filter(|c| c.as_os_str() == ".claude")
                .count();
            assert!(
                claude_count <= 1,
                "Nested .claude detected in path: {}",
                rel.display()
            );
        }
    }

    #[test]
    fn copy_path_entries_multiple_paths_merged() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // Source has .claude and .vscode
        let src_claude = src.path().join(".claude");
        std::fs::create_dir_all(&src_claude).unwrap();
        std::fs::write(src_claude.join("config.json"), "claude-cfg").unwrap();

        let src_vscode = src.path().join(".vscode");
        std::fs::create_dir_all(&src_vscode).unwrap();
        std::fs::write(src_vscode.join("settings.json"), "vscode-cfg").unwrap();

        // Dest already has .claude with different file
        let dst_claude = dst.path().join(".claude");
        std::fs::create_dir_all(&dst_claude).unwrap();
        std::fs::write(dst_claude.join("local.json"), "local-only").unwrap();

        let paths = vec![".claude".to_owned(), ".vscode".to_owned()];
        let copied = copy_path_entries(src.path(), dst.path(), &paths).unwrap();

        assert_eq!(copied.len(), 2);
        // .claude merged
        assert!(dst_claude.join("config.json").exists());
        assert!(dst_claude.join("local.json").exists());
        // .vscode copied
        assert!(dst.path().join(".vscode/settings.json").exists());
    }

    #[test]
    fn copy_path_entries_skips_missing() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        let paths = vec![".claude".to_owned(), ".nonexistent".to_owned()];
        let copied = copy_path_entries(src.path(), dst.path(), &paths).unwrap();

        assert!(copied.is_empty());
    }
}
