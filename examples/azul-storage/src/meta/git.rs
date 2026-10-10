//! `git-remote-azlin` (C7): plain git reads the drive index. Not written yet.

use std::io::{self, BufRead, Write};

use super::{bucket::Bucket, objects::ObjectId, objects::Objects, repo::MetaRepo, seal::Sealer, MetaError};

/// The capabilities the helper offers.
pub const CAPABILITIES: &str = "";

#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    let _ = data;
    0
}

#[must_use]
pub fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let _ = data;
    Vec::new()
}

pub fn git_pack(objects: &Objects, ids: &[ObjectId]) -> Result<Vec<u8>, MetaError> {
    let _ = (objects, ids);
    Ok(Vec::new())
}

pub fn reachable(objects: &Objects, wants: &[ObjectId]) -> Result<Vec<ObjectId>, MetaError> {
    let _ = (objects, wants);
    Ok(Vec::new())
}

#[must_use]
pub fn key_from_hex(text: &str) -> Option<[u8; 32]> {
    let _ = text;
    None
}

pub fn serve<B: Bucket, S: Sealer>(
    repo: &mut MetaRepo<B, S>,
    input: impl BufRead,
    output: impl Write,
    index_pack: &mut dyn FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let _ = (repo, input, output, index_pack);
    Ok(())
}
