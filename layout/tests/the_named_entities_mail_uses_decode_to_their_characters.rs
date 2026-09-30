//! The named entities mail uses decode to their characters.
//!
//! AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, 1.2
//! and 1.3 sample 01, gap E-XML-4): the XML loaders decoded six named
//! entities (`lt gt amp apos quot nbsp`) and left every other one literal,
//! so a newsletter footer read "&copy; 2026" and a button "Read more &rarr;".
//! HTML mail writes the HTML 4 set freely: typography (`&mdash; &hellip;
//! &ldquo;`), currency (`&euro; &pound;`), and the accented Latin-1 letters
//! (`caf&eacute;`, `&Uuml;bersicht`).
//!
//! Both loaders are checked: the tree loader AzMail uses (`parse_xml` +
//! `dom_from_parsed_xml`) and the fast path (`parse_xml_to_styled_dom`).
//! An unknown name stays literal, as before.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::dom::{Dom, NodeType};

const MAIL: &str = "<html><head></head><body><div>\
<p>&copy; 2026 Caf&eacute; M&uuml;ller &mdash; &ldquo;Hi&rdquo; &lsquo;x&rsquo; &hellip;</p>\
<p>&euro;5 &pound;4 &yen;3 2&times;3 6&divide;2 &laquo;a&raquo; &bull; &middot; &ndash; &reg; \
&trade; &rarr; &szlig; &Auml;&Ouml;&Uuml; &ntilde;&ccedil;&agrave;</p>\
<p>&notanentity; &amp;amp;</p>\
</div></body></html>";

/// Every character the markup's named entities stand for.
const DECODED: &[(&str, char)] = &[
    ("copy", '\u{A9}'),
    ("eacute", '\u{E9}'),
    ("uuml", '\u{FC}'),
    ("mdash", '\u{2014}'),
    ("ldquo", '\u{201C}'),
    ("rdquo", '\u{201D}'),
    ("lsquo", '\u{2018}'),
    ("rsquo", '\u{2019}'),
    ("hellip", '\u{2026}'),
    ("euro", '\u{20AC}'),
    ("pound", '\u{A3}'),
    ("yen", '\u{A5}'),
    ("times", '\u{D7}'),
    ("divide", '\u{F7}'),
    ("laquo", '\u{AB}'),
    ("raquo", '\u{BB}'),
    ("bull", '\u{2022}'),
    ("middot", '\u{B7}'),
    ("ndash", '\u{2013}'),
    ("reg", '\u{AE}'),
    ("trade", '\u{2122}'),
    ("rarr", '\u{2192}'),
    ("szlig", '\u{DF}'),
    ("Auml", '\u{C4}'),
    ("Ouml", '\u{D6}'),
    ("Uuml", '\u{DC}'),
    ("ntilde", '\u{F1}'),
    ("ccedil", '\u{E7}'),
    ("agrave", '\u{E0}'),
];

fn texts_of_dom(dom: &Dom, out: &mut String) {
    if let NodeType::Text(t) = &dom.root.node_type {
        out.push_str(t.as_str());
        out.push('\n');
    }
    for child in dom.children.as_ref() {
        texts_of_dom(child, out);
    }
}

fn assert_decoded(text: &str, which: &str) {
    for (name, ch) in DECODED {
        assert!(
            text.contains(*ch),
            "{which}: &{name}; decodes to U+{:04X}; the text is {text:?}",
            *ch as u32
        );
        assert!(
            !text.contains(&format!("&{name};")),
            "{which}: &{name}; is left literal in {text:?}"
        );
    }
    // Unknown names stay as written; `&amp;amp;` is the text "&amp;" (one
    // pass, no double decoding).
    assert!(text.contains("&notanentity;"), "{which}: {text:?}");
    assert!(text.contains("&amp;"), "{which}: &amp;amp; is the text &amp;: {text:?}");
}

#[test]
fn the_tree_loader_decodes_the_named_entities_mail_uses() {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let mut text = String::new();
    texts_of_dom(&dom, &mut text);
    assert_decoded(&text, "tree loader");
}

#[test]
fn the_fast_loader_decodes_the_named_entities_mail_uses() {
    let styled = azul_layout::xml::parse_xml_to_styled_dom(MAIL).expect("the mail parses");
    let mut text = String::new();
    for nd in styled.node_data.as_ref() {
        if let NodeType::Text(t) = &nd.node_type {
            text.push_str(t.as_str());
            text.push('\n');
        }
    }
    assert_decoded(&text, "fast loader");
}
