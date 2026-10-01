//! Real-world HTML mail parses like a browser parses it.
//!
//! The mail corpus (`tests/mail_corpus/`, see SOURCES.tsv: Cerberus, Mailgun,
//! Postmark, a Litmus-style template and the exploration's Gmail / Outlook /
//! Apple Mail / Thunderbird / receipt / hostile / legacy samples) is HTML as
//! mail clients send it: unquoted and bare attributes, `<BR>`, `<TABLE>`,
//! `<p>` and `<li>` without end tags, Outlook's `<o:p>` and conditional
//! comments. The XML loaders reject most of it (`InvalidQuote` on
//! `<meta http-equiv=Content-Type`, an `UnclosedRootNode` for a `<P>` never
//! closed). The LENIENT loader (`parse_html_string`) builds the tree Chrome
//! builds: `tests/mail_corpus/outlines/` holds Chrome's trees
//! (`scripts/refci/mail_outlines.py`, `DOMParser` text/html), and each mail's
//! `<head>` and `<body>` must outline the same.
//!
//! Two Postmark templates put template text (`{{#each ..}}`) between table
//! rows; Chrome moves such text in front of the table (foster parenting),
//! which the simplified tree construction does not do - their ELEMENTS must
//! still be Chrome's.
//!
//! And the lenient document loader (`parse_html_to_styled_dom`, the arena
//! path) builds the same DOM from every mail as the lenient tree loader
//! (`parse_html` + `dom_from_parsed_xml`): one tree construction for both.
//!
//! Not compiled by the author (house rule); RED before the lenient loader.

use azul_core::{
    dom::{Dom, NodeId, NodeType},
    xml::{html::outline, XmlNode, XmlNodeChild},
};
use azul_layout::xml::{
    dom_from_parsed_xml, parse_html, parse_html_string, parse_html_to_styled_dom,
};

/// The corpus, as `<dir>/<mail>` (without `.html`).
const CORPUS: &[&str] = &[
    "cerberus/cerberus-fluid",
    "cerberus/cerberus-hybrid",
    "cerberus/cerberus-responsive",
    "exploration/01_newsletter",
    "exploration/02_gmail_reply",
    "exploration/03_outlook_reply",
    "exploration/04_receipt",
    "exploration/05_apple_mail_reply",
    "exploration/06_thunderbird_reply",
    "exploration/07_hostile",
    "exploration/08_legacy_uppercase",
    "leemunroe/email-inlined",
    "mailgun/action",
    "mailgun/alert",
    "mailgun/billing",
    "postmark/invoice",
    "postmark/receipt",
    "postmark/welcome",
];

/// The mails whose template text Chrome foster-parents out of a table.
const FOSTER_PARENTED: &[&str] = &["postmark/invoice", "postmark/receipt"];

fn corpus_file(relative: &str) -> String {
    let path = format!(
        "{}/../tests/mail_corpus/{relative}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Chrome's `(head, body)` outlines of `mail`.
fn chrome_outlines(mail: &str) -> (String, String) {
    let text = corpus_file(&format!("outlines/{mail}.txt"));
    let mut lines = text.lines();
    let head = lines
        .next()
        .and_then(|l| l.strip_prefix("head: "))
        .unwrap_or_default()
        .trim_end()
        .to_string();
    let body = lines
        .next()
        .and_then(|l| l.strip_prefix("body: "))
        .unwrap_or_default()
        .trim_end()
        .to_string();
    (head, body)
}

/// The element named `tag`, depth first.
fn find<'a>(nodes: &'a [XmlNodeChild], tag: &str) -> Option<&'a XmlNode> {
    for node in nodes {
        if let XmlNodeChild::Element(element) = node {
            if element.node_type.as_str() == tag {
                return Some(element);
            }
            if let Some(found) = find(element.children.as_ref(), tag) {
                return Some(found);
            }
        }
    }
    None
}

/// The lenient loader's `(head, body)` outlines of `html` (a fragment's
/// roots are its body, as a browser's).
fn azul_outlines(html: &str) -> (String, String) {
    let nodes = parse_html_string(html);
    let head =
        find(&nodes, "head").map_or_else(String::new, |h| outline(h.children.as_ref(), true));
    let body = match find(&nodes, "body") {
        Some(body) => outline(body.children.as_ref(), true),
        None => outline(&nodes, true),
    };
    (head, body)
}

/// Where two outlines part, with some context.
fn first_difference(expected: &str, got: &str) -> String {
    let at = expected
        .char_indices()
        .zip(got.chars())
        .find(|((_, a), b)| a != b)
        .map_or(expected.len().min(got.len()), |((i, _), _)| i);
    let from = at.saturating_sub(120);
    let window = |s: &str| {
        let start = (from..=at).find(|i| s.is_char_boundary(*i)).unwrap_or(at);
        let end = (at + 200).min(s.len());
        let end = (end..=s.len())
            .find(|i| s.is_char_boundary(*i))
            .unwrap_or(s.len());
        s.get(start..end).unwrap_or_default().to_string()
    };
    format!(
        "at byte {at}\n    chrome: ...{}...\n    azul:   ...{}...",
        window(expected),
        window(got)
    )
}

/// An outline without its texts (and the spaces they leave).
fn elements_only(outline: &str) -> String {
    let mut out = String::new();
    let mut chars = outline.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            // Skip the JSON string.
            while let Some(d) = chars.next() {
                match d {
                    '\\' => {
                        let _ = chars.next();
                    }
                    '"' => break,
                    _ => {}
                }
            }
            continue;
        }
        out.push(c);
    }
    let mut squeezed = String::new();
    for word in out.split_whitespace() {
        if !squeezed.is_empty() {
            squeezed.push(' ');
        }
        squeezed.push_str(word);
    }
    squeezed
        .replace("{ ", "{")
        .replace(" }", "}")
        .replace("{}", "")
}

#[test]
fn every_corpus_mail_parses_to_the_tree_chrome_builds() {
    let mut failures = Vec::new();
    for mail in CORPUS.iter().filter(|m| !FOSTER_PARENTED.contains(m)) {
        let (chrome_head, chrome_body) = chrome_outlines(mail);
        let (head, body) = azul_outlines(&corpus_file(&format!("{mail}.html")));
        if head != chrome_head {
            failures.push(format!(
                "{mail} <head> {}",
                first_difference(&chrome_head, &head)
            ));
        }
        if body != chrome_body {
            failures.push(format!(
                "{mail} <body> {}",
                first_difference(&chrome_body, &body)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_mail_with_template_text_between_table_rows_keeps_chromes_elements() {
    for mail in FOSTER_PARENTED {
        let (_, chrome_body) = chrome_outlines(mail);
        let (_, body) = azul_outlines(&corpus_file(&format!("{mail}.html")));
        let (chrome, azul) = (elements_only(&chrome_body), elements_only(&body));
        assert!(
            !azul.is_empty() && azul == chrome,
            "{mail}: {}",
            first_difference(&chrome, &azul)
        );
    }
}

/// `(depth, what)` of every node of a `Dom`, depth first: an element is its
/// tag, a text its text.
fn dom_nodes(dom: &Dom, depth: usize, out: &mut Vec<(usize, String)>) {
    out.push((depth, kind(&dom.root.node_type)));
    for child in dom.children.as_ref() {
        dom_nodes(child, depth + 1, out);
    }
}

fn kind(node_type: &NodeType) -> String {
    match node_type {
        NodeType::Text(text) => format!("{:?}", text.as_str()),
        other => format!("{:?}", other.get_path()),
    }
}

#[test]
fn the_lenient_document_loader_builds_the_dom_of_the_lenient_tree_loader() {
    let mut failures = Vec::new();
    for mail in CORPUS {
        let html = corpus_file(&format!("{mail}.html"));

        let mut tree = Vec::new();
        dom_nodes(&dom_from_parsed_xml(parse_html(&html)), 0, &mut tree);

        let styled = parse_html_to_styled_dom(&html);
        let hierarchy = styled.node_hierarchy.as_container();
        let node_data = styled.node_data.as_ref();
        let mut depths: Vec<usize> = Vec::with_capacity(node_data.len());
        let mut arena = Vec::with_capacity(node_data.len());
        for (i, node) in node_data.iter().enumerate() {
            let depth = hierarchy[NodeId::new(i)]
                .parent_id()
                .map_or(0, |p| depths[p.index()] + 1);
            depths.push(depth);
            arena.push((depth, kind(&node.node_type)));
        }

        if tree != arena {
            let at = tree
                .iter()
                .zip(arena.iter())
                .position(|(a, b)| a != b)
                .unwrap_or(tree.len().min(arena.len()));
            failures.push(format!(
                "{mail}: {} nodes from the tree loader, {} from the document loader; first \
                 difference at node {at}: tree {:?}, arena {:?}",
                tree.len(),
                arena.len(),
                tree.get(at),
                arena.get(at)
            ));
        }
        assert!(arena.len() > 3, "{mail}: an empty document");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
