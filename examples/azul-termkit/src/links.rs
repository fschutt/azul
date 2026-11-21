//! The addresses in a terminal's text: a Cmd+click (Ctrl+click elsewhere) on one opens it.
//! Only web, mail and file addresses open; anything else under the click is text.

/// The schemes an address may open with.
const SCHEMES: &[&str] = &["https://", "http://", "mailto:", "file://"];

/// Whether `c` may stand in an address as a terminal shows it: no blank, no control
/// character, none of the characters that end one in running text.
fn in_address(c: char) -> bool {
    !(c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '`' | '\0'))
}

/// The address the `column` (a char of `row`, one per terminal cell) is in: web, mail or file,
/// without the punctuation after it that ends a sentence, nor a bracket around it (one of its
/// own, as in a wiki address, stays). `None` when the column is in no such address.
#[must_use]
pub fn url_at(row: &str, column: usize) -> Option<String> {
    let chars: Vec<char> = row.chars().collect();
    if column >= chars.len() || !in_address(chars[column]) {
        return None;
    }
    let mut start = column;
    while start > 0 && in_address(chars[start - 1]) {
        start -= 1;
    }
    let mut end = column + 1;
    while end < chars.len() && in_address(chars[end]) {
        end += 1;
    }
    let token: String = chars[start..end].iter().collect();
    // The last scheme that starts at or before the column ("(http://..." starts at "http").
    let at = column - start;
    let scheme_at = SCHEMES
        .iter()
        .filter_map(|scheme| {
            token
                .char_indices()
                .filter(|(i, _)| token[*i..].starts_with(scheme))
                .map(|(i, _)| token[..i].chars().count())
                .filter(|offset| *offset <= at)
                .max()
                .map(|offset| (offset, *scheme))
        })
        .max_by_key(|(offset, _)| *offset)?;
    let (offset, scheme) = scheme_at;
    let mut url: Vec<char> = token.chars().skip(offset).collect();
    // What ends a sentence after an address is not part of it; nor is a closing bracket the
    // address does not open itself.
    loop {
        let Some(&last) = url.last() else {
            break;
        };
        let unbalanced = |open: char, close: char| {
            last == close
                && url.iter().filter(|c| **c == close).count()
                    > url.iter().filter(|c| **c == open).count()
        };
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?')
            || unbalanced('(', ')')
            || unbalanced('[', ']')
            || unbalanced('{', '}')
        {
            url.pop();
        } else {
            break;
        }
    }
    let len = url.len();
    if at < offset || at >= offset + len {
        return None;
    }
    let url: String = url.into_iter().collect();
    let rest = &url[scheme.len()..];
    let valid = match scheme {
        "mailto:" => rest.contains('@') && !rest.starts_with('@') && !rest.ends_with('@'),
        _ => !rest.is_empty(),
    };
    valid.then_some(url)
}

/// The path a `file://` address names (`file:///tmp/a%20b.txt` is `/tmp/a b.txt`; a
/// `localhost` host is this machine); `None` for another host or no file address.
#[must_use]
pub fn file_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
