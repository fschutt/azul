//! What the index holds, beside it: every file it has read (indexed or found to hold no text)
//! with the size and date it had then - the next update reads only the files whose size or date
//! changed, and drops the ones that are gone. One line per file, written whole (a temporary
//! file renamed over the old one) after every commit of the index, so the list and the index
//! never disagree about a file they both know.

use std::{
    collections::HashMap,
    fs, io,
    path::Path,
};

/// The first line of the list: its format.
const HEADER: &str = "azul-search-index files 1";

/// The files the index has read, and when it last looked at the drive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct State {
    /// By path below the drive's folder: (size, date) as they were read.
    pub(crate) files: HashMap<String, (u64, Option<u64>)>,
    /// When an update last went over the drive (seconds since 1970).
    pub(crate) updated: Option<u64>,
}

/// A path with `\`, tab and line break escaped (a file name may hold them).
fn escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn number(text: &str) -> Option<u64> {
    if text == "-" {
        None
    } else {
        text.parse().ok()
    }
}

/// The list at `path`; an empty one when it is missing or not in this format (the index is
/// then read again in full).
pub(crate) fn read(path: &Path) -> State {
    let Ok(text) = fs::read_to_string(path) else {
        return State::default();
    };
    let mut lines = text.lines();
    let Some((header, updated)) = lines.next().and_then(|l| l.split_once('\t')) else {
        return State::default();
    };
    if header != HEADER {
        return State::default();
    }
    let mut state = State {
        files: HashMap::new(),
        updated: number(updated),
    };
    for line in lines {
        let mut fields = line.splitn(3, '\t');
        let (Some(size), Some(modified), Some(file)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(size) = size.parse() else {
            continue;
        };
        state.files.insert(unescape(file), (size, number(modified)));
    }
    state
}

/// Writes `state` to `path`: a temporary file beside it, renamed over it.
pub(crate) fn write(path: &Path, state: &State) -> io::Result<()> {
    let mut text = String::with_capacity(64 + state.files.len() * 48);
    let date = |value: Option<u64>| value.map_or_else(|| String::from("-"), |v| v.to_string());
    text.push_str(HEADER);
    text.push('\t');
    text.push_str(&date(state.updated));
    text.push('\n');
    let mut files: Vec<(&String, &(u64, Option<u64>))> = state.files.iter().collect();
    files.sort();
    for (file, (size, modified)) in files {
        text.push_str(&size.to_string());
        text.push('\t');
        text.push_str(&date(*modified));
        text.push('\t');
        text.push_str(&escape(file));
        text.push('\n');
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}
