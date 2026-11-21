//! Texts that leave the process without a secret in them (AZ-CN3, ARCH28 RF54).
//!
//! pCloud puts the user name and the PASSWORD in the query of every request it makes, WebHDFS its
//! delegation token, a user may type `https://ann:pw@cloud.example/dav` as an address, and the
//! failure text of an HTTP client or of OpenDAL names the URL it tried. That text reaches the red
//! line under the Add drive form ("Test failed: ..."), a toast, a log, a line on stdout and a bug
//! report - a screen share shows the password. [`scrub_url`] is applied where such a text is made:
//! at the transport's error text, at the OpenDAL error map, and at the lines AzDrive prints.
//!
//! What it keeps of a URL is what finds the server: scheme, host, port and path. The user name and
//! password before the `@`, the query and the fragment go, and a marker says that something went.
//! `password=...`, `token=...` and the like written outside a URL lose their value too.

/// What stands where a value was.
pub const HIDDEN: &str = "<hidden>";

/// The ends of a name whose value is a secret when it is written `name=value` (compared without
/// case): `password`, `access_token`, `X-Amz-Signature`, `client_secret`, ...
const SECRET_NAME_ENDS: &[&str] = &[
    "password",
    "passwd",
    "token",
    "secret",
    "signature",
    "credential",
    "apikey",
    "api_key",
    "api-key",
    "delegation",
    "authorization",
];

/// The names that are secret only when they are the whole name (`pass=`, but not `bypass=`).
const SECRET_NAMES: &[&str] = &["pwd", "pass", "sig", "sas"];

/// `text` with every URL in it cut down to scheme, host, port and path, and the value of every
/// `password=`, `token=`, ... pair outside a URL hidden. Text without a URL or a pair is returned
/// as it is; scrubbing again changes nothing.
#[must_use]
pub fn scrub_url(text: &str) -> String {
    hide_pairs(&hide_in_urls(text))
}

/// Whether `c` may be part of a URL scheme.
fn scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

/// Whether `c` ends a URL in running text.
fn ends_url(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`' | '\\')
}

/// Punctuation a sentence puts after a URL.
fn sentence_end(c: char) -> bool {
    matches!(c, '.' | ',' | ';' | ':' | '!' | ')' | ']' | '}')
}

/// Every `scheme://...` of `text` without its user info, query and fragment.
fn hide_in_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut done = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find("://") {
        let sep = from + found;
        // The scheme: the run of scheme characters before "://" that starts with a letter.
        let run_start = text[..sep]
            .char_indices()
            .rev()
            .find(|(_, c)| !scheme_char(*c))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let scheme_start = text[run_start..sep]
            .char_indices()
            .find(|(_, c)| c.is_ascii_alphabetic())
            .map(|(i, _)| run_start + i);
        let body_start = sep + 3;
        let Some(scheme_start) = scheme_start.filter(|start| *start >= done) else {
            from = body_start;
            continue;
        };
        let body_end = url_body_end(text, body_start);
        let mut url_end = body_end;
        while url_end > body_start
            && text[..url_end]
                .chars()
                .next_back()
                .is_some_and(sentence_end)
        {
            url_end -= text[..url_end]
                .chars()
                .next_back()
                .map_or(1, char::len_utf8);
        }
        out.push_str(&text[done..scheme_start]);
        out.push_str(&clean(&text[scheme_start..sep], &text[body_start..url_end]));
        done = url_end;
        from = body_end;
    }
    out.push_str(&text[done..]);
    out
}

/// Where the URL whose body starts at `start` ends in `text`: at the first character that ends a
/// URL in running text, except a [`HIDDEN`] marker (so a scrubbed text scrubs to itself).
fn url_body_end(text: &str, start: usize) -> usize {
    let mut i = start;
    while i < text.len() {
        if text[i..].starts_with(HIDDEN) {
            i += HIDDEN.len();
            continue;
        }
        let Some(c) = text[i..].chars().next() else {
            break;
        };
        if ends_url(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// The URL `scheme://body` without what could carry a secret.
fn clean(scheme: &str, body: &str) -> String {
    let (rest, fragment) = match body.split_once('#') {
        Some((rest, fragment)) => (rest, !fragment.is_empty()),
        None => (body, false),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, !query.is_empty()),
        None => (rest, false),
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(authority_end);
    // The user info is everything up to the LAST '@' of the authority (a password may hold one).
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let mut url = format!("{scheme}://{host}{path}");
    if query {
        url.push('?');
        url.push_str(HIDDEN);
    }
    if fragment {
        url.push('#');
        url.push_str(HIDDEN);
    }
    url
}

/// Whether `name` (a key of a `key=value` pair) names a secret.
fn is_secret_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    SECRET_NAMES.contains(&name.as_str()) || SECRET_NAME_ENDS.iter().any(|end| name.ends_with(end))
}

/// Whether `c` may be part of the name of a pair.
fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// Whether `c` ends the value of a pair.
fn ends_value(c: char) -> bool {
    c.is_whitespace() || matches!(c, '&' | ',' | ';' | '"' | '\'' | ')' | '<' | '>' | '`')
}

/// The value of every `secret-name=value` pair of `text` hidden.
fn hide_pairs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut done = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find('=') {
        let eq = from + found;
        from = eq + 1;
        let name_start = text[done..eq]
            .char_indices()
            .rev()
            .find(|(_, c)| !name_char(*c))
            .map_or(done, |(i, c)| done + i + c.len_utf8());
        let value_start = eq + 1;
        let value_end = text[value_start..]
            .find(ends_value)
            .map_or(text.len(), |i| value_start + i);
        let value = &text[value_start..value_end];
        if value.is_empty() || value == HIDDEN || !is_secret_name(&text[name_start..eq]) {
            continue;
        }
        out.push_str(&text[done..value_start]);
        out.push_str(HIDDEN);
        done = value_end;
        from = value_end;
    }
    out.push_str(&text[done..]);
    out
}
