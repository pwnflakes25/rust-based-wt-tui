use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Set up a temporary git repository for testing.
pub struct TestRepo {
    pub dir: TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let path = dir.path();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("Failed to init git repo");

        // Configure git user for commits
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .expect("Failed to config email");

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .expect("Failed to config name");

        // Create initial commit
        std::fs::write(path.join("README.md"), "# Test\n").expect("Failed to write README");

        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(path)
            .output()
            .expect("Failed to git commit");

        Self { dir }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    /// Get the path to the wt binary.
    #[allow(deprecated)]
    pub fn wt_bin() -> PathBuf {
        assert_cmd::cargo::cargo_bin("wt")
    }
}
