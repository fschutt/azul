//! The recorder's session folders on disk: creating them, writing the manifest atomically, putting
//! right what a crash left, listing the recordings and deleting one.
//!
//! A session is a folder under the recordings folder (`recorder_manifest.rs` describes what is in it).
//! [`DiskStore`] gives the track writers their files (new ones only: an earlier run's file is never
//! overwritten) and [`ManifestFile`] writes `manifest.json` through a temporary file and a rename, so
//! the manifest on disk is always a whole one. [`recover_session`] runs at the next start for a
//! session whose manifest still says `recording`: every segment is cut back to its last whole
//! fragment, a segment the manifest does not know (opened in the last moments) is removed, the line
//! files lose a torn last line, the open run ends at the last time that is certain, and the manifest
//! says `recovered`.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    recorder_manifest::{
        is_safe_relative, FileInfo, Manifest, SessionState, EVENTS_FILE, MANIFEST_FILE, TRACK_DIRS, TRANSCRIPT_FILE,
    },
    recorder_mp4::scan,
    recorder_track::{SegmentFile, SegmentStore},
    recorder_writer::ManifestSink,
};

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// The segment files of one session folder.
#[derive(Debug, Clone)]
pub struct DiskStore {
    root: PathBuf,
}

impl DiskStore {
    /// The store of the session folder `root` (which exists).
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

struct DiskFile {
    file: File,
    len: u64,
}

impl SegmentStore for DiskStore {
    fn create(&mut self, rel_path: &str) -> io::Result<Box<dyn SegmentFile>> {
        if !is_safe_relative(rel_path) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{rel_path:?} is not a plain relative path")));
        }
        let path = self.root.join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Box::new(DiskFile { file, len: 0 }))
    }
}

impl SegmentFile for DiskFile {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)?;
        self.len += bytes.len() as u64;
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.file.sync_data()
    }

    fn len(&self) -> u64 {
        self.len
    }
}

/// Replaces `path` with `bytes` through a temporary file in the same folder and a rename: a reader
/// (or a crash) sees the old file or the new one, never half of one.
///
/// # Errors
/// The file could not be written or renamed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path.file_name().map_or_else(|| String::from("file"), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!("{name}.tmp"));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent() {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

/// The manifest file of a session, written atomically.
#[derive(Debug, Clone)]
pub struct ManifestFile {
    path: PathBuf,
}

impl ManifestFile {
    /// The manifest of the session folder `session_dir`.
    #[must_use]
    pub fn new(session_dir: &Path) -> Self {
        Self {
            path: session_dir.join(MANIFEST_FILE),
        }
    }
}

impl ManifestSink for ManifestFile {
    fn write(&mut self, text: &str) -> io::Result<()> {
        write_atomic(&self.path, text.as_bytes())
    }
}

/// Makes the folder of a new session under `recordings`: `<base>`, or `<base>-2`, `<base>-3`, ...
/// when a session of that second exists. Returns its name and path.
///
/// # Errors
/// The folder could not be made.
pub fn new_session_dir(recordings: &Path, base: &str) -> io::Result<(String, PathBuf)> {
    fs::create_dir_all(recordings)?;
    for n in 1..=99 {
        let name = if n == 1 { base.to_string() } else { format!("{base}-{n}") };
        let path = recordings.join(&name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok((name, path)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "there are 99 sessions of the same second"))
}

/// A file that is appended to, made when it is not there (the events and the transcript).
///
/// # Errors
/// The file could not be opened.
pub fn open_lines(path: &Path) -> io::Result<File> {
    OpenOptions::new().append(true).create(true).open(path)
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

/// What recovering a session did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Recovery {
    pub session: String,
    /// The session was left recording and has been put right (a session in any other state is not).
    pub recovered: bool,
    /// Segments cut back to their last whole fragment.
    pub cut: usize,
    /// Segment files removed: empty ones, and ones the manifest did not know.
    pub removed: usize,
    /// Segments that were already whole (closed properly; only the manifest was behind).
    pub whole: usize,
    /// Bytes cut off, in all.
    pub lost_bytes: u64,
    /// The line files had a torn last line.
    pub lines_cut: bool,
}

/// Cuts a file of lines back to its last newline; the number of lines and bytes it holds, and whether a
/// torn tail was cut.
fn cut_torn_line(path: &Path) -> io::Result<(u64, u64, bool)> {
    let mut f = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((0, 0, false)),
        Err(e) => return Err(e),
    };
    let len = f.metadata()?.len();
    let (mut lines, mut last_newline_end) = (0_u64, 0_u64);
    let mut buf = vec![0_u8; 64 * 1024];
    let mut pos = 0_u64;
    f.seek(SeekFrom::Start(0))?;
    while pos < len {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for (i, b) in buf[..n].iter().enumerate() {
            if *b == b'\n' {
                lines += 1;
                last_newline_end = pos + i as u64 + 1;
            }
        }
        pos += n as u64;
    }
    let cut = last_newline_end < len;
    if cut {
        f.set_len(last_newline_end)?;
    }
    Ok((lines, last_newline_end, cut))
}

/// Puts right the session in `dir` whose manifest says `recording` (the app ended without stopping
/// it). A session in any other state is left as it is.
///
/// # Errors
/// The manifest cannot be read or written; what is wrong with it.
pub fn recover_session(dir: &Path) -> Result<Recovery, String> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let text = fs::read_to_string(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let mut m = Manifest::parse(&text).map_err(|e| e.to_string())?;
    let mut report = Recovery {
        session: m.session.clone(),
        ..Recovery::default()
    };
    if m.state != SessionState::Recording {
        return Ok(report);
    }
    let mut latest_end = m.runs.last().map_or(0, |r| r.start_us);
    for (name, track) in [("screen", &mut m.tracks.screen), ("camera", &mut m.tracks.camera), ("audio", &mut m.tracks.audio)] {
        let Some(track) = track.as_mut() else { continue };
        let mut kept = Vec::new();
        for mut seg in std::mem::take(&mut track.segments) {
            let path = dir.join(&seg.file);
            let Ok(mut file) = File::open(&path) else {
                report.removed += 1;
                m.diagnostics.push(crate::recorder_manifest::Diagnostic {
                    t_us: seg.start_us,
                    level: String::from("warn"),
                    text: format!("{} was missing at recovery and was left out", seg.file),
                });
                continue;
            };
            let Ok(s) = scan(&mut file) else {
                drop(file);
                let _ = fs::remove_file(&path);
                report.removed += 1;
                continue;
            };
            drop(file);
            if s.is_empty() {
                let _ = fs::remove_file(&path);
                report.removed += 1;
                continue;
            }
            if s.truncated {
                let f = OpenOptions::new().write(true).open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                f.set_len(s.whole_end).map_err(|e| format!("{}: {e}", path.display()))?;
                report.lost_bytes += s.file_len - s.whole_end;
                report.cut += 1;
            }
            if s.finished && !s.truncated {
                // Closed properly: only the manifest was behind.
                report.whole += 1;
            } else {
                // Not closed by the writer: what the file holds is what survived.
                seg.recovered = true;
            }
            seg.bytes = s.whole_end;
            seg.samples = s.samples;
            seg.complete = true;
            // The segment's own time starts at its first sample, which is at 0 in files made here.
            seg.end_us = Some(seg.start_us + s.first_us() + s.duration_us());
            latest_end = latest_end.max(seg.end_us.unwrap_or(0));
            kept.push(seg);
        }
        // Files in the track's folder the manifest never heard of (opened in the last moments).
        if let Ok(rd) = fs::read_dir(dir.join(name)) {
            for entry in rd.flatten() {
                let rel = format!("{name}/{}", entry.file_name().to_string_lossy());
                if !kept.iter().any(|s| s.file == rel) && entry.path().extension().is_some_and(|e| e == "mp4") {
                    let _ = fs::remove_file(entry.path());
                    report.removed += 1;
                }
            }
        }
        track.segments = kept;
        track.status = crate::recorder_manifest::TrackStatus::Stopped;
    }
    for (file, info) in [(EVENTS_FILE, &mut m.events), (TRANSCRIPT_FILE, &mut m.transcript)] {
        let (lines, bytes, cut) = cut_torn_line(&dir.join(file)).map_err(|e| format!("{file}: {e}"))?;
        report.lines_cut |= cut;
        if lines > 0 || info.is_some() {
            let dropped = info.as_ref().map_or(0, |i| i.dropped);
            *info = Some(FileInfo { file: file.to_string(), count: lines, bytes, dropped });
        }
    }
    if let Some(run) = m.runs.last_mut().filter(|r| r.end_us.is_none()) {
        run.end_us = Some(latest_end.max(run.start_us));
        run.end_reason = Some(String::from("crash"));
    }
    m.state = SessionState::Recovered;
    report.recovered = true;
    m.note(
        latest_end,
        "warn",
        &format!(
            "the app ended without stopping this recording: {} segment(s) were cut back to the last whole fragment ({} bytes lost), {} file(s) removed",
            report.cut, report.lost_bytes, report.removed
        ),
    );
    write_atomic(&manifest_path, m.to_json().as_bytes()).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    Ok(report)
}

/// Recovers every session under `recordings` that was left recording; what each did, and the
/// sessions that could not be recovered with the words.
#[must_use]
pub fn recover_all(recordings: &Path) -> (Vec<Recovery>, Vec<(String, String)>) {
    let (mut done, mut failed) = (Vec::new(), Vec::new());
    let Ok(rd) = fs::read_dir(recordings) else {
        return (done, failed);
    };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join(MANIFEST_FILE).is_file()).collect();
    dirs.sort();
    for dir in dirs {
        let name = dir.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match recover_session(&dir) {
            Ok(r) if r.recovered => done.push(r),
            Ok(_) => {}
            Err(e) => failed.push((name, e)),
        }
    }
    (done, failed)
}

// ---------------------------------------------------------------------------
// The recordings list
// ---------------------------------------------------------------------------

/// One recording in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingInfo {
    pub name: String,
    pub state: Option<SessionState>,
    pub started_unix_ms: Option<u64>,
    /// The time it was recording: the length of its runs.
    pub recorded_us: u64,
    /// Bytes of all the files in its folder.
    pub bytes: u64,
    /// The tracks it has.
    pub tracks: Vec<&'static str>,
    /// What is wrong with it (the manifest cannot be read), in words.
    pub problem: Option<String>,
}

fn dir_bytes(dir: &Path, depth: u32) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() && depth > 0 => dir_bytes(&e.path(), depth - 1),
            Ok(m) if m.is_file() => m.len(),
            _ => 0,
        })
        .sum()
}

/// The recordings under `recordings`, the newest first. A folder with no manifest is no recording.
#[must_use]
pub fn list_recordings(recordings: &Path) -> Vec<RecordingInfo> {
    let Ok(rd) = fs::read_dir(recordings) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let dir = entry.path();
        let manifest_path = dir.join(MANIFEST_FILE);
        if !manifest_path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let bytes = dir_bytes(&dir, 3);
        let info = match fs::read_to_string(&manifest_path).map_err(|e| e.to_string()).and_then(|t| Manifest::parse(&t).map_err(|e| e.to_string())) {
            Ok(m) => RecordingInfo {
                name,
                state: Some(m.state),
                started_unix_ms: Some(m.clock.origin_unix_ms),
                recorded_us: m.recorded_us(m.end_us()),
                bytes,
                tracks: m.tracks.present().into_iter().map(|(n, _)| n).collect(),
                problem: None,
            },
            Err(e) => RecordingInfo {
                name,
                state: None,
                started_unix_ms: None,
                recorded_us: 0,
                bytes,
                tracks: Vec::new(),
                problem: Some(e),
            },
        };
        out.push(info);
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

/// Deletes the recording called `name` under `recordings`; the bytes freed. Only a plain folder with
/// a manifest in it goes: a link, a path, or a folder that is not a recording is refused.
///
/// # Errors
/// What was refused, or what the system said.
pub fn delete_recording(recordings: &Path, name: &str) -> io::Result<u64> {
    let invalid = |why: &str| io::Error::new(io::ErrorKind::InvalidInput, why.to_string());
    if !is_safe_relative(name) || name.contains('/') {
        return Err(invalid("not the name of a recording"));
    }
    let dir = recordings.join(name);
    let meta = fs::symlink_metadata(&dir)?;
    if !meta.is_dir() {
        return Err(invalid("a link or a file is not a recording"));
    }
    if !dir.join(MANIFEST_FILE).is_file() {
        return Err(invalid("this folder has no manifest: it is not a recording"));
    }
    let freed = dir_bytes(&dir, 4);
    fs::remove_dir_all(&dir)?;
    Ok(freed)
}

/// The track folders, for a caller that wants to say where a recording's media is.
#[must_use]
pub fn track_folders() -> &'static [&'static str] {
    &TRACK_DIRS
}

#[cfg(test)]
#[path = "recorder_fs_test.rs"]
mod recorder_fs_test;
