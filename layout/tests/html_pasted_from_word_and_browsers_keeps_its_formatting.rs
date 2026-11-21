//! HTML pasted from Word, Outlook, LibreOffice and web pages keeps its bold,
//! italic, links and lists.
//!
//! The paste parser (`azul_layout::paste_html::sanitize_html`) read the
//! clipboard's HTML flavour with the STRICT XML loader. Word's flavour has
//! unquoted attributes (`<p class=MsoNormal>`), so it did not parse at all and
//! the paste fell back to the plain text - every format and link lost; a web
//! page's `<p>` left open did the same (`UnclosedRootNode`); LibreOffice's
//! upper-case `<BR>` was not void, so the line after it vanished. The paste
//! parser now reads the lenient loader's tree (`parse_html_string`, the tree a
//! browser builds).
//!
//! Word writes a list as paragraphs (`<p style='mso-list:l0 level1 lfo1'>`)
//! whose marker sits in a `<span style='mso-list:Ignore'>` (inside
//! `<![if !supportLists]>`): the paste makes them a list again - ordered when
//! the marker is a number or a letter (`1.`, `a)`), else unordered - and drops
//! the marker.
//!
//! Not compiled by the author (house rule); RED before the fix.

use azul_layout::paste_html::{
    sanitize_html, PastedBlockKind as K, PastedFormats, PastedNode as N,
};

fn text(s: &str) -> N {
    N::Text(s.to_string(), PastedFormats::default())
}

fn bold(s: &str) -> N {
    N::Text(
        s.to_string(),
        PastedFormats {
            bold: true,
            ..PastedFormats::default()
        },
    )
}

fn italic(s: &str) -> N {
    N::Text(
        s.to_string(),
        PastedFormats {
            italic: true,
            ..PastedFormats::default()
        },
    )
}

fn block(kind: K, children: Vec<N>) -> N {
    N::Block(kind, children)
}

fn pasted(html: &str) -> Vec<N> {
    sanitize_html(html)
        .unwrap_or_else(|| panic!("the paste does not parse: {html}"))
        .nodes
}

/// Word's clipboard flavour (Word 16, Windows), shortened.
const WORD: &str = "<html xmlns:o=\"urn:schemas-microsoft-com:office:office\" \
xmlns:w=\"urn:schemas-microsoft-com:office:word\" xmlns=\"http://www.w3.org/TR/REC-html40\">\n\
<head><meta http-equiv=Content-Type content=\"text/html; charset=utf-8\">\
<meta name=ProgId content=Word.Document>\n\
<style><!-- p.MsoNormal {margin:0cm;} --></style></head>\n\
<body lang=DE style='tab-interval:35.4pt'>\n\
<!--StartFragment-->\n\
<p class=MsoNormal>Hello <b>bold</b> and <i>italic</i> and \
<a href=\"https://example.org/\">a link</a><o:p></o:p></p>\n\
<p class=MsoListParagraphCxSpFirst style='text-indent:-18.0pt;mso-list:l0 level1 lfo1'>\
<![if !supportLists]><span style='font-family:Symbol;mso-fareast-font-family:Symbol'>\
<span style='mso-list:Ignore'>\u{B7}<span style='font:7.0pt \"Times New Roman\"'>\
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; </span></span></span><![endif]>\
First item<o:p></o:p></p>\n\
<p class=MsoListParagraphCxSpLast style='text-indent:-18.0pt;mso-list:l0 level1 lfo1'>\
<![if !supportLists]><span style='font-family:Symbol'><span style='mso-list:Ignore'>\u{B7}\
<span style='font:7.0pt \"Times New Roman\"'>&nbsp;&nbsp;&nbsp;&nbsp; </span></span></span>\
<![endif]>Second <b>item</b><o:p></o:p></p>\n\
<p class=MsoListParagraph style='text-indent:-18.0pt;mso-list:l1 level1 lfo2'>\
<![if !supportLists]><span><span style='mso-list:Ignore'>1.<span style='font:7.0pt \
\"Times New Roman\"'>&nbsp;&nbsp;&nbsp; </span></span></span><![endif]>Numbered<o:p></o:p></p>\n\
<!--EndFragment-->\n\
</body>\n\
</html>";

#[test]
fn a_word_paste_keeps_its_formats_its_link_and_its_lists() {
    assert_eq!(
        pasted(WORD),
        vec![
            block(
                K::Paragraph,
                vec![
                    text("Hello "),
                    bold("bold"),
                    text(" and "),
                    italic("italic"),
                    text(" and "),
                    N::Link("https://example.org/".to_string(), vec![text("a link")]),
                ],
            ),
            block(
                K::UnorderedList,
                vec![
                    block(K::ListItem, vec![text("First item")]),
                    block(K::ListItem, vec![text("Second "), bold("item")]),
                ],
            ),
            block(
                K::OrderedList,
                vec![block(K::ListItem, vec![text("Numbered")])],
            ),
        ]
    );
}

#[test]
fn a_web_page_with_open_paragraphs_and_bare_attributes_pastes_its_structure() {
    let html = "<p>One <b>bold</b><p>Two <a href=https://example.org/x>link</a><br>next\
                <ul><li>a<li>b</ul>";
    assert_eq!(
        pasted(html),
        vec![
            block(K::Paragraph, vec![text("One "), bold("bold")]),
            block(
                K::Paragraph,
                vec![
                    text("Two "),
                    N::Link("https://example.org/x".to_string(), vec![text("link")]),
                    N::Break,
                    text("next"),
                ],
            ),
            block(
                K::UnorderedList,
                vec![
                    block(K::ListItem, vec![text("a")]),
                    block(K::ListItem, vec![text("b")]),
                ],
            ),
        ]
    );
}

#[test]
fn an_upper_case_line_break_keeps_the_line_after_it() {
    // LibreOffice writes upper-case tags.
    assert_eq!(
        pasted("<P STYLE=\"margin-bottom: 0cm\"><B>Bold</B> text<BR>line</P>"),
        vec![block(
            K::Paragraph,
            vec![bold("Bold"), text(" text"), N::Break, text("line")],
        )]
    );
}

#[test]
fn an_outlook_o_p_is_content_not_a_paragraph() {
    // `<o:p>` is an inline element of Office's vocabulary: its text belongs to
    // the paragraph it is in.
    assert_eq!(
        pasted("<p class=MsoNormal>Hi<o:p>!</o:p></p>"),
        vec![block(K::Paragraph, vec![text("Hi"), text("!")])]
    );
}
