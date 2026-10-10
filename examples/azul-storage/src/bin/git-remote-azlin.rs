//! `git-remote-azlin`: git's remote helper for a drive index (feature `encryption`).
//!
//! ```text
//! AZLIN_DRIVE_KEY_FILE=drive.key git clone azlin::folder:///path/to/bucket my-index
//! ```
//!
//! git runs it as `git-remote-azlin <remote> <url>` and talks to it on stdin and stdout
//! (`azul_storage::meta::git`). The drive key comes from the file `AZLIN_DRIVE_KEY_FILE` names
//! (64 hex digits), never from the command line, where other users of the computer could read
//! it. A bucket in a folder is reached directly; an S3 bucket needs an HTTP transport, which
//! the apps bring (their own wiring).

use std::{
    io::{self, Write},
    process::{Command, Stdio},
};

use azul_storage::{
    crypto::DriveKey,
    meta::{git, FolderBucket, MetaRepo},
};

fn main() {
    if let Err(e) = run() {
        eprintln!("git-remote-azlin: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let url = args
        .get(2)
        .or_else(|| args.get(1))
        .ok_or("usage: git-remote-azlin <remote> <url>")?;
    let path = url
        .strip_prefix("folder://")
        .ok_or_else(|| format!("only a bucket in a folder (folder://<path>) is reached here, not {url}"))?;
    let key_file = std::env::var("AZLIN_DRIVE_KEY_FILE").map_err(|_| {
        String::from("AZLIN_DRIVE_KEY_FILE names no file with the drive key (64 hex digits)")
    })?;
    let text = std::fs::read_to_string(&key_file).map_err(|e| format!("{key_file}: {e}"))?;
    let key = git::key_from_hex(&text).ok_or("the drive key file holds no 64 hex digits")?;
    let mut repo = MetaRepo::open(
        FolderBucket::new(path),
        DriveKey::from_bytes(key),
        "git-remote-azlin",
        "git",
    )
    .map_err(|e| e.to_string())?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    git::serve(&mut repo, stdin.lock(), stdout.lock(), &mut index_pack).map_err(|e| e.to_string())
}

/// Takes a pack into the repository git runs the helper for (`GIT_DIR`): `git index-pack
/// --stdin`, its own output kept off the protocol.
fn index_pack(pack: &[u8]) -> io::Result<()> {
    let mut child = Command::new("git")
        .args(["index-pack", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    {
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("git index-pack took no input"))?;
        input.write_all(pack)?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("git index-pack: {status}")))
    }
}
