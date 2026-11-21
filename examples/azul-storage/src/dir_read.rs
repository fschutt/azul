//! One folder on disk, read fast: the names and kinds of its entries and, where the platform
//! hands them over with the names, their sizes, dates and inode numbers, in batches.
//!
//! - macOS: `getattrlistbulk` fills a 64 KiB buffer with a few hundred entries and the asked
//!   attributes (name, kind, date, inode, size) in one system call, so a folder of N files costs
//!   about N/300 calls and no stat at all.
//! - Linux and the other Unixes: `read_dir` (getdents64, a 32 KiB chunk per call) gives the
//!   name, the kind (`d_type`) and the inode number without a stat. Sizes and dates stay
//!   unknown here; whoever needs them stats only those entries (a view its rows in view).
//! - Windows: `FindNextFileW` returns the size and the date with each name, so they are known.
//!
//! A symbolic link is followed to what it points at (one stat for each link), as a listing of
//! one folder does; a broken one, a name that is not UTF-8 and anything that is neither a file
//! nor a folder (a socket, a device) are left out.

use std::{fs, io, ops::ControlFlow, path::Path, time::UNIX_EPOCH};

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Folder,
    File,
}

/// One entry of a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirItem {
    /// The entry's name (UTF-8: no key could name another).
    pub name: String,
    pub kind: ItemKind,
    /// A file's bytes, when the read had them; `None` for a folder, and for a file whose size
    /// the platform's directory read does not carry.
    pub size: Option<u64>,
    /// Last modified, in seconds since 1970, when the read had it.
    pub modified: Option<u64>,
    /// The inode number (the file ID on macOS), when the read had it.
    pub inode: Option<u64>,
    /// It is a symbolic link (`kind`, `size` and `modified` are what it points at): a walk of
    /// every folder below does not follow a link to a folder, or it could loop.
    pub link: bool,
}

impl DirItem {
    /// Whether the read had what a listing row shows: a file's size and date, a folder's date.
    #[must_use]
    pub fn has_stat(&self) -> bool {
        self.modified.is_some() && (self.kind == ItemKind::Folder || self.size.is_some())
    }
}

/// How many entries the platforms without a batched read hand over at once.
const PORTABLE_BATCH: usize = 512;

/// Reads the folder `dir`, handing its entries to `each_batch` as they are read (on macOS one
/// batch per system call); `ControlFlow::Break` stops the read.
pub fn read_dir_items(
    dir: &Path,
    each_batch: &mut dyn FnMut(Vec<DirItem>) -> ControlFlow<()>,
) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        bulk::read(dir, each_batch)
    }
    #[cfg(not(target_os = "macos"))]
    {
        read_portable(dir, each_batch)
    }
}

/// Every entry of the folder `dir` (see [`read_dir_items`]).
pub fn read_dir_all(dir: &Path) -> io::Result<Vec<DirItem>> {
    let mut all = Vec::new();
    read_dir_items(dir, &mut |batch| {
        all.extend(batch);
        ControlFlow::Continue(())
    })?;
    Ok(all)
}

/// The system calls this thread's folder reads made: the test hook that keeps the cost of a
/// listing in check (a listing of N files is a few batch reads, not N stats).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadCalls {
    /// `getattrlistbulk` calls (macOS; 0 elsewhere, where `read_dir` batches inside std).
    pub batch_reads: usize,
    /// Stats of single entries (a symbolic link followed, an entry the batch could not
    /// describe, [`stat_item`]).
    pub stats: usize,
}

thread_local! {
    static CALLS: std::cell::Cell<ReadCalls> = const {
        std::cell::Cell::new(ReadCalls { batch_reads: 0, stats: 0 })
    };
}

/// What this thread's folder reads cost so far.
#[must_use]
pub fn calls_on_this_thread() -> ReadCalls {
    CALLS.with(std::cell::Cell::get)
}

fn count(change: impl FnOnce(&mut ReadCalls)) {
    CALLS.with(|calls| {
        let mut now = calls.get();
        change(&mut now);
        calls.set(now);
    });
}

/// The kind, size and date of the entry at `path`, following a symbolic link (one stat,
/// counted); `None` for a broken link and for what is neither a file nor a folder.
pub fn stat_item(path: &Path) -> Option<(ItemKind, Option<u64>, Option<u64>)> {
    count(|c| c.stats += 1);
    let meta = fs::metadata(path).ok()?;
    let kind = if meta.is_dir() {
        ItemKind::Folder
    } else if meta.is_file() {
        ItemKind::File
    } else {
        return None;
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    Some((kind, (kind == ItemKind::File).then(|| meta.len()), modified))
}

/// `read_dir` with the kinds it reports: what every platform but macOS uses (and macOS's tests,
/// to compare).
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn read_portable(
    dir: &Path,
    each_batch: &mut dyn FnMut(Vec<DirItem>) -> ControlFlow<()>,
) -> io::Result<()> {
    let mut batch = Vec::with_capacity(PORTABLE_BATCH);
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        #[cfg(unix)]
        let inode = Some(std::os::unix::fs::DirEntryExt::ino(&entry));
        #[cfg(not(unix))]
        let inode = None;
        let item = if file_type.is_symlink() {
            let Some((kind, size, modified)) = stat_item(&entry.path()) else {
                continue;
            };
            DirItem {
                name,
                kind,
                size,
                modified,
                inode,
                link: true,
            }
        } else {
            let kind = if file_type.is_dir() {
                ItemKind::Folder
            } else if file_type.is_file() {
                ItemKind::File
            } else {
                continue;
            };
            // Windows' directory read carries the size and the date: `metadata` reads them
            // from it, no further call. Elsewhere it would be a stat per entry.
            #[cfg(windows)]
            let (size, modified) = match entry.metadata() {
                Ok(meta) => (
                    (kind == ItemKind::File).then(|| meta.len()),
                    meta.modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                ),
                Err(_) => (None, None),
            };
            #[cfg(not(windows))]
            let (size, modified) = (None, None);
            DirItem {
                name,
                kind,
                size,
                modified,
                inode,
                link: false,
            }
        };
        batch.push(item);
        if batch.len() >= PORTABLE_BATCH && each_batch(std::mem::take(&mut batch)).is_break() {
            return Ok(());
        }
    }
    if !batch.is_empty() {
        let _ = each_batch(batch);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod bulk {
    //! `getattrlistbulk(2)`: each entry in the buffer is its length, the set of attributes it
    //! has, then those attributes in a fixed order (the order of their bits, the error first),
    //! each on a 4-byte boundary. Only the attributes the returned set names are packed.

    use std::{fs::File, io, ops::ControlFlow, os::fd::AsRawFd, path::Path};

    use super::{count, stat_item, DirItem, ItemKind};

    /// The entry's error, packed right after the returned set (sys/attr.h; not in `libc`).
    const ATTR_CMN_ERROR: u32 = 0x2000_0000;
    /// `fsobj_type_t` values (sys/vnode.h).
    const VREG: u32 = 1;
    const VDIR: u32 = 2;
    const VLNK: u32 = 5;
    /// The buffer one call fills: a few hundred entries.
    const BUFFER_BYTES: usize = 64 * 1024;

    pub(super) fn read(
        dir: &Path,
        each_batch: &mut dyn FnMut(Vec<DirItem>) -> ControlFlow<()>,
    ) -> io::Result<()> {
        let folder = File::open(dir)?;
        let mut asked = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: libc::ATTR_CMN_RETURNED_ATTRS
                | ATTR_CMN_ERROR
                | libc::ATTR_CMN_NAME
                | libc::ATTR_CMN_OBJTYPE
                | libc::ATTR_CMN_MODTIME
                | libc::ATTR_CMN_FILEID,
            volattr: 0,
            dirattr: 0,
            fileattr: libc::ATTR_FILE_DATALENGTH,
            forkattr: 0,
        };
        // u32 words: the kernel packs on 4-byte boundaries from the buffer's start.
        let mut words = vec![0u32; BUFFER_BYTES / 4];
        loop {
            count(|c| c.batch_reads += 1);
            // SAFETY: `folder` is an open directory for the whole call, `asked` a valid
            // attrlist, and the buffer is `BUFFER_BYTES` long and writable.
            let entries = unsafe {
                libc::getattrlistbulk(
                    folder.as_raw_fd(),
                    std::ptr::addr_of_mut!(asked).cast(),
                    words.as_mut_ptr().cast(),
                    BUFFER_BYTES,
                    0,
                )
            };
            if entries < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if entries == 0 {
                return Ok(());
            }
            // SAFETY: a view of the same initialised words as bytes (u8 has no alignment),
            // dropped before the next call writes them again.
            let bytes: &[u8] =
                unsafe { std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), BUFFER_BYTES) };
            let batch = parse(bytes, entries.unsigned_abs() as usize, dir);
            if each_batch(batch).is_break() {
                return Ok(());
            }
        }
    }

    fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_ne_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    }

    fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
        Some(u64::from_ne_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
    }

    /// The `entries` entries at the start of `bytes`. An entry the buffer cannot describe (its
    /// error set, a link) is stat'ed by name; one without a name ends the batch (the buffer is
    /// not what the kernel documents, and nothing after it can be trusted).
    pub(super) fn parse(bytes: &[u8], entries: usize, dir: &Path) -> Vec<DirItem> {
        let mut items = Vec::with_capacity(entries);
        let mut at = 0usize;
        for _ in 0..entries {
            let Some(length) = u32_at(bytes, at).map(|l| l as usize) else {
                break;
            };
            let end = at + length;
            if length < 24 || end > bytes.len() {
                break;
            }
            let entry = &bytes[..end];
            match parse_entry(entry, at, dir) {
                Parsed::Item(item) => items.push(item),
                Parsed::Skip => {}
                Parsed::Broken => break,
            }
            at = end;
        }
        items
    }

    enum Parsed {
        Item(DirItem),
        /// Not listed (not UTF-8, a socket, a broken link).
        Skip,
        Broken,
    }

    /// The entry starting at `start` of `entry` (which ends where the entry ends).
    fn parse_entry(entry: &[u8], start: usize, dir: &Path) -> Parsed {
        let (Some(common), Some(file)) = (u32_at(entry, start + 4), u32_at(entry, start + 16))
        else {
            return Parsed::Broken;
        };
        let mut at = start + 4 + 20;
        let mut error = 0;
        if common & ATTR_CMN_ERROR != 0 {
            let Some(e) = u32_at(entry, at) else {
                return Parsed::Broken;
            };
            error = e;
            at += 4;
        }
        if common & libc::ATTR_CMN_NAME == 0 {
            return Parsed::Broken;
        }
        let (Some(offset), Some(length)) = (u32_at(entry, at), u32_at(entry, at + 4)) else {
            return Parsed::Broken;
        };
        // The offset is from the reference itself; the length counts the closing NUL.
        let from = at.wrapping_add_signed(offset as i32 as isize);
        let Some(raw) = entry.get(from..from + (length as usize).saturating_sub(1)) else {
            return Parsed::Broken;
        };
        at += 8;
        let Ok(name) = std::str::from_utf8(raw) else {
            return Parsed::Skip;
        };
        let name = name.to_string();
        let mut kind = None;
        if common & libc::ATTR_CMN_OBJTYPE != 0 {
            kind = u32_at(entry, at);
            at += 4;
        }
        let mut modified = None;
        if common & libc::ATTR_CMN_MODTIME != 0 {
            // A 64-bit `struct timespec`: seconds, then nanoseconds.
            modified = u64_at(entry, at).filter(|s| (*s as i64) >= 0);
            at += 16;
        }
        let mut inode = None;
        if common & libc::ATTR_CMN_FILEID != 0 {
            inode = u64_at(entry, at);
            at += 8;
        }
        let mut size = None;
        if file & libc::ATTR_FILE_DATALENGTH != 0 {
            size = u64_at(entry, at);
        }
        let kind = match (error, kind) {
            (0, Some(VDIR)) => ItemKind::Folder,
            (0, Some(VREG)) => ItemKind::File,
            (0, Some(VLNK)) | (1.., _) | (_, None) => {
                // A link points elsewhere, an entry with an error says nothing: ask the file.
                let path = dir.join(&name);
                let link = kind == Some(VLNK)
                    || (kind.is_none()
                        && std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()));
                let Some((kind, size, modified)) = stat_item(&path) else {
                    return Parsed::Skip;
                };
                return Parsed::Item(DirItem {
                    name,
                    kind,
                    size,
                    modified,
                    inode,
                    link,
                });
            }
            (0, Some(_)) => return Parsed::Skip,
        };
        Parsed::Item(DirItem {
            name,
            size: if kind == ItemKind::File { size } else { None },
            kind,
            modified,
            inode,
            link: false,
        })
    }
}
