//! One file's contents searched: `grep-searcher` reads it in 64 KB pieces (never the whole file
//! at once), quits at the first NUL byte (a binary file), transcodes a UTF-16 file with a
//! byte-order mark, keeps a few context lines, and hands every matching line to a sink that
//! keeps it as a [`LineMatch`]. The reader asks the stop flag before every piece: a cancelled
//! search leaves a big file at once.

use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use grep_searcher::{
    BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkContextKind, SinkMatch,
};

use crate::{walk::Stop, ContentMatcher, LineMatch, MAX_LINE_BYTES};

/// The most memory a line may take while it is searched (bytes): a longer line (a minified
/// file, a database dump) ends that file's search as unreadable.
const LINE_HEAP_LIMIT: usize = 16 * 1024 * 1024;

/// What the search of one file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileResult {
    /// Its matching lines (`true`: it has more than were kept).
    Lines(Vec<LineMatch>, bool),
    /// No line matches.
    Nothing,
    /// A binary file: passed over.
    Binary,
    /// Larger than the limit: not read.
    TooLarge,
    /// It could not be opened or read.
    Failed,
    /// The search was stopped while it was read.
    Stopped,
}

/// The searcher a walker's thread reuses for every file it reads.
pub(crate) fn searcher(context: usize) -> Searcher {
    SearcherBuilder::new()
        .line_number(true)
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .before_context(context)
        .after_context(context)
        .heap_limit(Some(LINE_HEAP_LIMIT))
        .build()
}

/// How a file is read for a content search.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Reading {
    /// A larger file is not read (bytes).
    pub(crate) max_size: u64,
    /// The most matching lines kept.
    pub(crate) keep: usize,
    /// A file with a UTF-16 byte-order mark is read as text; `false`: it is binary.
    pub(crate) utf16: bool,
}

/// Whether `head` (a file's first bytes) is a UTF-16 byte-order mark.
fn utf16_mark(head: [u8; 2]) -> bool {
    matches!(head, [0xFF, 0xFE] | [0xFE, 0xFF])
}

/// Searches the file at `path` for `matcher` as `reading` says.
pub(crate) fn search_file(
    path: &Path,
    matcher: &ContentMatcher,
    searcher: &mut Searcher,
    reading: Reading,
    stop: Stop<'_>,
) -> FileResult {
    let Ok(mut file) = File::open(path) else {
        return FileResult::Failed;
    };
    match file.metadata() {
        Ok(meta) if meta.len() > reading.max_size => return FileResult::TooLarge,
        Ok(_) => {}
        Err(_) => return FileResult::Failed,
    }
    if !reading.utf16 {
        let mut head = [0u8; 2];
        if file.read_exact(&mut head).is_ok() && utf16_mark(head) {
            return FileResult::Binary;
        }
        if file.seek(SeekFrom::Start(0)).is_err() {
            return FileResult::Failed;
        }
    }
    search_reader(file, matcher, searcher, reading.keep, stop)
}

/// Searches what `reader` reads (a file's bytes) - the part of [`search_file`] after the open.
pub(crate) fn search_reader<R: Read>(
    reader: R,
    matcher: &ContentMatcher,
    searcher: &mut Searcher,
    keep: usize,
    stop: Stop<'_>,
) -> FileResult {
    let mut sink = Collect {
        matcher,
        keep,
        lines: Vec::new(),
        before: Vec::new(),
        more: false,
        binary: false,
    };
    let result = searcher.search_reader(
        &matcher.matcher,
        Interruptible {
            inner: reader,
            stop,
        },
        &mut sink,
    );
    if stop.is_set() {
        return FileResult::Stopped;
    }
    if result.is_err() {
        return FileResult::Failed;
    }
    if sink.binary {
        FileResult::Binary
    } else if sink.lines.is_empty() {
        FileResult::Nothing
    } else {
        FileResult::Lines(sink.lines, sink.more)
    }
}

/// A reader that fails once the search is stopped: the searcher asks it for every 64 KB, so a
/// stop is heard within one piece of a file however big the file is.
struct Interruptible<'s, R> {
    inner: R,
    stop: Stop<'s>,
}

impl<R: Read> Read for Interruptible<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.stop.is_set() {
            return Err(io::Error::other("the search was stopped"));
        }
        self.inner.read(buf)
    }
}

/// The sink that keeps the matching lines of one file (and their context lines).
struct Collect<'m> {
    matcher: &'m ContentMatcher,
    /// The most lines kept.
    keep: usize,
    lines: Vec<LineMatch>,
    /// The context lines since the last match, waiting for the next one.
    before: Vec<String>,
    /// A line matched after `keep` were kept.
    more: bool,
    /// A NUL byte: the file is binary, its lines are dropped.
    binary: bool,
}

impl Sink for Collect<'_> {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, found: &SinkMatch<'_>) -> Result<bool, io::Error> {
        if self.lines.len() >= self.keep {
            self.more = true;
            return Ok(false);
        }
        let before = std::mem::take(&mut self.before);
        let number = found.line_number().unwrap_or(0);
        self.lines
            .push(line_match(self.matcher, number, found.bytes(), before));
        Ok(true)
    }

    fn context(
        &mut self,
        _searcher: &Searcher,
        context: &SinkContext<'_>,
    ) -> Result<bool, io::Error> {
        let text = cut(&String::from_utf8_lossy(without_line_break(context.bytes())));
        match context.kind() {
            SinkContextKind::Before => self.before.push(text),
            SinkContextKind::After => {
                if let Some(last) = self.lines.last_mut() {
                    last.after.push(text);
                }
            }
            SinkContextKind::Other => {}
        }
        Ok(true)
    }

    fn binary_data(&mut self, _searcher: &Searcher, _offset: u64) -> Result<bool, io::Error> {
        self.binary = true;
        Ok(false)
    }
}

/// A line without its `\n` (and the `\r` of a CRLF line).
fn without_line_break(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// `text` cut to [`MAX_LINE_BYTES`] at a character's boundary (a context line).
fn cut(text: &str) -> String {
    if text.len() <= MAX_LINE_BYTES {
        return text.to_string();
    }
    let mut end = MAX_LINE_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// The part of `line` a match keeps: all of it, or (a long line) a window of about
/// [`MAX_LINE_BYTES`] around its first match - a quarter before it - on characters' boundaries.
fn window(line: &str, first: Option<(usize, usize)>) -> (usize, usize) {
    if line.len() <= MAX_LINE_BYTES {
        return (0, line.len());
    }
    let (start, end) = first.unwrap_or((0, 0));
    let mut from = start.saturating_sub(MAX_LINE_BYTES / 4).min(line.len());
    let mut to = from.saturating_add(MAX_LINE_BYTES).max(end).min(line.len());
    while !line.is_char_boundary(from) {
        from -= 1;
    }
    while !line.is_char_boundary(to) {
        to += 1;
    }
    (from, to)
}

/// A matching line as the searcher handed it over (with its line break): its text as UTF-8,
/// where `matcher` matches in it, the column of the first match, the context lines before it.
pub(crate) fn line_match(
    matcher: &ContentMatcher,
    number: u64,
    bytes: &[u8],
    before: Vec<String>,
) -> LineMatch {
    let line = String::from_utf8_lossy(without_line_break(bytes));
    let mut ranges = matcher.find_all(&line);
    let column = ranges
        .first()
        .and_then(|r| line.get(..r.0))
        .map_or(1, |lead| lead.chars().count() + 1);
    let (from, to) = window(&line, ranges.first().copied());
    ranges.retain(|r| r.0 >= from && r.1 <= to);
    LineMatch {
        line: number,
        column,
        text: line[from..to].to_string(),
        text_offset: from,
        ranges,
        before,
        after: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::Pattern;

    fn stop<'a>(cancel: &'a AtomicBool, limit: &'a AtomicBool) -> Stop<'a> {
        Stop { cancel, limit }
    }

    fn search(text: &[u8], pattern: Pattern, context: usize, keep: usize) -> FileResult {
        let matcher = ContentMatcher::new(&pattern).expect("compiles");
        let (cancel, limit) = (AtomicBool::new(false), AtomicBool::new(false));
        search_reader(text, &matcher, &mut searcher(context), keep, stop(&cancel, &limit))
    }

    #[test]
    fn a_matching_line_keeps_its_number_its_column_and_its_matches() {
        let text = b"pub fn picked() -> u32 {\r\n    7\r\n}\r\n// picked twice, picked\r\n";
        let FileResult::Lines(lines, more) = search(text, Pattern::literal("picked"), 0, 10)
        else {
            panic!("no lines");
        };
        assert!(!more);
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].line, lines[0].column), (1, 8));
        assert_eq!(lines[1].text, "// picked twice, picked", "without the CRLF");
        assert_eq!(lines[1].ranges, vec![(3, 9), (17, 23)]);
        assert_eq!(lines[1].text_offset, 0);
    }

    #[test]
    fn context_lines_come_with_their_match() {
        let text = b"one\ntwo\nneedle\nfour\nfive\nsix\n";
        let FileResult::Lines(lines, _) = search(text, Pattern::literal("needle"), 1, 10) else {
            panic!("no lines");
        };
        assert_eq!(lines[0].before, vec![String::from("two")]);
        assert_eq!(lines[0].after, vec![String::from("four")]);
    }

    #[test]
    fn a_file_keeps_only_so_many_lines_and_says_there_are_more() {
        let text = b"x\nx\nx\nx\n";
        let FileResult::Lines(lines, more) = search(text, Pattern::literal("x"), 0, 2) else {
            panic!("no lines");
        };
        assert_eq!(lines.len(), 2);
        assert!(more);
    }

    #[test]
    fn a_nul_byte_makes_a_file_binary_and_its_lines_are_dropped() {
        assert_eq!(
            search(b"picked\n\0\0\0", Pattern::literal("picked"), 0, 10),
            FileResult::Binary
        );
        assert_eq!(search(b"nothing\n", Pattern::literal("picked"), 0, 10), FileResult::Nothing);
    }

    #[test]
    fn a_utf16_file_with_a_byte_order_mark_is_read_as_text() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "first\nthe picked one\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let FileResult::Lines(lines, _) = search(&bytes, Pattern::literal("picked"), 0, 10) else {
            panic!("the UTF-16 file was not read as text");
        };
        assert_eq!((lines[0].line, lines[0].text.as_str()), (2, "the picked one"));
    }

    #[test]
    fn a_long_line_keeps_a_window_around_its_first_match() {
        let line = format!("{}needle{}\n", "a".repeat(5000), "b".repeat(5000));
        let FileResult::Lines(lines, _) =
            search(line.as_bytes(), Pattern::literal("needle"), 0, 10)
        else {
            panic!("no lines");
        };
        let m = &lines[0];
        assert!(m.text.len() <= MAX_LINE_BYTES + 6, "{}", m.text.len());
        assert_eq!(m.ranges, vec![(5000, 5006)]);
        let (start, end) = m.ranges[0];
        assert_eq!(&m.text[start - m.text_offset..end - m.text_offset], "needle");
        assert_eq!(m.column, 5001);
    }

    #[test]
    fn a_stopped_search_leaves_the_file_at_once() {
        let matcher = ContentMatcher::new(&Pattern::literal("x")).expect("compiles");
        let (cancel, limit) = (AtomicBool::new(true), AtomicBool::new(false));
        let result = search_reader(
            &b"x\n"[..],
            &matcher,
            &mut searcher(0),
            10,
            stop(&cancel, &limit),
        );
        assert_eq!(result, FileResult::Stopped);
    }

    /// A file that never ends: lines of `a`s without a match.
    struct Endless;

    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            for (i, byte) in buf.iter_mut().enumerate() {
                *byte = if i % 8 == 7 { b'\n' } else { b'a' };
            }
            Ok(buf.len())
        }
    }

    /// A cancel from another thread ends the read of a file that would never end, within
    /// milliseconds (the reader asks before every 64 KB).
    #[test]
    fn a_cancel_from_another_thread_ends_the_read_of_an_endless_file() {
        let matcher = ContentMatcher::new(&Pattern::literal("picked")).expect("compiles");
        let (cancel, limit) = (AtomicBool::new(false), AtomicBool::new(false));
        let (said, heard) = std::sync::mpsc::channel::<std::time::Instant>();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(20));
                cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = said.send(std::time::Instant::now());
            });
            let result =
                search_reader(Endless, &matcher, &mut searcher(0), 10, stop(&cancel, &limit));
            let ended = std::time::Instant::now();
            assert_eq!(result, FileResult::Stopped);
            let cancelled = heard.recv().expect("the cancel's time");
            let late = ended.saturating_duration_since(cancelled);
            assert!(late < std::time::Duration::from_millis(500), "it read on for {late:?}");
        });
    }
}
