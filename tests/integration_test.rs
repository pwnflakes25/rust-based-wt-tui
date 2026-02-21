mod common;

use assert_cmd::Command;
use predicates::prelude::*;

use common::TestRepo;

#[test]
fn list_shows_main_worktree() {
    let repo = TestRepo::new();

    Command::new(TestRepo::wt_bin())
        .args(["list"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("NAME"))
        .stdout(predicate::str::contains("PATH"))
        .stdout(predicate::str::contains("HEAD"));
}

#[test]
fn status_shows_current_worktree() {
    let repo = TestRepo::new();

    Command::new(TestRepo::wt_bin())
        .args(["status"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Worktree:"))
        .stdout(predicate::str::contains("Status:"));
}

#[test]
fn new_creates_worktree() {
    let repo = TestRepo::new();

    Command::new(TestRepo::wt_bin())
        .args(["new", "test-branch", "--base", "HEAD"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Verify worktree directory was created
    assert!(repo.path().join(".worktrees").join("test-branch").exists());

    // Verify .gitignore was updated
    let gitignore = std::fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains(".worktrees/"));
}

#[test]
fn new_then_list_shows_worktree() {
    let repo = TestRepo::new();

    // Create a worktree
    Command::new(TestRepo::wt_bin())
        .args(["new", "feat-list", "--base", "HEAD"])
        .current_dir(repo.path())
        .assert()
        .success();

    // List should show it
    Command::new(TestRepo::wt_bin())
        .args(["list"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("feat-list"));
}

#[test]
fn switch_prints_path_to_stdout() {
    let repo = TestRepo::new();

    // Create a worktree
    Command::new(TestRepo::wt_bin())
        .args(["new", "switch-test", "--base", "HEAD"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Switch should print the path
    Command::new(TestRepo::wt_bin())
        .args(["switch", "switch-test"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(".worktrees/switch-test"));
}

#[test]
fn remove_deletes_worktree() {
    let repo = TestRepo::new();

    // Create then remove
    Command::new(TestRepo::wt_bin())
        .args(["new", "to-remove", "--base", "HEAD"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert!(repo.path().join(".worktrees").join("to-remove").exists());

    Command::new(TestRepo::wt_bin())
        .args(["remove", "to-remove"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert!(!repo.path().join(".worktrees").join("to-remove").exists());
}

#[test]
fn init_prints_shell_snippet() {
    let repo = TestRepo::new();

    Command::new(TestRepo::wt_bin())
        .args(["init"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("wts()"))
        .stdout(predicate::str::contains("wt switch"));
}

#[test]
fn env_copies_files() {
    let repo = TestRepo::new();

    // Create .env in main worktree
    std::fs::write(repo.path().join(".env"), "SECRET=hello").unwrap();
    std::fs::write(repo.path().join(".env.local"), "LOCAL=world").unwrap();

    // Create a target worktree
    Command::new(TestRepo::wt_bin())
        .args(["new", "env-target", "--base", "HEAD"])
        .current_dir(repo.path())
        .assert()
        .success();

    // The auto_copy_env should have copied them already (default config)
    let target_dir = repo.path().join(".worktrees").join("env-target");
    assert!(target_dir.join(".env").exists());

    let content = std::fs::read_to_string(target_dir.join(".env")).unwrap();
    assert_eq!(content, "SECRET=hello");
}

#[test]
fn outside_git_repo_shows_error() {
    let dir = tempfile::tempdir().unwrap();

    Command::new(TestRepo::wt_bin())
        .args(["list"])
        .current_dir(dir.path())
        .assert()
        .failure();
}

#[test]
fn duplicate_worktree_errors() {
    let repo = TestRepo::new();

    // Create first
    Command::new(TestRepo::wt_bin())
        .args(["new", "dup-test", "--base", "HEAD"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Create duplicate should fail
    Command::new(TestRepo::wt_bin())
        .args(["new", "dup-test", "--base", "HEAD"])
        .current_dir(repo.path())
        .assert()
        .failure();
}

#[test]
fn merge_into_current_branch() {
    let repo = TestRepo::new();

    // Create a worktree with a new branch
    Command::new(TestRepo::wt_bin())
        .args(["new", "merge-src", "--base", "HEAD"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Commit the .gitignore change that `wt new` creates in the main worktree
    std::process::Command::new("git")
        .args(["add", ".gitignore"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "Add .gitignore"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    // Make a commit in the new worktree
    let wt_path = repo.path().join(".worktrees").join("merge-src");
    std::fs::write(wt_path.join("new-file.txt"), "new content").unwrap();

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&wt_path)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["commit", "-m", "Add new file"])
        .current_dir(&wt_path)
        .output()
        .unwrap();

    // Merge from main worktree
    Command::new(TestRepo::wt_bin())
        .args(["merge", "merge-src", "--no-delete"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Verify the file exists in main
    assert!(repo.path().join("new-file.txt").exists());
}
