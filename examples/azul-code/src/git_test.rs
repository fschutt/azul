use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "azcode-git-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    dir
}

#[test]
fn head_names_a_branch_or_a_detached_commit() {
    assert_eq!(branch_of_head("ref: refs/heads/main\n").as_deref(), Some("main"));
    assert_eq!(
        branch_of_head("ref: refs/heads/fix/input-bugs\n").as_deref(),
        Some("fix/input-bugs")
    );
    assert_eq!(
        branch_of_head("86c0e821e0b3a5c1d2e3f4a5b6c7d8e9f0a1b2c3\n").as_deref(),
        Some("86c0e82")
    );
    assert_eq!(branch_of_head(""), None);
    assert_eq!(branch_of_head("not a head"), None);
}

#[test]
fn the_branch_is_read_from_the_folder_its_parents_and_a_worktrees_git_file() {
    let repo = temp_dir("repo");
    std::fs::create_dir_all(repo.join(".git")).expect(".git");
    std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    assert_eq!(branch(&repo).as_deref(), Some("main"));
    let inside = repo.join("src").join("deep");
    std::fs::create_dir_all(&inside).expect("a folder inside");
    assert_eq!(branch(&inside).as_deref(), Some("main"), "a folder inside the repository");

    let worktree = temp_dir("worktree");
    let gitdir = repo.join(".git").join("worktrees").join("wt");
    std::fs::create_dir_all(&gitdir).expect("the worktree's git dir");
    std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/feature\n").expect("HEAD");
    std::fs::write(worktree.join(".git"), format!("gitdir: {}\n", gitdir.display())).expect(".git file");
    assert_eq!(branch(&worktree).as_deref(), Some("feature"));

    let plain = temp_dir("plain");
    assert_eq!(branch(&plain), None, "no repository");
    for dir in [repo, worktree, plain] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
