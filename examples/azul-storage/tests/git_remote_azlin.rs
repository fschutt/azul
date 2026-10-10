//! `git clone azlin::folder://...` with the real git and the real helper (feature
//! `encryption`; skipped where git cannot hold the `sha256` object format, before 2.29).

#![cfg(feature = "encryption")]

use std::{env, path::PathBuf, process::Command};

use azul_storage::{
    crypto::DriveKey,
    meta::{merge::keep_both, Change, FolderBucket, MetaRepo},
};

fn git_supports_sha256() -> bool {
    let Ok(out) = Command::new("git").arg("--version").output() else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let version = text.split_whitespace().nth(2).unwrap_or("");
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    major > 2 || (major == 2 && minor >= 29)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn git_clones_a_drive_index_with_the_same_commits() {
    if !git_supports_sha256() {
        eprintln!("skipped: no git that holds the sha256 object format");
        return;
    }
    let root = env::temp_dir().join(format!("azul-git-remote-azlin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let bucket = root.join("bucket");
    std::fs::create_dir_all(&bucket).unwrap();
    let key = [7u8; 32];
    let key_file = root.join("drive.key");
    std::fs::write(&key_file, hex(&key)).unwrap();

    let mut repo = MetaRepo::create(
        FolderBucket::new(&bucket),
        DriveKey::from_bytes(key),
        "laptop",
        "Laptop",
    )
    .unwrap();
    let pointer = b"azlin-pointer 1\nsize 4\n";
    let blob = repo.write_blob(pointer);
    repo.commit(
        &[Change::Put {
            path: "docs/a.txt".to_string(),
            id: blob,
        }],
        "first\n",
        &mut keep_both,
    )
    .unwrap();
    let blob = repo.write_blob(b"azlin-pointer 1\nsize 9\n");
    repo.commit(
        &[Change::Put {
            path: "b.txt".to_string(),
            id: blob,
        }],
        "second\n",
        &mut keep_both,
    )
    .unwrap();
    let head = repo.head().unwrap().to_hex();

    let helper = PathBuf::from(env!("CARGO_BIN_EXE_git-remote-azlin"));
    let mut paths = vec![helper.parent().unwrap().to_path_buf()];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let path = env::join_paths(paths).unwrap();
    let clone = root.join("clone");
    let status = Command::new("git")
        .arg("clone")
        .arg(format!("azlin::folder://{}", bucket.display()))
        .arg(&clone)
        .env("PATH", &path)
        .env("AZLIN_DRIVE_KEY_FILE", &key_file)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .unwrap();
    assert!(status.success(), "git clone: {status}");

    let log = Command::new("git")
        .arg("-C")
        .arg(&clone)
        .args(["log", "--format=%H"])
        .output()
        .unwrap();
    let commits: Vec<String> = String::from_utf8_lossy(&log.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0], head, "the same commit ids");
    let file = Command::new("git")
        .arg("-C")
        .arg(&clone)
        .args(["show", "HEAD:docs/a.txt"])
        .output()
        .unwrap();
    assert_eq!(file.stdout, pointer);
    let _ = std::fs::remove_dir_all(&root);
}

/// An Azlin drive (`azlin::drive://<drive id>`) lives in an S3 bucket that the azcloud
/// command line reaches with the account's credentials and its HTTPS transport. The helper
/// hands such a URL to `azcloud git-remote`, with git's input and output, and says so when
/// there is no azcloud to hand it to.
#[cfg(unix)]
#[test]
fn an_azlin_drive_url_is_handed_to_azcloud_with_gits_input_and_output() {
    use std::{
        io::Write,
        os::unix::fs::PermissionsExt,
        process::Stdio,
    };

    let root = env::temp_dir().join(format!("azul-git-remote-azcloud-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // A stand-in azcloud: its arguments, then what git wrote (shell builtins only).
    let script = root.join("azcloud");
    std::fs::write(
        &script,
        "#!/bin/sh\necho \"$@\"\nwhile IFS= read -r line; do printf '%s\\n' \"$line\"; done\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let helper = PathBuf::from(env!("CARGO_BIN_EXE_git-remote-azlin"));

    let mut child = Command::new(&helper)
        .args(["origin", "drive://d_1"])
        .env("PATH", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"capabilities\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "--drive d_1 git-remote origin drive://d_1\ncapabilities\n"
    );

    std::fs::remove_file(&script).unwrap();
    let out = Command::new(&helper)
        .args(["origin", "drive://d_1"])
        .env("PATH", &root)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("azcloud git-remote"), "{text}");
    let _ = std::fs::remove_dir_all(&root);
}
