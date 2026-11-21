//! `git-remote-azlin`: git's remote helper for a drive index (feature `encryption`).
//!
//! ```text
//! AZLIN_DRIVE_KEY_FILE=drive.key git clone azlin::folder:///path/to/bucket my-index
//! git clone azlin::drive://d_1234 my-index
//! ```
//!
//! git runs it as `git-remote-azlin <remote> <url>` and talks to it on stdin and stdout
//! (`azul_storage::meta::git`).
//!
//! - `folder://<path>`: a bucket in a folder, reached here. The drive key comes from the file
//!   `AZLIN_DRIVE_KEY_FILE` names (64 hex digits), never from the command line, where other
//!   users of the computer could read it.
//! - `drive://<drive id>` (empty: the current drive): an Azlin drive, in an S3 bucket. The
//!   URL goes to `azcloud --drive <id> git-remote <remote> <url>` with git's input and output.
//!   The azcloud command line holds the account's credentials and the drive key, signs the
//!   S3 requests and sends them through its HTTPS transport. This binary links no HTTP client.

use std::{io, process::Command};

use azul_storage::{
    crypto::DriveKey,
    meta::{git, FolderBucket},
};

fn main() {
    if let Err(e) = run() {
        eprintln!("git-remote-azlin: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let remote = args.get(1).ok_or("usage: git-remote-azlin <remote> <url>")?;
    let url = args.get(2).unwrap_or(remote);
    if let Some(path) = url.strip_prefix("folder://") {
        return serve_folder(path);
    }
    if let Some(drive) = url.strip_prefix("drive://") {
        return hand_to_azcloud(remote, url, drive.trim_end_matches('/'));
    }
    Err(format!(
        "a bucket in a folder (folder://<path>) or an Azlin drive (drive://<drive id>), not {url}"
    ))
}

/// The helper over the bucket in the folder `path`, with the key from `AZLIN_DRIVE_KEY_FILE`.
fn serve_folder(path: &str) -> Result<(), String> {
    let key_file = std::env::var("AZLIN_DRIVE_KEY_FILE").map_err(|_| {
        String::from("AZLIN_DRIVE_KEY_FILE names no file with the drive key (64 hex digits)")
    })?;
    let text = std::fs::read_to_string(&key_file).map_err(|e| format!("{key_file}: {e}"))?;
    let key = git::key_from_hex(&text).ok_or("the drive key file holds no 64 hex digits")?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    git::serve_bucket(
        FolderBucket::new(path),
        DriveKey::from_bytes(key),
        stdin.lock(),
        stdout.lock(),
        &mut git::index_pack,
    )
    .map_err(|e| e.to_string())
}

/// An Azlin drive: `azcloud git-remote` serves it, on this process's input and output.
fn hand_to_azcloud(remote: &str, url: &str, drive: &str) -> Result<(), String> {
    let mut command = Command::new("azcloud");
    if !drive.is_empty() {
        command.args(["--drive", drive]);
    }
    let status = command
        .args(["git-remote", remote, url])
        .status()
        .map_err(|e| {
            format!(
                "an Azlin drive is reached through the azcloud command line (azcloud git-remote, \
                 examples/azcloud-api built with the feature `encryption`), which did not start: \
                 {e}"
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("azcloud git-remote: {status}"))
    }
}
