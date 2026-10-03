//! The tree construction's rules as DATA (`azul_core::xml::html`).
//!
//! Every special case of the HTML Living Standard's tree construction (13.2.6) is a row of
//! one of the tables below, with the section it comes from; the algorithms that run them
//! (close a p element, the adoption agency, foster parenting, ...) are in `xml_html_tree.rs`.
//! A new rule is a new row, not a new code path:
//!
//! - [`ELEMENTS`]: what each element IS (void, formatting, special, a scope boundary, ...) and
//!   how its content is tokenized;
//! - [`MODES`]: which elements decide the insertion mode (13.2.4.1);
//! - [`START_TAGS`] / [`END_TAGS`]: what a start / end tag does in the body (13.2.6.4.7) - and,
//!   with the HTML-only steps off, the XML conveniences of the strict loaders (one table, so
//!   the strict loaders' implied end tags are the same rows);
//! - [`TABLE_START_TAGS`] / [`TABLE_END_TAGS`]: the table insertion modes (13.2.6.4.9 -
//!   13.2.6.4.15);
//! - [`QUIRKS_PUBLIC_ID_PREFIXES`] & co: the doctypes of quirks mode (13.2.6.4.1).

// ============================================================================
// The elements
// ============================================================================

/// No content and no end tag (13.1.2 "void elements", and the legacy ones the parser
/// inserts and pops at once: `basefont bgsound frame keygen param`).
pub(super) const VOID: u32 = 1 << 0;
/// The formatting elements (13.2.4.3): reopened in the next block, adopted when misnested.
pub(super) const FORMATTING: u32 = 1 << 1;
/// The "special" category (13.2.4.2): a block an end tag does not reach past.
pub(super) const SPECIAL: u32 = 1 << 2;
/// `h1` - `h6`.
pub(super) const HEADING: u32 = 1 << 3;
/// The head's content (13.2.6.4.4 "in head"): before the body it goes into the head, in
/// the body it is inserted where it stands.
pub(super) const HEAD: u32 = 1 << 4;
/// Puts a marker on the list of active formatting elements (13.2.4.3): what was opened
/// outside is not reopened inside.
pub(super) const MARKER: u32 = 1 << 5;
/// A boundary of "has an element in scope" (13.2.4.2), the default scope (the `MathML` and
/// SVG text integration points other than `foreignObject` are left out: the strict loaders'
/// scope).
pub(super) const SCOPE: u32 = 1 << 6;
/// A boundary of "has an element in table scope".
pub(super) const TABLE_SCOPE: u32 = 1 << 7;
/// Closed by "generate implied end tags" (13.2.6.3).
pub(super) const IMPLIED_END: u32 = 1 << 8;
/// Content misplaced in it is foster-parented (13.2.6.1): `table tbody tfoot thead tr`.
pub(super) const FOSTER_TARGET: u32 = 1 << 9;
/// Ends SVG / `MathML` content (13.2.6.5, the start tags that break out of foreign content;
/// `font` only with `color`, `face` or `size`).
pub(super) const BREAKOUT: u32 = 1 << 10;
/// An HTML / `MathML` text integration point when it is an SVG / `MathML` element (13.2.6):
/// its start tags and text are HTML again.
pub(super) const INTEGRATION_POINT: u32 = 1 << 11;

/// How an element's content is tokenized (13.2.6.2, the generic raw text and RCDATA element
/// parsing algorithms; 13.2.6.4.7 "plaintext").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContentModel {
    /// Markup.
    Markup,
    /// Text with its character references decoded, up to the element's end tag.
    RcData,
    /// Text as written, up to the element's end tag (script data's escapes are not modelled).
    RawText,
    /// Text as written, to the end of the input.
    PlainText,
}

/// One element: its name, its categories, its content.
pub(super) struct Element {
    pub(super) name: &'static str,
    pub(super) flags: u32,
    pub(super) content: ContentModel,
}

const fn el(name: &'static str, flags: u32) -> Element {
    Element {
        name,
        flags,
        content: ContentModel::Markup,
    }
}

const fn text_el(name: &'static str, flags: u32, content: ContentModel) -> Element {
    Element {
        name,
        flags,
        content,
    }
}

const F: u32 = FORMATTING | BREAKOUT;
const S: u32 = SPECIAL;
const SB: u32 = SPECIAL | BREAKOUT;
const SV: u32 = SPECIAL | VOID;
const SH: u32 = SPECIAL | HEADING | BREAKOUT;
const SIP: u32 = SPECIAL | INTEGRATION_POINT;
const SMS: u32 = SPECIAL | MARKER | SCOPE;
const SFT: u32 = SPECIAL | FOSTER_TARGET;

/// Every element a rule names, sorted by name (byte order: binary-searched; a test keeps
/// it sorted). An element not listed has no category and markup content.
pub(super) static ELEMENTS: &[Element] = &[
    el("a", FORMATTING),
    el("address", S),
    el("annotation-xml", S),
    el("applet", SMS),
    el("area", SV),
    el("article", S),
    el("aside", S),
    el("b", F),
    el("base", SV | HEAD),
    el("basefont", SV | HEAD),
    el("bgsound", SV | HEAD),
    el("big", F),
    el("blockquote", SB),
    el("body", SB),
    el("br", SV | BREAKOUT),
    el("button", S),
    el("caption", SMS),
    el("center", SB),
    el("code", F),
    el("col", SV),
    el("colgroup", S),
    el("dd", SB | IMPLIED_END),
    el("desc", SIP),
    el("details", S),
    el("dir", S),
    el("div", SB),
    el("dl", SB),
    el("dt", SB | IMPLIED_END),
    el("em", F),
    el("embed", SV | BREAKOUT),
    el("fieldset", S),
    el("figcaption", S),
    el("figure", S),
    // a breakout only with `color`, `face` or `size` (the tree construction asks)
    el("font", FORMATTING),
    el("footer", S),
    el("foreignobject", SIP | SCOPE),
    el("form", S),
    el("frame", SV),
    el("frameset", S),
    el("h1", SH),
    el("h2", SH),
    el("h3", SH),
    el("h4", SH),
    el("h5", SH),
    el("h6", SH),
    el("head", SB),
    el("header", S),
    el("hgroup", S),
    el("hr", SV | BREAKOUT),
    el("html", S | SCOPE | TABLE_SCOPE),
    el("i", F),
    text_el("iframe", S, ContentModel::RawText),
    el("img", SV | BREAKOUT),
    el("input", SV),
    el("keygen", SV),
    el("li", SB | IMPLIED_END),
    el("link", SV | HEAD),
    el("listing", SB),
    el("main", S),
    el("marquee", SMS),
    el("menu", SB),
    el("meta", SV | HEAD | BREAKOUT),
    el("mi", SIP),
    el("mn", SIP),
    el("mo", SIP),
    el("ms", SIP),
    el("mtext", SIP),
    el("nav", S),
    el("nobr", F),
    text_el("noembed", S, ContentModel::RawText),
    text_el("noframes", S | HEAD, ContentModel::RawText),
    // scripting is off (as in DOMParser): its content is markup
    el("noscript", S),
    el("object", SMS),
    el("ol", SB),
    el("optgroup", IMPLIED_END),
    el("option", IMPLIED_END),
    el("p", SB | IMPLIED_END),
    el("param", SV),
    text_el("plaintext", S, ContentModel::PlainText),
    el("pre", SB),
    el("rb", IMPLIED_END),
    el("rp", IMPLIED_END),
    el("rt", IMPLIED_END),
    el("rtc", IMPLIED_END),
    el("ruby", BREAKOUT),
    el("s", F),
    text_el("script", S | HEAD, ContentModel::RawText),
    el("search", S),
    el("section", S),
    el("select", S),
    el("small", F),
    el("source", SV),
    el("span", BREAKOUT),
    el("strike", F),
    el("strong", F),
    text_el("style", S | HEAD, ContentModel::RawText),
    el("sub", BREAKOUT),
    el("summary", S),
    el("sup", BREAKOUT),
    el("table", SFT | SCOPE | TABLE_SCOPE | BREAKOUT),
    el("tbody", SFT),
    el("td", SMS),
    el("template", SMS | TABLE_SCOPE | HEAD),
    text_el("textarea", S, ContentModel::RcData),
    el("tfoot", SFT),
    el("th", SMS),
    el("thead", SFT),
    text_el("title", SIP | HEAD, ContentModel::RcData),
    el("tr", SFT),
    el("track", SV),
    el("tt", F),
    el("u", F),
    el("ul", SB),
    el("var", BREAKOUT),
    el("wbr", SV),
    text_el("xmp", S, ContentModel::RawText),
];

/// The row of `name` (lower-case), if a rule names it.
pub(super) fn element(name: &str) -> Option<&'static Element> {
    ELEMENTS
        .binary_search_by(|e| e.name.cmp(name))
        .ok()
        .map(|i| &ELEMENTS[i])
}

/// The categories of `name` (lower-case): an OR of the flags above, 0 for an element no
/// rule names.
pub(super) fn flags(name: &str) -> u32 {
    element(name).map_or(0, |e| e.flags)
}

/// Whether `name` (lower-case) has every flag of `flag`.
pub(super) fn is(name: &str, flag: u32) -> bool {
    flags(name) & flag == flag
}

/// How the content of `name` (lower-case) is tokenized.
pub(super) fn content(name: &str) -> ContentModel {
    element(name).map_or(ContentModel::Markup, |e| e.content)
}

/// The start tags a browser reads as another element (13.2.6.4.7: "A start tag whose tag
/// name is "image": change the token's tag name to "img" and reprocess it"). HTML content
/// only - an SVG `<image>` stays one.
pub(super) static ALIASES: &[(&str, &str)] = &[("image", "img")];

/// The HTML void elements (the HTML Standard's, and the legacy ones its
/// parser treats so): no content, no end tag.
#[must_use]
pub fn is_void_element(tag: &str) -> bool {
    is(tag, VOID)
}

// ============================================================================
// The insertion modes
// ============================================================================

/// The insertion modes of the body (13.2.4.1, 13.2.6.4.7 - 13.2.6.4.15). Before the body
/// (initial ... after head) the tree construction keeps a phase of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// 13.2.6.4.7 "in body" (also inside a `select`: "in select" is not modelled).
    Body,
    /// 13.2.6.4.9 "in table".
    Table,
    /// 13.2.6.4.13 "in table body".
    TableBody,
    /// 13.2.6.4.14 "in row".
    Row,
    /// 13.2.6.4.15 "in cell".
    Cell,
    /// 13.2.6.4.11 "in caption".
    Caption,
    /// 13.2.6.4.12 "in column group".
    ColumnGroup,
    /// 13.2.6.4.18 "in template": the body's rules, and table parts without a table
    /// ([`TEMPLATE_CONTENT`]).
    Template,
}

/// 13.2.4.1 "reset the insertion mode appropriately": the open HTML element nearest to the
/// current node that is listed here decides the mode.
pub(super) static MODES: &[(&str, Mode)] = &[
    ("body", Mode::Body),
    ("caption", Mode::Caption),
    ("colgroup", Mode::ColumnGroup),
    ("html", Mode::Body),
    ("table", Mode::Table),
    ("tbody", Mode::TableBody),
    ("td", Mode::Cell),
    ("template", Mode::Template),
    ("tfoot", Mode::TableBody),
    ("th", Mode::Cell),
    ("thead", Mode::TableBody),
    ("tr", Mode::Row),
];

/// The mode an open `name` (lower-case, an HTML element) puts the body in.
pub(super) fn mode_of(name: &str) -> Option<Mode> {
    MODES.iter().find(|(n, _)| *n == name).map(|(_, m)| *m)
}

/// 13.2.6.4.18 "in template": a table part's start tag right in a template is processed in
/// the mode it would have in a table (so it needs no table, no implied `<tbody>` / `<tr>`);
/// any other start tag by the body's rules.
pub(super) static TEMPLATE_CONTENT: &[(&[&str], Mode)] = &[
    (
        &["caption", "colgroup", "tbody", "tfoot", "thead"],
        Mode::Table,
    ),
    (&["col"], Mode::ColumnGroup),
    (&["tr"], Mode::TableBody),
    (&["td", "th"], Mode::Row),
];

/// The row of [`TABLE_START_TAGS`] a start tag `name` (lower-case) right in a template takes.
pub(super) fn template_start(name: &str) -> Option<TableStart> {
    TEMPLATE_CONTENT
        .iter()
        .find(|(names, _)| names.contains(&name))
        .and_then(|(_, mode)| table_start(*mode, name))
}

// ============================================================================
// Scopes
// ============================================================================

/// The scope an element is searched in (13.2.4.2 "has an element in scope"): the search
/// from the current node stops at a boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    /// [`SCOPE`].
    Default,
    /// "in list item scope": the default one, `ol` and `ul`.
    ListItem,
    /// "in button scope": the default one and `button`.
    Button,
    /// "in table scope": [`TABLE_SCOPE`].
    Table,
}

impl Scope {
    /// Whether an open `name` (lower-case) ends a search in this scope.
    pub(super) fn is_boundary(self, name: &str) -> bool {
        match self {
            Self::Default => is(name, SCOPE),
            Self::ListItem => is(name, SCOPE) || name == "ol" || name == "ul",
            Self::Button => is(name, SCOPE) || name == "button",
            Self::Table => is(name, TABLE_SCOPE),
        }
    }
}

// ============================================================================
// The start tags of the body (13.2.6.4.7 "in body")
// ============================================================================

/// One step of a start tag's rule. The steps marked HTML do nothing in the strict loaders
/// (`TreeRules::Xml` / `XmlFolded`), which keep the other steps as XML conveniences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// "If the stack of open elements has a p element in button scope, then close a p
    /// element."
    CloseP,
    /// The same, except in quirks mode (the start tag `table`).
    ClosePUnlessQuirks,
    /// "If the current node is an HTML element whose tag name is one of h1 ... h6, then
    /// ... pop the current node off the stack of open elements."
    PopHeading,
    /// "If the current node is an X element, then pop the current node" (`option`,
    /// `optgroup`).
    PopCurrent(&'static str),
    /// The loop of `li` / `dd` `dt`: close the nearest open item of these names, unless a
    /// special element other than `address`, `div` and `p` comes first.
    CloseListItem(&'static [&'static str]),
    /// XML: a cell / row / row group closes the open cell / row / row group of its table
    /// (what the table modes' "clear the stack back to a ... context" and "close the cell"
    /// do; in HTML the table modes take these start tags before the body's rules).
    CloseInTable(&'static [&'static str]),
    /// HTML: "If the stack of open elements has an X element in scope, then ... generate
    /// implied end tags, pop elements until an X element has been popped" (`button`).
    CloseInScope(&'static str),
    /// HTML: "If the stack of open elements has a ruby element in scope, then generate
    /// implied end tags" (`rb rtc`; `rp rt`: except for `rtc` elements).
    CloseRubyParts { except_rtc: bool },
    /// HTML: an `a` start tag while an `a` is in the list of active formatting elements:
    /// the adoption agency algorithm for `a`, then that `a` is removed from the list and the
    /// stack.
    CloseOpenLink,
    /// HTML: "If the stack of open elements has a nobr element in scope, then ... run the
    /// adoption agency algorithm for the token, then once again reconstruct the active
    /// formatting elements."
    CloseOpenNobr,
    /// HTML: "If the form element pointer is not null, and there is no template element on
    /// the stack of open elements, then this is a parse error; ignore the token."
    IgnoreInForm,
    /// HTML: "Parse error. Ignore the token." (the table parts, `frame`, `head` in the
    /// body).
    Ignore,
    /// HTML: "Reconstruct the active formatting elements, if any."
    Reconstruct,
    /// "Insert an HTML element for the token."
    Insert,
    /// "Insert an HTML element for the token. Immediately pop the current node off the
    /// stack of open elements."
    InsertVoid,
    /// Insert, and "push onto the list of active formatting elements that element".
    InsertFormatting,
    /// Insert, and "insert a marker at the end of the list of active formatting elements".
    InsertMarker,
    /// Insert, and (HTML) set the form element pointer.
    InsertForm,
    /// Insert; (HTML) "If the next token is a U+000A LINE FEED (LF) character token, then
    /// ignore that token" (`pre listing textarea`).
    InsertSkippingNewline,
    /// Insert a foreign element (`math`, `svg`): its content is SVG / `MathML`; acknowledged
    /// self-closing.
    InsertForeign,
}

use Step::{
    CloseInScope, CloseInTable, CloseListItem, CloseOpenLink, CloseOpenNobr, CloseP,
    ClosePUnlessQuirks, CloseRubyParts, Ignore, IgnoreInForm, Insert, InsertForeign, InsertForm,
    InsertFormatting, InsertMarker, InsertSkippingNewline, InsertVoid, PopCurrent, PopHeading,
    Reconstruct,
};

/// The cells, rows and row groups a table part's start tag closes (XML; [`CloseInTable`]).
const CELLS: &[&str] = &["td", "th"];
const CELLS_AND_ROWS: &[&str] = &["td", "th", "tr"];
const TABLE_PARTS: &[&str] = &[
    "td", "th", "tr", "thead", "tbody", "tfoot", "caption", "colgroup",
];

/// 13.2.6.4.7 "in body", the start tags (each row cites the rule's tag names), in the
/// order the standard lists them. A start tag no row names: [`DEFAULT_START`]. `html`,
/// `body` and `frameset` are the document's (the tree construction handles them before
/// these rows in HTML; XML inserts them).
pub(super) static START_TAGS: &[(&[&str], &[Step])] = &[
    // "base, basefont, bgsound, link, meta, noframes, script, style, template, title":
    // process using the rules for "in head" - inserted where they stand.
    (
        &["base", "basefont", "bgsound", "link", "meta"],
        &[InsertVoid],
    ),
    (&["noframes", "script", "style", "title"], &[Insert]),
    // "template": also "insert a marker at the end of the list of active formatting
    // elements"
    (&["template"], &[InsertMarker]),
    // "address, article, aside, blockquote, center, details, dialog, dir, div, dl,
    // fieldset, figcaption, figure, footer, header, hgroup, main, menu, nav, ol, p, search,
    // section, summary, ul"
    (
        &[
            "address",
            "article",
            "aside",
            "blockquote",
            "center",
            "details",
            "dialog",
            "dir",
            "div",
            "dl",
            "fieldset",
            "figcaption",
            "figure",
            "footer",
            "header",
            "hgroup",
            "main",
            "menu",
            "nav",
            "ol",
            "p",
            "search",
            "section",
            "summary",
            "ul",
        ],
        &[CloseP, Insert],
    ),
    // "h1, h2, h3, h4, h5, h6": a heading ends the open heading
    (
        &["h1", "h2", "h3", "h4", "h5", "h6"],
        &[CloseP, PopHeading, Insert],
    ),
    // "pre, listing"
    (&["pre", "listing"], &[CloseP, InsertSkippingNewline]),
    // "form"
    (&["form"], &[IgnoreInForm, CloseP, InsertForm]),
    // "li"
    (&["li"], &[CloseListItem(&["li"]), CloseP, Insert]),
    // "dd, dt"
    (
        &["dd", "dt"],
        &[CloseListItem(&["dd", "dt"]), CloseP, Insert],
    ),
    // "plaintext" (the tokenizer reads the rest as text: ELEMENTS)
    (&["plaintext"], &[CloseP, Insert]),
    // "button"
    (&["button"], &[CloseInScope("button"), Reconstruct, Insert]),
    // "a"
    (&["a"], &[CloseOpenLink, Reconstruct, InsertFormatting]),
    // "b, big, code, em, font, i, s, small, strike, strong, tt, u"
    (
        &[
            "b", "big", "code", "em", "font", "i", "s", "small", "strike", "strong", "tt", "u",
        ],
        &[Reconstruct, InsertFormatting],
    ),
    // "nobr"
    (&["nobr"], &[Reconstruct, CloseOpenNobr, InsertFormatting]),
    // "applet, marquee, object"
    (
        &["applet", "marquee", "object"],
        &[Reconstruct, InsertMarker],
    ),
    // "table": a table stays in an open p in quirks mode
    (&["table"], &[ClosePUnlessQuirks, Insert]),
    // "area, br, embed, img, keygen, wbr" and "input"
    (
        &["area", "br", "embed", "img", "keygen", "wbr", "input"],
        &[Reconstruct, InsertVoid],
    ),
    // "param, source, track"
    (&["param", "source", "track"], &[InsertVoid]),
    // "hr"
    (&["hr"], &[CloseP, InsertVoid]),
    // "textarea"
    (&["textarea"], &[InsertSkippingNewline]),
    // "xmp"
    (&["xmp"], &[CloseP, Reconstruct, Insert]),
    // "iframe", "noembed"
    (&["iframe", "noembed"], &[Insert]),
    // "select"
    (&["select"], &[Reconstruct, Insert]),
    // "optgroup, option" (an `optgroup` also ends an open `optgroup`: the select's rule,
    // which keeps `<optgroup>`s siblings as Chrome does)
    (&["option"], &[PopCurrent("option"), Reconstruct, Insert]),
    (
        &["optgroup"],
        &[
            PopCurrent("option"),
            PopCurrent("optgroup"),
            Reconstruct,
            Insert,
        ],
    ),
    // "rb, rtc"
    (
        &["rb", "rtc"],
        &[CloseRubyParts { except_rtc: false }, Insert],
    ),
    // "rp, rt"
    (
        &["rp", "rt"],
        &[CloseRubyParts { except_rtc: true }, Insert],
    ),
    // "math", "svg"
    (&["math", "svg"], &[Reconstruct, InsertForeign]),
    // "caption, col, colgroup, frame, head, tbody, td, tfoot, th, thead, tr": parse error,
    // ignored in the body (inside a table the table modes take them first). XML closes
    // the open cell / row / row group instead.
    (&["tr"], &[Ignore, CloseInTable(CELLS_AND_ROWS), Insert]),
    (&["td", "th"], &[Ignore, CloseInTable(CELLS), Insert]),
    (
        &["caption", "colgroup", "tbody", "tfoot", "thead"],
        &[Ignore, CloseInTable(TABLE_PARTS), Insert],
    ),
    (&["col", "frame"], &[Ignore, InsertVoid]),
    (&["head"], &[Ignore, Insert]),
];

/// "Any other start tag": "Reconstruct the active formatting elements, if any. Insert an
/// HTML element for the token."
pub(super) static DEFAULT_START: &[Step] = &[Reconstruct, Insert];

/// The steps of the start tag `name` (lower-case).
pub(super) fn start_steps(name: &str) -> &'static [Step] {
    START_TAGS
        .iter()
        .find(|(names, _)| names.contains(&name))
        .map_or(DEFAULT_START, |(_, steps)| steps)
}

/// Whether a `start` tag ends an open `open` element it would go into
/// directly.
///
/// The tree construction's implied end tags for a direct parent
/// (`<p>` ends where a `<div>` starts, an `<li>` at the next `<li>`, a cell at
/// the next cell or row). What a document editor asks before it nests
/// `start` inside `open`: a loader would make them siblings. Names are
/// lower-case. Read from [`START_TAGS`] (the steps the strict loaders run).
#[must_use]
pub fn start_tag_closes(open: &str, start: &str) -> bool {
    start_steps(start).iter().any(|step| match step {
        CloseP | ClosePUnlessQuirks => open == "p",
        PopHeading => is(open, HEADING),
        PopCurrent(name) => *name == open,
        CloseListItem(names) | CloseInTable(names) => names.contains(&open),
        _ => false,
    })
}

// ============================================================================
// The end tags of the body (13.2.6.4.7 "in body")
// ============================================================================

/// What an end tag does in HTML's body. In XML every end tag closes its element if it is
/// open in the row's scope (a formatting element with a block open inside it once that
/// block closes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EndAction {
    /// "If the stack of open elements does not have an element in scope that is an HTML
    /// element with the same tag name as that of the token, then this is a parse error;
    /// ignore the token. Otherwise: generate implied end tags ... pop elements until an
    /// HTML element with the same tag name as the token has been popped."
    CloseInScope,
    /// "p": "If the stack of open elements does not have a p element in button scope, then
    /// this is a parse error; insert an HTML element for a "p" start tag token with no
    /// attributes. Close a p element."
    P,
    /// "h1 ... h6": closes the nearest open heading, whatever its level.
    Heading,
    /// "a, b, big, code, em, font, i, nobr, s, small, strike, strong, tt, u": the adoption
    /// agency algorithm.
    Adoption,
    /// "applet, marquee, object": close in scope, then "clear the list of active formatting
    /// elements up to the last marker".
    CloseToMarker,
    /// "br": "Drop the attributes from the token, and act as described in the next entry;
    /// i.e. act as if this was a "br" start tag token with no attributes".
    Br,
    /// "form": the form element pointer's element leaves the stack (what is open in it
    /// stays open).
    Form,
    /// "body", "html", and (in the body) "head": nothing - the body stays open, content
    /// after `</body>` goes into it.
    Ignore,
    /// "Any other end tag": close the nearest open element of this name, unless a special
    /// element comes first.
    AnyOther,
}

/// 13.2.6.4.7 "in body", the end tags: `(names, the scope XML looks in, the HTML action)`.
/// An end tag no row names: [`DEFAULT_END`].
pub(super) static END_TAGS: &[(&[&str], Scope, EndAction)] = &[
    (&["body", "html", "head"], Scope::Default, EndAction::Ignore),
    // "address, article, aside, blockquote, button, center, details, dialog, dir, div, dl,
    // fieldset, figcaption, figure, footer, header, hgroup, listing, main, menu, nav, ol,
    // pre, search, section, summary, ul"
    (
        &[
            "address",
            "article",
            "aside",
            "blockquote",
            "button",
            "center",
            "details",
            "dialog",
            "dir",
            "div",
            "dl",
            "fieldset",
            "figcaption",
            "figure",
            "footer",
            "header",
            "hgroup",
            "listing",
            "main",
            "menu",
            "nav",
            "ol",
            "pre",
            "search",
            "section",
            "summary",
            "ul",
        ],
        Scope::Default,
        EndAction::CloseInScope,
    ),
    (&["form"], Scope::Default, EndAction::Form),
    (&["p"], Scope::Button, EndAction::P),
    // "li": in list item scope
    (&["li"], Scope::ListItem, EndAction::CloseInScope),
    (&["dd", "dt"], Scope::Default, EndAction::CloseInScope),
    (
        &["h1", "h2", "h3", "h4", "h5", "h6"],
        Scope::Default,
        EndAction::Heading,
    ),
    (
        &[
            "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small", "strike", "strong",
            "tt", "u",
        ],
        Scope::Default,
        EndAction::Adoption,
    ),
    (
        &["applet", "marquee", "object"],
        Scope::Default,
        EndAction::CloseToMarker,
    ),
    (&["br"], Scope::Default, EndAction::Br),
    // the table's end tags: the table modes take them in a table; in the body they are
    // any other end tag (XML looks in table scope)
    (
        &[
            "td", "th", "tr", "tbody", "thead", "tfoot", "table", "caption", "colgroup",
        ],
        Scope::Table,
        EndAction::AnyOther,
    ),
];

/// "Any other end tag", in the default scope (XML).
pub(super) const DEFAULT_END: (Scope, EndAction) = (Scope::Default, EndAction::AnyOther);

/// The rule of the end tag `name` (lower-case).
pub(super) fn end_rule(name: &str) -> (Scope, EndAction) {
    END_TAGS
        .iter()
        .find(|(names, _, _)| names.contains(&name))
        .map_or(DEFAULT_END, |(_, scope, action)| (*scope, *action))
}

// ============================================================================
// The table modes (13.2.6.4.9 - 13.2.6.4.15)
// ============================================================================

/// "Clear the stack back to a ... context": pop until the current node is one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Context {
    /// "a table context": `table`, `template`, `html`.
    Table,
    /// "a table body context": `tbody`, `tfoot`, `thead`, `template`, `html`.
    TableBody,
    /// "a table row context": `tr`, `template`, `html`.
    Row,
}

impl Context {
    /// Whether an open `name` ends the clearing.
    pub(super) fn holds(self, name: &str) -> bool {
        matches!(name, "template" | "html")
            || match self {
                Self::Table => name == "table",
                Self::TableBody => matches!(name, "tbody" | "tfoot" | "thead"),
                Self::Row => name == "tr",
            }
    }
}

/// What a start tag does in a table mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TableStart {
    /// Clear the stack back to the context, insert (a marker too: `caption`, `td`, `th`).
    Insert(Context, bool),
    /// Clear the stack back to the context, insert the implied element (`<col>` its
    /// `<colgroup>`, a row its `<tbody>`, a cell its `<tr>`), reprocess the token.
    Imply(Context, &'static str),
    /// If one of these is open in table scope: pop until it has been popped, reprocess the
    /// token; else ignore it.
    CloseAndReprocess(&'static [&'static str]),
    /// Insert where it stands, no foster parenting (`style script template` - the rules of
    /// "in head").
    InsertHere,
    /// `input`: a hidden one is inserted where it stands (and popped), any other is
    /// foster-parented.
    Input,
    /// `form`: inserted and popped at once (if no form is open), where it stands.
    Form,
    /// Inserted and popped (`col` in a column group).
    InsertVoid,
}

/// The table modes' start tags: `(mode, names, action)`. A start tag no row of its mode
/// names: in table body and in row the rows of "in table" next; then "in table": foster
/// parenting and the body's rules; "in cell" / "in caption": the body's rules; "in column
/// group": the colgroup ends and the token is reprocessed.
pub(super) static TABLE_START_TAGS: &[(Mode, &[&str], TableStart)] = &[
    // 13.2.6.4.9 "in table"
    (
        Mode::Table,
        &["caption"],
        TableStart::Insert(Context::Table, true),
    ),
    (
        Mode::Table,
        &["colgroup", "tbody", "tfoot", "thead"],
        TableStart::Insert(Context::Table, false),
    ),
    (
        Mode::Table,
        &["col"],
        TableStart::Imply(Context::Table, "colgroup"),
    ),
    (
        Mode::Table,
        &["td", "th", "tr"],
        TableStart::Imply(Context::Table, "tbody"),
    ),
    // "table": parse error; the open table ends, the new one is its sibling
    (
        Mode::Table,
        &["table"],
        TableStart::CloseAndReprocess(&["table"]),
    ),
    (
        Mode::Table,
        &["style", "script", "template"],
        TableStart::InsertHere,
    ),
    (Mode::Table, &["input"], TableStart::Input),
    (Mode::Table, &["form"], TableStart::Form),
    // 13.2.6.4.13 "in table body"
    (
        Mode::TableBody,
        &["tr"],
        TableStart::Insert(Context::TableBody, false),
    ),
    (
        Mode::TableBody,
        &["td", "th"],
        TableStart::Imply(Context::TableBody, "tr"),
    ),
    (
        Mode::TableBody,
        &["caption", "col", "colgroup", "tbody", "tfoot", "thead"],
        TableStart::CloseAndReprocess(&["tbody", "tfoot", "thead"]),
    ),
    // 13.2.6.4.14 "in row"
    (
        Mode::Row,
        &["td", "th"],
        TableStart::Insert(Context::Row, true),
    ),
    (
        Mode::Row,
        &[
            "caption", "col", "colgroup", "tbody", "tfoot", "thead", "tr",
        ],
        TableStart::CloseAndReprocess(&["tr"]),
    ),
    // 13.2.6.4.15 "in cell": a table part closes the cell
    (
        Mode::Cell,
        &[
            "caption", "col", "colgroup", "tbody", "td", "tfoot", "th", "thead", "tr",
        ],
        TableStart::CloseAndReprocess(&["td", "th"]),
    ),
    // 13.2.6.4.11 "in caption": a table part closes the caption
    (
        Mode::Caption,
        &[
            "caption", "col", "colgroup", "tbody", "td", "tfoot", "th", "thead", "tr",
        ],
        TableStart::CloseAndReprocess(&["caption"]),
    ),
    // 13.2.6.4.12 "in column group"
    (Mode::ColumnGroup, &["col"], TableStart::InsertVoid),
];

/// What an end tag does in a table mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TableEnd {
    /// The element of the token's name: if it is open in table scope, pop until it has
    /// been popped; else ignore the token.
    Close,
    /// If one of these is open in table scope: pop until it has been popped, reprocess the
    /// token; else ignore it.
    CloseAndReprocess(&'static [&'static str]),
    /// If the token's own element is open in table scope, as [`TableEnd::CloseAndReprocess`]
    /// (in row: `</tbody>` ends the row; in cell: `</tr>` ends the cell); else ignore.
    CloseIfOpen(&'static [&'static str]),
    /// "Parse error. Ignore the token."
    Ignore,
}

/// The table modes' end tags: `(mode, names, action)`; a tag no row of its mode names goes
/// on as a start tag does ([`TABLE_START_TAGS`]).
pub(super) static TABLE_END_TAGS: &[(Mode, &[&str], TableEnd)] = &[
    // 13.2.6.4.9 "in table"
    (Mode::Table, &["table"], TableEnd::Close),
    (
        Mode::Table,
        &[
            "body", "caption", "col", "colgroup", "html", "tbody", "td", "tfoot", "th", "thead",
            "tr",
        ],
        TableEnd::Ignore,
    ),
    // 13.2.6.4.13 "in table body"
    (
        Mode::TableBody,
        &["tbody", "tfoot", "thead"],
        TableEnd::Close,
    ),
    (
        Mode::TableBody,
        &["table"],
        TableEnd::CloseAndReprocess(&["tbody", "tfoot", "thead"]),
    ),
    (
        Mode::TableBody,
        &[
            "body", "caption", "col", "colgroup", "html", "td", "th", "tr",
        ],
        TableEnd::Ignore,
    ),
    // 13.2.6.4.14 "in row"
    (Mode::Row, &["tr"], TableEnd::Close),
    (Mode::Row, &["table"], TableEnd::CloseAndReprocess(&["tr"])),
    (
        Mode::Row,
        &["tbody", "tfoot", "thead"],
        TableEnd::CloseIfOpen(&["tr"]),
    ),
    (
        Mode::Row,
        &["body", "caption", "col", "colgroup", "html", "td", "th"],
        TableEnd::Ignore,
    ),
    // 13.2.6.4.15 "in cell"
    (Mode::Cell, &["td", "th"], TableEnd::Close),
    (
        Mode::Cell,
        &["body", "caption", "col", "colgroup", "html"],
        TableEnd::Ignore,
    ),
    (
        Mode::Cell,
        &["table", "tbody", "tfoot", "thead", "tr"],
        TableEnd::CloseIfOpen(&["td", "th"]),
    ),
    // 13.2.6.4.11 "in caption"
    (Mode::Caption, &["caption"], TableEnd::Close),
    (
        Mode::Caption,
        &["table"],
        TableEnd::CloseAndReprocess(&["caption"]),
    ),
    (
        Mode::Caption,
        &[
            "body", "col", "colgroup", "html", "tbody", "td", "tfoot", "th", "thead", "tr",
        ],
        TableEnd::Ignore,
    ),
    // 13.2.6.4.12 "in column group"
    (Mode::ColumnGroup, &["colgroup"], TableEnd::Close),
    (Mode::ColumnGroup, &["col"], TableEnd::Ignore),
];

/// The row of `name` (lower-case) in `mode`'s start tags.
pub(super) fn table_start(mode: Mode, name: &str) -> Option<TableStart> {
    TABLE_START_TAGS
        .iter()
        .find(|(m, names, _)| *m == mode && names.contains(&name))
        .map(|(_, _, action)| *action)
}

/// The row of `name` (lower-case) in `mode`'s end tags.
pub(super) fn table_end(mode: Mode, name: &str) -> Option<TableEnd> {
    TABLE_END_TAGS
        .iter()
        .find(|(m, names, _)| *m == mode && names.contains(&name))
        .map(|(_, _, action)| *action)
}

// ============================================================================
// Quirks mode (13.2.6.4.1 "initial", a DOCTYPE token)
// ============================================================================

/// "The public identifier starts with" one of these (ASCII case-insensitively): quirks
/// mode.
pub(super) static QUIRKS_PUBLIC_ID_PREFIXES: &[&str] = &[
    "+//silmaril//dtd html pro v0r11 19970101//",
    "-//as//dtd html 3.0 aswedit + extensions//",
    "-//advasoft ltd//dtd html 3.0 aswedit + extensions//",
    "-//ietf//dtd html 2.0 level 1//",
    "-//ietf//dtd html 2.0 level 2//",
    "-//ietf//dtd html 2.0 strict level 1//",
    "-//ietf//dtd html 2.0 strict level 2//",
    "-//ietf//dtd html 2.0 strict//",
    "-//ietf//dtd html 2.0//",
    "-//ietf//dtd html 2.1e//",
    "-//ietf//dtd html 3.0//",
    "-//ietf//dtd html 3.2 final//",
    "-//ietf//dtd html 3.2//",
    "-//ietf//dtd html 3//",
    "-//ietf//dtd html level 0//",
    "-//ietf//dtd html level 1//",
    "-//ietf//dtd html level 2//",
    "-//ietf//dtd html level 3//",
    "-//ietf//dtd html strict level 0//",
    "-//ietf//dtd html strict level 1//",
    "-//ietf//dtd html strict level 2//",
    "-//ietf//dtd html strict level 3//",
    "-//ietf//dtd html strict//",
    "-//ietf//dtd html//",
    "-//metrius//dtd metrius presentational//",
    "-//microsoft//dtd internet explorer 2.0 html strict//",
    "-//microsoft//dtd internet explorer 2.0 html//",
    "-//microsoft//dtd internet explorer 2.0 tables//",
    "-//microsoft//dtd internet explorer 3.0 html strict//",
    "-//microsoft//dtd internet explorer 3.0 html//",
    "-//microsoft//dtd internet explorer 3.0 tables//",
    "-//netscape comm. corp.//dtd html//",
    "-//netscape comm. corp.//dtd strict html//",
    "-//o'reilly and associates//dtd html 2.0//",
    "-//o'reilly and associates//dtd html extended 1.0//",
    "-//o'reilly and associates//dtd html extended relaxed 1.0//",
    "-//sq//dtd html 2.0 hotmetal + extensions//",
    "-//softquad software//dtd hotmetal pro 6.0::19990601::extensions to html 4.0//",
    "-//softquad//dtd hotmetal pro 4.0::19971010::extensions to html 4.0//",
    "-//spyglass//dtd html 2.0 extended//",
    "-//sun microsystems corp.//dtd hotjava html//",
    "-//sun microsystems corp.//dtd hotjava strict html//",
    "-//w3c//dtd html 3 1995-03-24//",
    "-//w3c//dtd html 3.2 draft//",
    "-//w3c//dtd html 3.2 final//",
    "-//w3c//dtd html 3.2//",
    "-//w3c//dtd html 3.2s draft//",
    "-//w3c//dtd html 4.0 frameset//",
    "-//w3c//dtd html 4.0 transitional//",
    "-//w3c//dtd html experimental 19960712//",
    "-//w3c//dtd html experimental 970421//",
    "-//w3c//dtd w3 html//",
    "-//w3o//dtd w3 html 3.0//",
    "-//webtechs//dtd mozilla html 2.0//",
    "-//webtechs//dtd mozilla html//",
];

/// "The public identifier is set to" one of these (case-insensitively): quirks mode.
pub(super) static QUIRKS_PUBLIC_IDS: &[&str] = &[
    "-//w3o//dtd w3 html strict 3.0//en//",
    "-/w3c/dtd html 4.0 transitional/en",
    "html",
];

/// "The system identifier is set to" this: quirks mode.
pub(super) static QUIRKS_SYSTEM_IDS: &[&str] =
    &["http://www.ibm.com/data/dtd/v11/ibmxhtml1-transitional.dtd"];

/// "The system identifier is missing and the public identifier starts with" one of these:
/// quirks mode (with a system identifier: limited quirks, which changes no tree).
pub(super) static QUIRKS_PUBLIC_ID_PREFIXES_WITHOUT_SYSTEM_ID: &[&str] = &[
    "-//w3c//dtd html 4.01 frameset//",
    "-//w3c//dtd html 4.01 transitional//",
];

/// Whether `prefix` (lower-case) starts `s`, ASCII case-insensitively.
fn starts_with_ignoring_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// Whether a document with this doctype is in quirks mode (13.2.6.4.1, "A DOCTYPE token").
/// `name` is lower-case.
pub(super) fn doctype_is_quirky(
    name: &str,
    public_id: Option<&str>,
    system_id: Option<&str>,
    force_quirks: bool,
) -> bool {
    if force_quirks || name != "html" {
        return true;
    }
    if let Some(system) = system_id {
        if QUIRKS_SYSTEM_IDS
            .iter()
            .any(|id| system.eq_ignore_ascii_case(id))
        {
            return true;
        }
    }
    let Some(public) = public_id else {
        return false;
    };
    QUIRKS_PUBLIC_IDS
        .iter()
        .any(|id| public.eq_ignore_ascii_case(id))
        || QUIRKS_PUBLIC_ID_PREFIXES
            .iter()
            .any(|prefix| starts_with_ignoring_case(public, prefix))
        || (system_id.is_none()
            && QUIRKS_PUBLIC_ID_PREFIXES_WITHOUT_SYSTEM_ID
                .iter()
                .any(|prefix| starts_with_ignoring_case(public, prefix)))
}
