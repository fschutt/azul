//! `git-remote-azlin` (C7): plain git reads the drive index. A git remote helper
//! (see `gitremote-helpers(7)`) over a device's copy of the repository: `git clone
//! azlin::folder:///path/to/bucket` gives the ordinary git repository, with the same commit
//! ids - the objects are byte for byte git's, in the `sha256` object format.
//!
//! The helper speaks the `fetch` flavour of the protocol:
//!
//! - `capabilities`: `fetch`, `option`, `object-format`;
//! - `option object-format true`: the `list` names the object format (`:object-format sha256`);
//!   other options are unsupported;
//! - `list`: polls the drive and answers `refs/heads/main` (the drive's history) and `HEAD`;
//! - `fetch <id> <ref>` (a batch, ended by a blank line): every object the wanted commits
//!   reach goes to git as one pack (version 2, no deltas, zlib in stored blocks - git inflates
//!   it like any other), which `git index-pack --stdin` takes into the repository.
//!
//! Without the drive key there is nothing to read: the helper needs it (the binary takes it
//! from a file, never from the command line). Pushing is not offered.

use std::io::{self, BufRead, Write};

use sha2::{Digest, Sha256};

use super::{
    bucket::Bucket,
    objects::{Kind, ObjectId, Objects},
    repo::{MetaRepo, MAIN},
    seal::Sealer,
    MetaError,
};

/// The capabilities the helper offers.
pub const CAPABILITIES: &str = "fetch\noption\nobject-format\n";

/// Adler-32 of `data` (zlib's checksum).
#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5_552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

/// `data` as a zlib stream of stored (uncompressed) deflate blocks: valid zlib that any
/// inflater reads, with no compressor needed.
#[must_use]
pub fn zlib_stored(data: &[u8]) -> Vec<u8> {
    // CMF 0x78 (deflate, 32 KiB window), FLG 0x01: (0x78 * 256 + 0x01) % 31 == 0.
    let mut out = Vec::with_capacity(data.len() + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    if data.is_empty() {
        // One final stored block of length 0.
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xff, 0xff]);
    }
    let mut blocks = data.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        let last = blocks.peek().is_none();
        out.push(u8::from(last));
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// The header of an object in a pack: its type (git's numbers) and its size, 4 bits then
/// 7 bits per byte, the high bit saying another byte follows.
fn pack_object_header(kind: Kind, size: usize, out: &mut Vec<u8>) {
    let mut rest = size >> 4;
    let mut byte = (kind.code() << 4) | (size & 0x0f) as u8;
    while rest > 0 {
        out.push(byte | 0x80);
        byte = (rest & 0x7f) as u8;
        rest >>= 7;
    }
    out.push(byte);
}

/// A git pack (version 2, no deltas) of `ids` from `objects`, with the SHA-256 trailer of a
/// repository in the `sha256` object format.
pub fn git_pack(objects: &Objects, ids: &[ObjectId]) -> Result<Vec<u8>, MetaError> {
    let mut out = Vec::new();
    out.extend_from_slice(b"PACK");
    out.extend_from_slice(&2u32.to_be_bytes());
    let count = u32::try_from(ids.len()).map_err(|_| MetaError::Corrupt {
        key: "pack".to_string(),
        reason: "more objects than a pack holds".to_string(),
    })?;
    out.extend_from_slice(&count.to_be_bytes());
    for id in ids {
        let (kind, body) = objects
            .get(id)
            .ok_or_else(|| MetaError::MissingObject { id: id.to_hex() })?;
        pack_object_header(kind, body.len(), &mut out);
        out.extend_from_slice(&zlib_stored(body));
    }
    let trailer = Sha256::digest(&out);
    out.extend_from_slice(&trailer);
    Ok(out)
}

/// Every object the commits `wants` reach: the commits, their parents, their trees and
/// blobs; each once.
pub fn reachable(objects: &Objects, wants: &[ObjectId]) -> Result<Vec<ObjectId>, MetaError> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut stack: Vec<ObjectId> = wants.to_vec();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let (kind, _) = objects
            .get(&id)
            .ok_or_else(|| MetaError::MissingObject { id: id.to_hex() })?;
        match kind {
            Kind::Commit => {
                let commit = objects.commit(&id)?;
                stack.push(commit.tree);
                stack.extend(commit.parents);
            }
            Kind::Tree => stack.extend(objects.tree(&id)?.entries().iter().map(|e| e.id)),
            Kind::Blob => {}
        }
        out.push(id);
    }
    Ok(out)
}

/// The 32 bytes of a drive key written as 64 hex digits (whitespace around is fine).
#[must_use]
pub fn key_from_hex(text: &str) -> Option<[u8; 32]> {
    super::from_hex(text.trim())?.try_into().ok()
}

fn protocol_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn io_error(e: MetaError) -> io::Error {
    io::Error::other(e.to_string())
}

/// Serves git's remote-helper commands from `input` to `output` over `repo` until git sends
/// a blank line or closes the input. `index_pack` takes each pack into git's repository (the
/// binary pipes it to `git index-pack --stdin`).
pub fn serve<B: Bucket, S: Sealer>(
    repo: &mut MetaRepo<B, S>,
    mut input: impl BufRead,
    mut output: impl Write,
    index_pack: &mut dyn FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let mut object_format = false;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let command = line.trim_end_matches(['\n', '\r']);
        if command.is_empty() {
            return Ok(());
        }
        if command == "capabilities" {
            // The list, then the blank line that ends it.
            writeln!(output, "{CAPABILITIES}")?;
        } else if let Some(option) = command.strip_prefix("option ") {
            if option == "object-format true" {
                object_format = true;
                writeln!(output, "ok")?;
            } else {
                writeln!(output, "unsupported")?;
            }
        } else if command == "list" || command == "list for-push" {
            repo.pull().map_err(io_error)?;
            if object_format {
                writeln!(output, ":object-format sha256")?;
            }
            if let Some(head) = repo.head() {
                writeln!(output, "{} {MAIN}", head.to_hex())?;
                writeln!(output, "@{MAIN} HEAD")?;
            }
            writeln!(output)?;
        } else if let Some(first) = command.strip_prefix("fetch ") {
            // A batch of fetch lines, ended by a blank line.
            let mut wants = Vec::new();
            let mut next = first.to_string();
            loop {
                let id = next
                    .split_whitespace()
                    .next()
                    .and_then(ObjectId::from_hex)
                    .ok_or_else(|| protocol_error(format!("not an object to fetch: {next}")))?;
                wants.push(id);
                line.clear();
                if input.read_line(&mut line)? == 0 {
                    break;
                }
                let more = line.trim_end_matches(['\n', '\r']);
                match more.strip_prefix("fetch ") {
                    Some(rest) => next = rest.to_string(),
                    None => break,
                }
            }
            repo.fetch_all().map_err(io_error)?;
            let ids = reachable(repo.objects(), &wants).map_err(io_error)?;
            let pack = git_pack(repo.objects(), &ids).map_err(io_error)?;
            index_pack(&pack)?;
            writeln!(output)?;
        } else {
            return Err(protocol_error(format!("a command the helper does not know: {command}")));
        }
        output.flush()?;
    }
}
