//! The lenient loader (`azul_core::xml::html`): every row is markup a
//! browser reads and the tree it builds from it, as an outline of the
//! `<body>`'s children (the whole document for one with `<html>`). The trees
//! are Chrome's (`DOMParser`, text/html), except where a row's note says how
//! the simplified tree construction differs.

use super::*;

/// The node named `tag`, depth first.
fn find_element<'a>(nodes: &'a [XmlNodeChild], tag: &str) -> Option<&'a XmlNode> {
    for node in nodes {
        if let XmlNodeChild::Element(element) = node {
            if element.node_type.as_str() == tag {
                return Some(element);
            }
            if let Some(found) = find_element(element.children.as_ref(), tag) {
                return Some(found);
            }
        }
    }
    None
}

/// The outline of what `html` puts in the `<body>` (a fragment's roots).
fn body_of(html: &str, with_attributes: bool) -> String {
    let nodes = parse_html_nodes(html);
    match find_element(&nodes, "body") {
        Some(body) => outline(body.children.as_ref(), with_attributes),
        None => outline(&nodes, with_attributes),
    }
}

/// The outline of the whole document.
fn document_of(html: &str, with_attributes: bool) -> String {
    outline(&parse_html_nodes(html), with_attributes)
}

fn check(rows: &[(&str, &str)], with_attributes: bool, of: fn(&str, bool) -> String) {
    let mut failures = Vec::new();
    for (input, expected) in rows {
        let got = of(input, with_attributes);
        if got != *expected {
            failures.push(alloc::format!(
                "{input:?}\n  expected {expected}\n  got      {got}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Mail writes `width=150`, `BGCOLOR=#FFFFFF`, `noshade`, `nowrap`: an XML
/// tokenizer stops at the first one (`InvalidQuote`, the exploration's samples 03 and 08).
#[test]
fn unquoted_and_bare_attributes_are_read_like_a_browser_reads_them() {
    check(
        &[
            (
                "<table><tr><td width=150 valign=top>x</td></tr></table>",
                "table{tbody{tr{td[width=150 valign=top]{\"x\"}}}}",
            ),
            // a fragment is a document: the `<meta>` goes into its head
            (
                "<meta http-equiv=Content-Type content=\"text/html; charset=us-ascii\"><font color=#FF0000 size=+1>a</font>",
                "font[color=#FF0000 size=+1]{\"a\"}",
            ),
            (
                "<hr noshade size=1><input disabled checked>",
                "hr[noshade= size=1] input[disabled= checked=]",
            ),
            (
                "<a href=foo/bar.html title='it''s'>l</a>",
                "a[href=foo/bar.html title=it 's'=]{\"l\"}",
            ),
            // a repeated attribute keeps its first value
            (
                "<div id=a id=b class=c CLASS=d>x</div>",
                "div[id=a class=c]{\"x\"}",
            ),
        ],
        true,
        body_of,
    );
}

/// `<TABLE>`, `<BR>`, `ALIGN=RIGHT` (sample 08): HTML names are case-insensitive.
#[test]
fn upper_case_names_are_the_lower_case_elements() {
    check(
        &[
            (
                "<TABLE BORDER=1><TR><TD ALIGN=RIGHT>a</TD></TR></TABLE><P CLASS=X>b",
                "table[border=1]{tbody{tr{td[align=RIGHT]{\"a\"}}}} p[class=X]{\"b\"}",
            ),
            ("<Div><SPAN>a</span></DIV>", "div{span{\"a\"}}"),
        ],
        true,
        body_of,
    );
}

/// Every HTML void element ends where it starts: `<br>` does not swallow what follows.
#[test]
fn void_elements_take_no_content_with_or_without_a_slash() {
    check(
        &[
            (
                "a<BR>b<br>c<br/>d<img src=x.png>e<hr>f<wbr>g<input>h<embed src=e>i<keygen>j<col>k",
                "\"a\" br \"b\" br \"c\" br \"d\" img \"e\" hr \"f\" wbr \"g\" input \"h\" embed \"i\" keygen \"jk\"",
            ),
            // `</br>` is a line break, `</img>` nothing
            (
                "a</br>b<img src=x></img>c",
                "\"a\" br \"b\" img \"c\"",
            ),
        ],
        false,
        body_of,
    );
}

/// `<!-- ---- -->` (`InvalidCommentData` in XML), `--!>`, `<!-->`.
#[test]
fn comments_end_at_their_marker_whatever_they_hold() {
    check(
        &[
            // Chrome keeps the comments as nodes between the texts; azul drops them and the text is one run
            (
                "a<!-- ---- -->b<!-- x -- y -->c<!---->d<!-->e<!--->f<!-- z --!>g",
                "\"abcdefg\"",
            ),
            ("a<!-- never closed", "\"a\""),
        ],
        false,
        body_of,
    );
}

/// A `<` that starts no tag and an `&` that starts no reference are text.
#[test]
fn lt_and_amp_in_text_and_attribute_values_are_characters() {
    check(
        &[
            (
                "<a title=\"a<b & c\" href=\"?x=1&copy=2&amp;y=3\">AT&T a < b & c &foo bar 1<2</a>",
                "a[title=a<b & c href=?x=1&copy=2&y=3]{\"AT&T a < b & c &foo bar 1<2\"}",
            ),
            ("a < b <3 <> <= </ c", "\"a < b <3 <> <= \""),
            // Chrome: `"a" "cd"` (the bogus comment is a node there)
            ("a</ b>c</>d", "\"acd\""),
        ],
        true,
        body_of,
    );
}

/// C0 controls are not XML characters (`NonXmlChar`); a browser keeps them in the DOM
/// where they paint as nothing or a box. CR LF and a lone CR are a line feed.
#[test]
fn stray_control_characters_are_dropped() {
    check(
        &[
            // Chrome keeps the controls; azul drops them
            ("a\u{1}b\u{8}c\u{B}d\u{1F}e\r\nf\rg", "\"abcde f g\""),
        ],
        false,
        body_of,
    );
}

#[test]
fn elements_left_open_at_the_end_are_closed() {
    check(
        &[
            ("<div><b>text<i>more", "div{b{\"text\" i{\"more\"}}}"),
            // a tag cut off by the end is dropped
            ("a<div class=\"x", "\"a\""),
            ("a<div class=x", "\"a\""),
        ],
        false,
        body_of,
    );
}

/// `p`, `li`, `dt` / `dd`, `option`, `tr` / `td` / `th` and the row groups end where the
/// next one (or a block) starts.
#[test]
fn implied_end_tags_close_paragraphs_items_and_cells() {
    check(
        &[
            (
                "<p>a<p>b<div>c</div><p>d<ul><li>e</ul><p>f<h1>g</h1>",
                "p{\"a\"} p{\"b\"} div{\"c\"} p{\"d\"} ul{li{\"e\"}} p{\"f\"} h1{\"g\"}",
            ),
            (
                "<ul><li>a<li>b<ul><li>c<li>d</ul><li>e</ul><ol><li>x<li>y</ol>",
                "ul{li{\"a\"} li{\"b\" ul{li{\"c\"} li{\"d\"}}} li{\"e\"}} ol{li{\"x\"} li{\"y\"}}",
            ),
            (
                "<dl><dt>a<dd>b<dt>c<dd>d</dl>",
                "dl{dt{\"a\"} dd{\"b\"} dt{\"c\"} dd{\"d\"}}",
            ),
            (
                "<table><thead><tr><th>h1<th>h2<tbody><tr><td>a<td>b<tr><td>c<td>d<tfoot><tr><td>f</table>",
                "table{thead{tr{th{\"h1\"} th{\"h2\"}}} tbody{tr{td{\"a\"} td{\"b\"}} tr{td{\"c\"} td{\"d\"}}} tfoot{tr{td{\"f\"}}}}",
            ),
            (
                "<select><option>a<option>b<optgroup label=g><option>c<optgroup label=h><option>d</select>",
                "select{option{\"a\"} option{\"b\"} optgroup{option{\"c\"}} optgroup{option{\"d\"}}}",
            ),
            // the implied tbody and tr
            (
                "<table><tr><td>a</td></tr></table><table><td>b</td></table>",
                "table{tbody{tr{td{\"a\"}}}} table{tbody{tr{td{\"b\"}}}}",
            ),
            (
                "<h1>a<h2>b</h2>c",
                "h1{\"a\"} h2{\"b\"} \"c\"",
            ),
            (
                "<ul><li><div>a<li>b</div></ul>",
                "ul{li{div{\"a\"}} li{\"b\"}}",
            ),
            (
                "<table><tr><td><b>a<td>b</table>c",
                "table{tbody{tr{td{b{\"a\"}} td{\"b\"}}}} \"c\"",
            ),
        ],
        false,
        body_of,
    );
}

/// A stray end tag (sanitizers and editors leave them) is ignored; one that matches an
/// open element closes up to it - but never across a table cell.
#[test]
fn an_end_tag_closes_only_within_its_scope() {
    check(
        &[
            ("<div>a<span>b</div>c", "div{\"a\" span{\"b\"}} \"c\""),
            ("<div>a</font>b</span>c</div>d", "div{\"abc\"} \"d\""),
            (
                "<div><table><tr><td>a</div>b</td></tr></table>c</div>e",
                "div{table{tbody{tr{td{\"ab\"}}}} \"c\"} \"e\"",
            ),
            (
                "<table><tr><td><table><tr><td>a</td></tr></table></td></tr></table>",
                "table{tbody{tr{td{table{tbody{tr{td{\"a\"}}}}}}}}",
            ),
            // `</p>` without a `<p>` is an empty paragraph
            ("a</p>b", "\"a\" p \"b\""),
        ],
        false,
        body_of,
    );
}

/// The HTML Standard's adoption agency algorithm (13.2.6.4.7, "in body", an end tag of a
/// formatting element): `</b>` with a block open inside the `<b>` ends the `<b>` where it
/// stands, moves the block out of it and clones the `<b>` into the block - Outlook's
/// `<font><div>a</font>b</div>` keeps "b" out of the font, as every browser does.
#[test]
fn a_misnested_formatting_element_is_adopted_as_a_browser_adopts_it() {
    check(
        &[
            ("<b>x<p>y</b>z</p>w", "b{\"x\"} p{b{\"y\"} \"z\"} \"w\""),
            ("<b><i>x</b>y</i>", "b{i{\"x\"}} i{\"y\"}"),
            ("<a>1<p>2</a>3</p>", "a{\"1\"} p{a{\"2\"} \"3\"}"),
            ("<a>1<button>2</a>3</button>", "a{\"1\"} button{a{\"2\"} \"3\"}"),
            ("<a>1<b>2</a>3</b>", "a{\"1\" b{\"2\"}} b{\"3\"}"),
            (
                "<a>1<div>2<div>3</a>4</div>5</div>",
                "a{\"1\"} div{a{\"2\"} div{a{\"3\"} \"4\"} \"5\"}",
            ),
            (
                "<p><b><i>a<div>b</b>c</div>",
                "p{b{i{\"a\"}}} div{b{i{\"b\"}} i{\"c\"}}",
            ),
            (
                "<div><b>1<div>2<div>3</b>4</div></div></div>",
                "div{b{\"1\"} div{b{\"2\"} div{b{\"3\"} \"4\"}}}",
            ),
            // the inner loop gives up after three elements that are not formatting ones
            (
                "<b><em><foo><foo><foo><foo><foo><aside></b></em>",
                "b{em{foo{foo{foo{foo{foo}}}}}} aside{b}",
            ),
            (
                "<b>1<i>2<p>3</b>4</i>5</p>",
                "b{\"1\" i{\"2\"}} i p{i{b{\"3\"} \"4\"} \"5\"}",
            ),
            (
                "<em>a<strong>b</em>c</strong>d",
                "em{\"a\" strong{\"b\"}} strong{\"c\"} \"d\"",
            ),
            (
                "<u>a<s>b<p>c</u>d</s>e",
                "u{\"a\" s{\"b\"}} s p{s{u{\"c\"} \"d\"} \"e\"}",
            ),
        ],
        false,
        body_of,
    );
    // the clone keeps the original's attributes
    check(
        &[
            (
                "<font face=Arial><div>a</font>b</div>c",
                "font[face=Arial] div{font[face=Arial]{\"a\"} \"b\"} \"c\"",
            ),
            (
                "<font face=Arial><p>a<p>b</font>c",
                "font[face=Arial]{p{\"a\"}} p{font[face=Arial]{\"b\"} \"c\"}",
            ),
            (
                "<a href=1>a<div>b<a href=2>c</a></div>",
                "a[href=1]{\"a\"} div{a[href=1]{\"b\"} a[href=2]{\"c\"}}",
            ),
        ],
        true,
        body_of,
    );
}

/// `<p><b>x<p>y`: the bold goes on in the next paragraph (the HTML Standard's
/// "reconstruct the active formatting elements"); a cell starts without it.
#[test]
fn formatting_elements_continue_in_the_next_block() {
    check(
        &[
            ("<p>a<b>b<p>c</b>d", "p{\"a\" b{\"b\"}} p{b{\"c\"} \"d\"}"),
            // at most three equal ones are reopened
            (
                "<font>a<font>b<font>c<font>d<p>e",
                "font{\"a\" font{\"b\" font{\"c\" font{\"d\" p{\"e\"}}}}}",
            ),
            // a link does not nest in a link
            ("<a href=1>a<a href=2>b</a>c", "a{\"a\"} a{\"b\"} \"c\""),
            (
                "<b><table><tr><td>x</td></tr></table>y",
                "b{table{tbody{tr{td{\"x\"}}}} \"y\"}",
            ),
        ],
        false,
        body_of,
    );
}

#[test]
fn doctype_raw_text_and_rcdata_are_read_as_html() {
    check(
        &[
            (
                "<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\"><html><head><title>t</title></head><body>x</body></html>",
                "\"x\"",
            ),
            // one line feed after `<pre>` is dropped
            (
                "<pre>\nline1\nline2</pre><pre>\n\nx</pre>",
                "pre{\"line1 line2\"} pre{\" x\"}",
            ),
            (
                "<textarea>\n<b>x</b> &lt;</textarea>",
                "textarea{\"<b>x</b> <\"}",
            ),
            // Chrome: `"a" "b"` (a comment node between)
            (
                "a<![CDATA[x<y]]>b",
                "\"ab\"",
            ),
        ],
        false,
        body_of,
    );
}

/// Word's `<![if !supportLists]>`, Outlook's conditional comments and `<o:p>`.
#[test]
fn word_and_outlook_markup_parses() {
    check(
        &[
            (
                "<p class=MsoListParagraph><![if !supportLists]><span>1.<span>&nbsp;&nbsp;</span></span><![endif]>Item<o:p></o:p></p><!--[if gte mso 9]><xml><o:shapedefaults v:ext=\"edit\" spidmax=\"1026\" /></xml><![endif]-->",
                "p{span{\"1.\" span{\"\u{A0}\u{A0}\"}} \"Item\" o:p}",
            ),
            (
                "<p class=MsoNormal><span lang=DE>Hi<o:p></o:p></span></p><p class=MsoNormal><o:p>&nbsp;</o:p></p>",
                "p{span{\"Hi\" o:p}} p{o:p{\"\u{A0}\"}}",
            ),
        ],
        false,
        body_of,
    );
}

#[test]
fn svg_elements_close_themselves() {
    check(
        &[
            // `<div/>` is an open div in HTML
            (
                "<svg width=10><path d=\"M0 0\"/><rect/></svg><div/>after",
                "svg{path rect} div{\"after\"}",
            ),
        ],
        false,
        body_of,
    );
}

/// The implied `<head>` / `<body>` (13.2.6.4.1 - 13.2.6.4.6): every document has both; the
/// head's elements before the body go into the head (also after `</head>`), content into the
/// body, white space before the document is nothing, a second `<body>` / `<html>` only adds the
/// attributes the first one lacks, and `</body>` / `</html>` change nothing.
#[test]
fn a_document_gets_its_head_and_body_where_a_browser_puts_them() {
    check(
        &[
            (
                "<html><style>p{}</style><p>x</p></html>",
                "html{head{style{\"p{}\"}} body{p{\"x\"}}}",
            ),
            (
                "<html><head><title>t</title><div>body?</div></head><body class=b>x</body></html>",
                "html{head{title{\"t\"}} body[class=b]{div{\"body?\"} \"x\"}}",
            ),
            (
                "<html><body>a</body></html>b<p>c",
                "html{head body{\"ab\" p{\"c\"}}}",
            ),
            (
                "<body class=a>x<body class=b id=c>y",
                "html{head body[class=a id=c]{\"xy\"}}",
            ),
            ("x", "html{head body{\"x\"}}"),
            ("  \n x", "html{head body{\"x\"}}"),
            ("<p>x<html lang=en>", "html[lang=en]{head body{p{\"x\"}}}"),
            (
                "<html><head><style>a{}</style> x</head>",
                "html{head{style{\"a{}\"}} body{\"x\"}}",
            ),
            (
                "<html><head></head><style>p{}</style><body>x",
                "html{head{style{\"p{}\"}} body{\"x\"}}",
            ),
            (
                "<head></head><meta charset=utf-8><p>x",
                "html{head{meta[charset=utf-8]} body{p{\"x\"}}}",
            ),
            // `<noscript>` with scripting off: in the head, what does not belong there ends it
            (
                "<head><noscript><style>a{}</style></noscript></head><p>x",
                "html{head{noscript{style{\"a{}\"}}} body{p{\"x\"}}}",
            ),
            (
                "<head><noscript><p>x</p></noscript>",
                "html{head{noscript} body{p{\"x\"}}}",
            ),
            (
                "<noscript><b>x</b></noscript>y",
                "html{head{noscript} body{b{\"x\"} \"y\"}}",
            ),
            (
                "<noframes><b>x</b></noframes>y",
                "html{head{noframes{\"<b>x</b>\"}} body{\"y\"}}",
            ),
            ("<title>a<b>&amp;</title>", "html{head{title{\"a<b>&\"}} body}"),
            ("<style>a</style b>c", "html{head{style{\"a\"}} body{\"c\"}}"),
            ("<div>a</body>b</html>c", "html{head body{div{\"abc\"}}}"),
            (
                "<html><head><style>a > b { color: red } p:before { content: \"</p>\" }</style><script>if (a < b && c) { x = \"<b>\"; }</script></head><body>y</body></html>",
                "html{head{style{\"a > b { color: red } p:before { content: \\\"</p>\\\" }\"} script{\"if (a < b && c) { x = \\\"<b>\\\"; }\"}} body{\"y\"}}",
            ),
        ],
        true,
        document_of,
    );
}

/// The HTML Standard's named references (all 2231), the legacy ones without
/// their `;`, and the numeric ones through the Windows-1252 repair.
#[test]
fn character_references_decode_as_in_a_browser() {
    let text = |s: &str| decode_character_references(s, CharRefMode::HtmlText).into_owned();
    assert_eq!(text("&check; &starf;"), "\u{2713} \u{2605}");
    assert_eq!(text("&NotEqualTilde;"), "\u{2242}\u{338}", "two characters");
    assert_eq!(
        text("&copy 2026"),
        "\u{A9} 2026",
        "a legacy name without its ;"
    );
    assert_eq!(text("&notin; &notit;"), "\u{2209} \u{AC}it;");
    assert_eq!(
        text("&#150; &#x80; &#0; &#xD800; &#1114112;"),
        "\u{2013} \u{20AC} \u{FFFD} \u{FFFD} \u{FFFD}"
    );
    assert_eq!(
        text("&amp &lt; &Eacute; &eacute &nbsp;"),
        "& < \u{C9} \u{E9} \u{A0}"
    );
    assert_eq!(text("&foo; &amp;lt; & &; &#;"), "&foo; &lt; & &; &#;");
    let attribute =
        |s: &str| decode_character_references(s, CharRefMode::HtmlAttribute).into_owned();
    assert_eq!(attribute("?a=1&copy=2&notit;x&amp"), "?a=1&copy=2&notit;x&");
    assert_eq!(attribute("&copy &copy; &copyx"), "\u{A9} \u{A9} &copyx");
    // XML's rules (the strict loaders): a `;` is required, a number that is not
    // a scalar value stays as written, the names are HTML's.
    let xml = |s: &str| decode_character_references(s, CharRefMode::Xml).into_owned();
    assert_eq!(
        xml("&copy 2026 &copy; &#0; &#xD800; &check;"),
        "&copy 2026 \u{A9} \u{0} &#xD800; \u{2713}"
    );
    assert!(matches!(
        decode_character_references("plain", CharRefMode::Xml),
        Cow::Borrowed(_)
    ));
    assert_eq!(named_character_reference("copy", true), Some("\u{A9}"));
    assert_eq!(named_character_reference("copy", false), Some("\u{A9}"));
    assert_eq!(
        named_character_reference("check", false),
        None,
        "not a legacy name"
    );
    assert_eq!(
        named_character_reference("CounterClockwiseContourIntegral", true),
        Some("\u{2233}")
    );
}

/// A browser stops nesting at 512 elements; deeper ones become siblings
/// (and a thousand unclosed `<div>`s do not overflow the stack of the
/// recursive `XmlNode` drop).
#[test]
fn elements_deeper_than_the_limit_become_siblings() {
    let html = "<div>".repeat(2000) + "x";
    let nodes = parse_html_nodes(&html);
    let mut depth = 0;
    let mut cursor: &[XmlNodeChild] = &nodes;
    while let Some(XmlNodeChild::Element(e)) = cursor.first() {
        depth += 1;
        cursor = e.children.as_ref();
    }
    assert!(depth <= MAX_XML_NESTING_DEPTH + 1, "nested {depth} deep");
    drop(nodes);
}

// ============================================================================
// The one encoder (DEDUP_WIDGETS_API F5): the inverse of
// `decode_character_references`, for every writer of HTML / XML text.
// ============================================================================

/// Text content needs `&`, `<` and `>` escaped - nothing else: quotes are
/// plain characters between tags.
#[test]
fn text_encoding_escapes_ampersand_and_angle_brackets_and_keeps_quotes() {
    assert_eq!(
        encode_text("a < b & c > d \"e\" 'f'"),
        "a &lt; b &amp; c &gt; d \"e\" 'f'"
    );
    assert_eq!(
        encode_text("&amp;"),
        "&amp;amp;",
        "an existing reference is text too"
    );
    assert_eq!(
        encode_text("<script>a && b</script>"),
        "&lt;script&gt;a &amp;&amp; b&lt;/script&gt;"
    );
}

/// An attribute value also needs both quotes escaped, so it is safe inside
/// `"..."` and `'...'` alike (and `<` / `>` for the XML loaders' sake).
#[test]
fn attribute_encoding_also_escapes_both_quotes() {
    assert_eq!(
        encode_attribute("x=\"1\" & y='2' <z>"),
        "x=&quot;1&quot; &amp; y=&apos;2&apos; &lt;z&gt;"
    );
    assert_eq!(
        encode_attribute("https://example.org/?a=1&b=2"),
        "https://example.org/?a=1&amp;b=2"
    );
}

/// XML 1.0 has no way to write the C0 controls other than tab, line feed
/// and carriage return, nor U+FFFE / U+FFFF - not even as a reference - so
/// the encoder leaves them out instead of writing a document a strict
/// loader (or Windows' toast XML) rejects. DEL and the C1 controls are
/// legal XML and stay.
#[test]
fn encoding_drops_the_characters_xml_cannot_carry_and_keeps_tab_and_line_breaks() {
    let s = "a\u{0}b\u{8}c\td\ne\rf\u{b}g\u{c}h\u{1f}i\u{fffe}j\u{ffff}k\u{7f}l\u{85}m";
    let kept = "abc\td\ne\rfghijk\u{7f}l\u{85}m";
    assert_eq!(encode_text(s), kept);
    assert_eq!(encode_attribute(s), kept);
}

/// Plain text comes back unchanged.
#[test]
fn encoding_plain_text_changes_nothing() {
    for s in [
        "",
        "plain",
        "\u{1F642} caf\u{e9} \u{65e5}\u{672c}",
        "tab\tand\nlines",
    ] {
        assert_eq!(encode_text(s), s);
        assert_eq!(encode_attribute(s), s);
    }
}

/// What the encoder writes, the decoder reads back as the original - in
/// every mode, so a writer never has to know which loader reads its output.
#[test]
fn an_encoded_string_decodes_back_to_itself_in_every_mode() {
    for s in [
        "",
        "plain",
        "a<b>&c\"d'e",
        "&amp;lt;",
        "&copy 2026 &copy; &#169; &#xA9;",
        "\u{1F642}&\u{1F642}",
        "?a=1&copy=2&lt=3",
        "&",
        "&;",
        "&#",
    ] {
        let text = encode_text(s);
        let attribute = encode_attribute(s);
        assert_eq!(
            decode_character_references(&text, CharRefMode::Xml),
            s,
            "{s:?} as XML text"
        );
        assert_eq!(
            decode_character_references(&text, CharRefMode::HtmlText),
            s,
            "{s:?} as HTML text"
        );
        assert_eq!(
            decode_character_references(&attribute, CharRefMode::HtmlAttribute),
            s,
            "{s:?} as an HTML attribute"
        );
        assert_eq!(
            decode_character_references(&attribute, CharRefMode::Xml),
            s,
            "{s:?} as an XML attribute"
        );
    }
}
