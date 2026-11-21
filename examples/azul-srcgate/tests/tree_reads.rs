//! The kit reads the whole tree: the lexer takes every `.rs` file of azul's crates without a
//! panic, and `code` keeps every byte count and line of every one (AR5b-0's acceptance gate: a
//! lexer that chokes on a file would let a gate skip it).

use std::path::Path;

use azul_srcgate::{code, lex_spans, workspace};

/// Every `.rs` file below `dir` but in a `target` folder.
fn rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for p in rd.flatten().map(|e| e.path()) {
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target" || n == ".git") {
                continue;
            }
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn every_rs_file_of_the_workspace_is_lexed_and_blanked_without_losing_a_byte_or_a_line() {
    let base = workspace();
    let mut files = Vec::new();
    for dir in ["core", "css", "layout", "dll", "doc", "examples"] {
        rs_files(&base.join(dir), &mut files);
    }
    assert!(
        files.len() > 1000,
        "{} files: the scan read the wrong place",
        files.len()
    );
    let mut unreadable = Vec::new();
    let mut bytes = 0usize;
    for path in &files {
        let Ok(src) = std::fs::read_to_string(path) else {
            unreadable.push(path.display().to_string());
            continue;
        };
        bytes += src.len();
        let (tokens, comments) = lex_spans(&src);
        // the spans lie inside the file, on char boundaries, in order
        let mut last = 0;
        for (_, span) in &tokens {
            assert!(
                span.start >= last
                    && span.end <= src.len()
                    && src.is_char_boundary(span.start)
                    && src.is_char_boundary(span.end),
                "{}: a token span {span:?} is out of order or inside a char",
                path.display()
            );
            last = span.start;
        }
        for span in &comments {
            assert!(
                src.is_char_boundary(span.start) && src.is_char_boundary(span.end),
                "{}",
                path.display()
            );
        }
        let blanked = code(&src);
        assert_eq!(
            blanked.len(),
            src.len(),
            "{}: the blanked text keeps every byte",
            path.display()
        );
        assert_eq!(
            blanked.matches('\n').count(),
            src.matches('\n').count(),
            "{}: and every line break",
            path.display()
        );
    }
    // A file that is not UTF-8 is not Rust source (a fixture); there are few.
    assert!(unreadable.len() < 10, "{unreadable:#?}");
    assert!(bytes > 10_000_000, "{bytes} bytes read");
}
