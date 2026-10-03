//! Articles through azul's HTML5-like parser (`Xml::create_from_html`, the lenient tokenizer
//! and the browser-like tree construction every azul loader shares).
//!
//! - [`plain_text`] / [`excerpt`]: the text of a piece of HTML (a title that holds `&amp;` or
//!   `<code>`, a description for the list's two lines), its character references decoded by
//!   the parser - the engine's one table of HTML's names.

/// The text of `html`: references decoded, scripts and styles left out, blocks apart, every
/// run of white space (a no-break space too) one space, trimmed.
#[must_use]
pub fn plain_text(_html: &str) -> String {
    String::new()
}

/// [`plain_text`] cut to at most `max_chars` characters at a word's end, with `…` when cut.
#[must_use]
pub fn excerpt(_html: &str, _max_chars: usize) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_decodes_references_and_keeps_blocks_apart() {
        assert_eq!(plain_text("<p>Hello <b>world</b></p><p>Second</p>"), "Hello world Second");
        assert_eq!(plain_text("Tom &amp; Jerry&rsquo;s &nbsp; caf&eacute;"), "Tom & Jerry\u{2019}s caf\u{e9}");
        assert_eq!(plain_text("a<br>b<script>x()</script>c"), "a bc");
        assert_eq!(plain_text("<style>p { color: red }</style>  spaced \n\t out  "), "spaced out");
        assert_eq!(plain_text("x < y & z"), "x < y & z", "text that only looks like markup");
        assert_eq!(plain_text("Why <code>Option</code> matters"), "Why Option matters");
        assert_eq!(plain_text(""), "");
    }

    #[test]
    fn an_excerpt_ends_at_a_word() {
        assert_eq!(excerpt("<p>one two three four</p>", 100), "one two three four");
        assert_eq!(excerpt("<p>one two three four</p>", 9), "one two\u{2026}");
        assert_eq!(excerpt("<p>abcdefghijkl</p>", 5), "abcde\u{2026}", "one long word is cut inside");
    }
}
