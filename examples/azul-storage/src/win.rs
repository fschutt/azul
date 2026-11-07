//! What this crate asks of Windows that std does not tell (ARCH28 RF49 / AR5a-9).
//!
//! std gives a file's size, times and attributes on Windows. It gives no file identity (the volume
//! serial number and the file index `GetFileInformationByHandle` returns), no change time, no
//! reparse tag, no free space, no name of the file system, and its rename cannot be told to write
//! through. Without them a Windows file had `ino = dev = ctime = 0` - "no identity" (AZ-W6) -, the
//! disk-reserve guard was off because the free space of a disk was "unknown" and unknown was read
//! as endless (AZ-W3), a FAT32 stick took 4 GiB and then failed in the middle of a file, and every
//! reparse point was a link (AZ-W15).
//!
//! The module has two halves.
//!
//! - **The model** has no system call and is built, and tested, on every host - the policy tests
//!   run with Windows' numbers on a Mac and on Linux: FILETIME arithmetic, what a reparse point makes
//!   of an entry, how the numbers of a handle become a [`Stat`], which file systems cap a file, which
//!   errors say "another program has it".
//! - **The calls** are `cfg(windows)` and thin: each fills an [`Info`] (or answers one number) and
//!   hands it to the model. Nothing here runs without a Windows machine; the CI job
//!   `windows_storage_check` builds it, and `cargo test -p azul-storage --features encryption win`
//!   on a Windows machine runs its tests (`windows_` in their names).
//!
//! A policy that differs by system takes a [`crate::key::System`] as a value - `System::here()` is the
//! one place that asks which system this is - so it is tested for Windows on every host; the
//! dispatch to this file is by `cfg`.

#![cfg_attr(not(windows), allow(dead_code))]

use std::io;

use crate::beneath::{Kind, Stat};

// ---- The model: nothing here asks the system anything ----

/// `FILE_ATTRIBUTE_READONLY`.
pub(crate) const ATTR_READONLY: u32 = 0x0000_0001;
/// `FILE_ATTRIBUTE_DIRECTORY`.
pub(crate) const ATTR_DIRECTORY: u32 = 0x0000_0010;
/// `FILE_ATTRIBUTE_REPARSE_POINT`: a link, a mount point, or a file a filter driver serves.
pub(crate) const ATTR_REPARSE_POINT: u32 = 0x0000_0400;
/// `FILE_ATTRIBUTE_OFFLINE`: the bytes are on slow storage; reading them recalls them.
pub(crate) const ATTR_OFFLINE: u32 = 0x0000_1000;
/// `FILE_ATTRIBUTE_RECALL_ON_OPEN` (a cloud placeholder: opening it downloads it).
pub(crate) const ATTR_RECALL_ON_OPEN: u32 = 0x0004_0000;
/// `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (a cloud placeholder: reading it downloads it).
pub(crate) const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// A tag with this bit set names another file system object - a symbolic link, a junction, a
/// volume mount point, a WSL link - whose target a path walks into.
const TAG_NAME_SURROGATE: u32 = 0x2000_0000;
/// `IO_REPARSE_TAG_WOF`: a file compressed by the Windows Overlay Filter (CompactOS); it reads as
/// itself.
const TAG_WOF: u32 = 0x8000_0017;
/// `IO_REPARSE_TAG_DEDUP`: a file the data deduplication service stores in chunks; it reads as itself.
const TAG_DEDUP: u32 = 0x8000_0013;
/// `IO_REPARSE_TAG_ONEDRIVE`: OneDrive's older placeholders.
const TAG_ONEDRIVE: u32 = 0x8000_0021;
/// `IO_REPARSE_TAG_CLOUD` and its sixteen variants (`CLOUD_1` .. `CLOUD_F`, which differ in the
/// bits of [`TAG_CLOUD_MASK`]): the Cloud Files API's placeholders (OneDrive Files On-Demand).
const TAG_CLOUD: u32 = 0x9000_001A;
const TAG_CLOUD_MASK: u32 = 0x0000_F000;

/// 100-nanosecond ticks from 1601-01-01 (a FILETIME's zero) to 1970-01-01.
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

/// A FILETIME - 100 ns ticks since 1601 - as nanoseconds since 1970. 0 for 1970 itself and before
/// it: the value every [`Stat`] time has "where unknown" (the Unix side clamps the same way).
#[must_use]
pub(crate) fn filetime_ns(ticks: i64) -> u64 {
    if ticks <= UNIX_EPOCH_TICKS {
        return 0;
    }
    ((ticks - UNIX_EPOCH_TICKS) as u64).saturating_mul(100)
}

/// The 64-bit file index a handle's information gives, from its two halves.
#[must_use]
pub(crate) fn file_index(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

/// Whether a reparse point with this tag reads as the file or folder itself: the placeholders of a
/// sync provider, a compressed file, a deduplicated one. Every other tag - above all those that
/// name another object - is a link: nothing is read or walked through it.
#[must_use]
pub(crate) fn is_transparent_tag(tag: u32) -> bool {
    if tag & TAG_NAME_SURROGATE != 0 {
        return false;
    }
    matches!(tag, TAG_WOF | TAG_DEDUP | TAG_ONEDRIVE) || tag & !TAG_CLOUD_MASK == TAG_CLOUD
}

/// What an entry with these attributes and this reparse tag (0 when it is none or the tag could not
/// be read) is for a sync: a folder, a file, or a [`Kind::Link`] - something that is not read, not
/// walked into and not taken for deleted.
///
/// A file whose bytes are not on this computer - a cloud placeholder that reading would download,
/// an offline file that reading would recall - is a link too: hashing every placeholder of a
/// OneDrive folder would download all of it. A folder placeholder is a folder (listing it asks for
/// names, not for bytes).
#[must_use]
pub(crate) fn kind_of(attributes: u32, reparse_tag: u32) -> Kind {
    let folder = attributes & ATTR_DIRECTORY != 0;
    if attributes & ATTR_REPARSE_POINT != 0 {
        if !is_transparent_tag(reparse_tag) {
            return Kind::Link;
        }
        if !folder
            && attributes & (ATTR_RECALL_ON_DATA_ACCESS | ATTR_RECALL_ON_OPEN | ATTR_OFFLINE) != 0
        {
            return Kind::Link;
        }
    } else if !folder && attributes & ATTR_OFFLINE != 0 {
        return Kind::Link;
    }
    if folder {
        Kind::Dir
    } else {
        Kind::File
    }
}

/// What the system said of one file or folder, as plain numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Info {
    pub attributes: u32,
    /// The reparse tag; 0 when the entry is no reparse point or the tag could not be read.
    pub reparse_tag: u32,
    pub size: u64,
    /// The last write, as a FILETIME.
    pub written: i64,
    /// The change time (any change of data or metadata), as a FILETIME; 0 when the system did not
    /// give one - then it is not compared.
    pub changed: i64,
    /// The volume's serial number.
    pub volume: u64,
    /// The file index within the volume; 0 when there is none.
    pub index: u64,
    /// The file system's file id is wider than the 64-bit index (ReFS): the index alone does not
    /// name one file, so the file has no identity here.
    pub wide_id: bool,
    pub links: u32,
}

impl Info {
    /// What the entry is ([`kind_of`]).
    #[must_use]
    pub(crate) fn kind(&self) -> Kind {
        kind_of(self.attributes, self.reparse_tag)
    }

    /// The same as a [`Stat`]: the volume serial number and the file index are `dev` and `ino` -
    /// both 0 where the file has no identity (an index of 0, or an id wider than the index).
    #[must_use]
    pub(crate) fn stat(&self) -> Stat {
        let known = self.index != 0 && !self.wide_id;
        Stat {
            kind: self.kind(),
            size: self.size,
            mtime_ns: filetime_ns(self.written),
            ctime_ns: filetime_ns(self.changed),
            ino: if known { self.index } else { 0 },
            dev: if known { self.volume } else { 0 },
            mode: if self.attributes & ATTR_READONLY != 0 {
                0o444
            } else {
                0o644
            },
            nlink: u64::from(self.links),
        }
    }
}

/// The largest file FAT and FAT32 hold: 4 GiB less one byte. The one number: the macOS and Linux
/// probes of [`crate::beneath::file_size_limit`] answer it too.
pub(crate) const FAT_FILE_MAX: u64 = (1 << 32) - 1;

/// The largest file the file system called `name` (as `GetVolumeInformation` names it: `NTFS`,
/// `FAT32`, `exFAT`, `ReFS`) holds, when that is below what a sync meets; `None` for the others.
#[must_use]
pub(crate) fn fat_cap(name: &str) -> Option<u64> {
    let name = name.trim();
    (name.eq_ignore_ascii_case("FAT32") || name.eq_ignore_ascii_case("FAT")).then_some(FAT_FILE_MAX)
}

/// `ERROR_SHARING_VIOLATION`: another program has the file open and does not share it.
pub(crate) const ERROR_SHARING_VIOLATION: i32 = 32;
/// `ERROR_LOCK_VIOLATION`: another program has locked the part of the file asked for.
pub(crate) const ERROR_LOCK_VIOLATION: i32 = 33;

/// Whether the Windows error number `code` says another program has the file (open without sharing
/// it, or a range of it locked): the file is not broken, it is busy.
#[must_use]
pub(crate) fn is_held_open_code(code: i32) -> bool {
    matches!(code, ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
}

/// Whether `e` says another program holds the file ([`is_held_open_code`]). On other systems no
/// error says it: their numbers 32 and 33 are other errors.
#[must_use]
pub(crate) fn is_held_open(e: &io::Error) -> bool {
    #[cfg(windows)]
    {
        e.raw_os_error().is_some_and(is_held_open_code)
    }
    #[cfg(not(windows))]
    {
        let _ = e;
        false
    }
}

/// The waits, in milliseconds, between the tries of a rename another program holds up (a virus
/// scanner or an indexer has the new file open for a moment): five tries after the first, a second
/// at most.
pub(crate) const HELD_BACKOFF_MS: [u64; 5] = [20, 50, 120, 300, 600];

// ---- The calls: Windows only ----

#[cfg(windows)]
pub(crate) use sys::{file_system_name, free_space, fstat, lstat, rename, stat};

#[cfg(windows)]
mod sys {
    use std::{
        ffi::c_void,
        fs::{File, OpenOptions},
        io,
        mem::size_of,
        os::windows::{
            ffi::{OsStrExt, OsStringExt},
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        },
        path::Path,
        time::Duration,
    };

    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FileAttributeTagInfo, FileBasicInfo, FileIdInfo, GetDiskFreeSpaceExW,
            GetFileInformationByHandle, GetFileInformationByHandleEx, GetVolumeInformationW,
            GetVolumePathNameW, MoveFileExW, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_TAG_INFO,
            FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        },
    };

    use super::{file_index, is_held_open, Info, ATTR_REPARSE_POINT, HELD_BACKOFF_MS};
    use crate::beneath::Stat;

    /// `ERROR_ACCESS_DENIED`.
    const ERROR_ACCESS_DENIED: i32 = 5;

    /// `path` as the NUL-terminated UTF-16 the wide calls take; a NUL inside it would end the path
    /// early (at another file), so such a path is refused.
    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
        if units.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a NUL in the path",
            ));
        }
        units.push(0);
        Ok(units)
    }

    /// The folder or file at `path` opened for its attributes only: no data access, so no sharing
    /// violation can come of another program holding it. With `follow` false a reparse point is
    /// opened as itself.
    fn open_attributes(path: &Path, follow: bool) -> io::Result<File> {
        let mut flags = FILE_FLAG_BACKUP_SEMANTICS;
        if !follow {
            flags |= FILE_FLAG_OPEN_REPARSE_POINT;
        }
        OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(flags)
            .open(path)
    }

    /// One `GetFileInformationByHandleEx` class into `out`.
    fn by_class<T>(handle: HANDLE, class: i32, out: &mut T) -> bool {
        // SAFETY: `handle` is open for the call (the caller holds the file it came from), `out` is a
        // plain `repr(C)` struct of the size passed.
        unsafe {
            GetFileInformationByHandleEx(
                handle,
                class,
                (out as *mut T).cast::<c_void>(),
                size_of::<T>() as u32,
            ) != 0
        }
    }

    /// What the system says of the file `handle` is open on. The handle needs the right to read
    /// attributes (every read handle has it; the files this crate makes are opened to read and write
    /// for that).
    fn info_of_handle(handle: HANDLE) -> io::Result<Info> {
        let mut by_handle = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: as `by_class`; the struct is the one this call fills.
        if unsafe { GetFileInformationByHandle(handle, &mut by_handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // The change time is not in the information above. A system that does not give it leaves it
        // 0, which no comparison uses.
        let mut basic = FILE_BASIC_INFO::default();
        let changed = if by_class(handle, FileBasicInfo, &mut basic) {
            basic.ChangeTime
        } else {
            0
        };
        // A reparse point's tag says whether it is a link or a file a filter serves; a tag that
        // cannot be read stays 0, which is a link (nothing is read through it).
        let mut reparse_tag = 0;
        if by_handle.dwFileAttributes & ATTR_REPARSE_POINT != 0 {
            let mut tag = FILE_ATTRIBUTE_TAG_INFO::default();
            if by_class(handle, FileAttributeTagInfo, &mut tag) {
                reparse_tag = tag.ReparseTag;
            }
        }
        // ReFS gives files 128-bit ids; its 64-bit index does not name one file. NTFS' id is the
        // index with zeros above it. A system that cannot answer keeps the index it has.
        let mut id = FILE_ID_INFO::default();
        let wide_id = by_class(handle, FileIdInfo, &mut id)
            && id.FileId.Identifier[8..].iter().any(|byte| *byte != 0);
        Ok(Info {
            attributes: by_handle.dwFileAttributes,
            reparse_tag,
            size: (u64::from(by_handle.nFileSizeHigh) << 32) | u64::from(by_handle.nFileSizeLow),
            written: filetime_ticks(
                by_handle.ftLastWriteTime.dwHighDateTime,
                by_handle.ftLastWriteTime.dwLowDateTime,
            ),
            changed,
            volume: u64::from(by_handle.dwVolumeSerialNumber),
            index: file_index(by_handle.nFileIndexHigh, by_handle.nFileIndexLow),
            wide_id,
            links: by_handle.nNumberOfLinks,
        })
    }

    fn filetime_ticks(high: u32, low: u32) -> i64 {
        i64::try_from(file_index(high, low)).unwrap_or(i64::MAX)
    }

    /// What std's own metadata says, for an entry that would not open for its attributes (an access
    /// control list that denies even that): no identity, no change time, every reparse point a link.
    fn info_of_metadata(meta: &std::fs::Metadata) -> Info {
        Info {
            attributes: meta.file_attributes(),
            reparse_tag: 0,
            size: meta.len(),
            written: i64::try_from(meta.last_write_time()).unwrap_or(i64::MAX),
            changed: 0,
            volume: 0,
            index: 0,
            wide_id: false,
            links: 1,
        }
    }

    fn info_at(path: &Path, follow: bool) -> io::Result<Info> {
        match open_attributes(path, follow) {
            Ok(file) => info_of_handle(file.as_raw_handle()),
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => {
                let meta = if follow {
                    std::fs::metadata(path)
                } else {
                    std::fs::symlink_metadata(path)
                }?;
                Ok(info_of_metadata(&meta))
            }
            Err(e) => Err(e),
        }
    }

    /// The `lstat` of the entry at `path`: a reparse point is itself.
    pub(crate) fn lstat(path: &Path) -> io::Result<Stat> {
        info_at(path, false).map(|info| info.stat())
    }

    /// The `stat` of the entry at `path`, a link followed.
    pub(crate) fn stat(path: &Path) -> io::Result<Stat> {
        info_at(path, true).map(|info| info.stat())
    }

    /// The `fstat` of an open file.
    pub(crate) fn fstat(file: &File) -> io::Result<Stat> {
        info_of_handle(file.as_raw_handle()).map(|info| info.stat())
    }

    /// The bytes free for this user on the disk of `path` (quotas counted). `path` is a folder of the
    /// disk; a UNC share needs its trailing backslash.
    pub(crate) fn free_space(path: &Path) -> io::Result<u64> {
        let mut folder = path.as_os_str().to_os_string();
        if !folder.to_string_lossy().ends_with('\\') {
            folder.push("\\");
        }
        let folder = wide(Path::new(&folder))?;
        let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
        // SAFETY: a NUL-terminated wide path and three out-pointers to u64s that live across the call.
        if unsafe { GetDiskFreeSpaceExW(folder.as_ptr(), &mut available, &mut total, &mut free) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(available)
    }

    /// The name of the file system of the volume `path` is on (`NTFS`, `FAT32`, `exFAT`, `ReFS`).
    pub(crate) fn file_system_name(path: &Path) -> io::Result<String> {
        const ROOT: usize = 1024;
        const NAME: usize = 64;
        let path = wide(path)?;
        let mut root = vec![0u16; ROOT];
        // SAFETY: a NUL-terminated wide path and a buffer of the length passed.
        if unsafe { GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), ROOT as u32) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut name = vec![0u16; NAME];
        // SAFETY: `root` is NUL-terminated by the call above; the name buffer has the length passed;
        // the other out-pointers are null, which the call allows.
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                name.as_mut_ptr(),
                NAME as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let end = name
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(name.len());
        Ok(std::ffi::OsString::from_wide(&name[..end])
            .to_string_lossy()
            .into_owned())
    }

    /// Renames `from` to `to`, written through to the disk before it returns
    /// (`MOVEFILE_WRITE_THROUGH`: NTFS has no folder to flush, so this is the one way a rename
    /// is known durable). With `replace` an existing `to` is replaced; without it the rename
    /// fails with `AlreadyExists` when `to` is taken, in the same step (no look between). A program
    /// that holds the file for a moment (a virus scanner, an indexer: a sharing or lock violation) is
    /// waited for through [`HELD_BACKOFF_MS`]; a replace that is refused for access - a read-only
    /// target, one open with delete sharing - is std's rename, which replaces both.
    pub(crate) fn rename(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
        let (source, target) = (wide(from)?, wide(to)?);
        let flags = MOVEFILE_WRITE_THROUGH
            | if replace {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        let mut waits = HELD_BACKOFF_MS.iter();
        loop {
            // SAFETY: two NUL-terminated wide paths that live across the call.
            if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) } != 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if is_held_open(&error) {
                if let Some(wait) = waits.next() {
                    std::thread::sleep(Duration::from_millis(*wait));
                    continue;
                }
            } else if replace && error.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
                return std::fs::rename(from, to);
            }
            return Err(error);
        }
    }
}
