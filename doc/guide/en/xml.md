---
slug: xml
title: XML Parsing (Standalone)
language: en
canonical_slug: xml
audience: external
maturity: wip
guide_order: 230
topic_only: false
short_desc: Loading a Dom from an XML string — the syntax, supported tags, and how it interacts with components.
prerequisites: []
tracked_files:
  - core/src/xml.rs
  - layout/src/xml/mod.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# XML Parsing (Standalone)

> **WIP** — public surface is stable but the component-instantiation pipeline (`ComponentMap`, `<my-widget>` style custom tags) is mid-migration; APIs marked *component* below may shift.

Azul ships an XML/HTML5-lite parser that runs without a window, a renderer, or any GPU dependency. Use it as a tooling library: validate `.azul` files in CI, scrape resource URLs from a static site, build a `StyledDom` for headless screenshot rendering, or feed XHTML into custom layout code.

The parser is exposed in two layers:

| Layer | Crate | What it produces |
|---|---|---|
| Tree (mutable) | `azul_layout::xml::parse_xml_string` | `Vec<XmlNodeChild>` — a generic XML tree |
| Direct-to-DOM | `azul_layout::xml::parse_xml_to_styled_dom` | `StyledDom` — ready for layout, with `<style>` blocks already applied |

Both layers handle the same HTML5-lite quirks: BOM stripping, `<?xml?>` and `<!DOCTYPE>` skipping, void elements (`<br>`, `<img>`, `<meta>` …), entity decoding, and `<style>` text extraction.

## Parsing into an XML tree

`parse_xml_string` consumes a string and returns the document's child list — the parser tolerates multiple root nodes so you can paste a fragment without an enclosing element.

```rust,no_run
use azul_layout::xml::{parse_xml_string, XmlNodeChild, XmlNode};

let src = r#"
    <article id="p1" class="post">
      <h1>Hello</h1>
      <p>First &amp; only paragraph.</p>
    </article>
"#;

let children: Vec<XmlNodeChild> = parse_xml_string(src).unwrap();
assert_eq!(children.len(), 1);
match &children[0] {
    XmlNodeChild::Element(node) => {
        assert_eq!(node.node_type.as_str(), "article");
        assert_eq!(node.children.as_ref().len(), 2); // <h1>, <p>
    }
    _ => unreachable!(),
}
```

`XmlNode` (`core/src/xml.rs:60`) carries the tag name, an attribute map (`StringPairVec`), and an ordered list of `XmlNodeChild`. A child is either another `Element` or a `Text` leaf. The parser decodes the standard XML entities (`&lt;`, `&gt;`, `&amp;`, `&apos;`, `&quot;`, `&nbsp;`, plus numeric and hex character references) on the way in; you read decoded UTF-8 strings.

```rust,no_run
use azul_layout::xml::{parse_xml_string, XmlNodeChild};

let xml = "<a href=\"/foo?x=1&amp;y=2\">link</a>";
let children = parse_xml_string(xml).unwrap();
if let XmlNodeChild::Element(a) = &children[0] {
    let href = a.attributes.as_ref().iter()
        .find(|p| p.key.as_str() == "href")
        .map(|p| p.value.as_str())
        .unwrap();
    assert_eq!(href, "/foo?x=1&y=2");
}
```

`parse_xml(s)` is a thin wrapper that returns `Xml { root: ... }` — the same data, packaged so you can call resource-scanning helpers on it directly (see below).

## HTML5-lite quirks

The tokenizer is strict XML, but the tree-builder layer adds enough HTML5 leniency that most real-world fragments parse without an `xmlns` declaration or strict closing rules.

| Behaviour | Triggered by | Example |
|---|---|---|
| Void elements auto-close | tag name ∈ `area, base, br, col, embed, hr, img, input, link, meta, param, source, track, wbr` | `<br>` ≡ `<br/>` |
| Auto-close on conflict | `<li>` before `<li>`, `<p>` before block-level, `<td>/<tr>/<th>` siblings, `<dt>/<dd>`, `<option>/<optgroup>` | `<p>a<p>b</p>` parses as two siblings |
| Mismatched close tags | walks the tag stack to find the matching opener, pops everything between | `<a><b></a>` closes both |
| BOM, `<?xml?>`, `<!DOCTYPE …>`, leading `<!-- … -->` | skipped before tokenization | `\u{FEFF}<!DOCTYPE html>\n<x/>` parses fine |

The auto-close rules live in `layout/src/xml/mod.rs:700-744` and apply only to the tree-building path; `parse_xml_to_fast_dom_with_css` (the FastDom path) honours void elements but does *not* auto-close conflicting parents.

## Parsing directly into a StyledDom

`parse_xml_to_styled_dom` skips the intermediate `XmlNode` tree. It feeds tokenizer events straight into a `CompactDomBuilder`, collects every `<style>` block as CSS, runs the cascade, and returns a `StyledDom`. This is the path the layout engine uses for `.azul` files; on a typical document it allocates ~3–5× less than the two-step parse + `str_to_dom` path.

```rust,no_run
use azul_layout::xml::parse_xml_to_styled_dom;

let html = r#"
    <html>
      <head>
        <style>
          body { background-color: white; }
          .big { font-size: 32px; color: #2563eb; }
        </style>
      </head>
      <body>
        <p class="big">Headless render target.</p>
      </body>
    </html>
"#;

let styled = parse_xml_to_styled_dom(html).unwrap();
let node_count = styled.node_hierarchy.as_ref().len();
println!("parsed {} nodes", node_count);
```

The path drops everything inside `<head>` from the DOM (it is for stylesheet collection only), keeps `<body>` and its children, and applies the CSS via the standard cascade. Inline `style="…"` attributes are also parsed — the same way `azul_css::parser2::parse_css_declaration` parses them when set from Rust code.

Set `AZUL_MEM_BREAKDOWN=1` in the environment to print per-phase RSS and timing for the three sub-passes (tokenize+fast_dom, css attach, cascade). This is the same instrumentation the layout window uses and is the fastest way to localize a parse-time regression.

## Errors

Parse errors carry line and column positions. The top-level enum is `XmlError` (`core/src/xml.rs:941`), which wraps tokenizer errors (`XmlParseError`), structural errors (`MalformedHierarchy`, `UnexpectedCloseTag`, `NoRootNode`), and a few resource-limit variants (`NodesLimitReached`, `AttributesLimitReached`).

```rust,no_run
use azul_layout::xml::{parse_xml_string, XmlError};

let bad = "<a><b></a>"; // mismatched close
match parse_xml_string(bad) {
    Ok(_) => {} // tolerated by HTML5-lite walking
    Err(XmlError::MalformedHierarchy(e)) => {
        eprintln!("expected </{}>, got </{}>", e.expected, e.got);
    }
    Err(other) => eprintln!("{}", other),
}
```

Every error implements `Display` and prints in `line N:M` form — paste the message into an editor's go-to-line dialog and you land on the offending byte.

## Scanning external resources

`Xml::scan_external_resources` (`core/src/xml.rs:323`) walks the parsed document and returns every URL it would need to fetch to render the page: `<img src>`, `<img srcset>`, `<link href>`, `<script src>`, `<video|audio|source src>`, `<a href>` when the URL has a resource-like extension, deprecated `background=`, plus `url(…)` and `@import` inside `<style>` blocks and inline `style=` attributes.

```rust,no_run
use azul_layout::xml::{parse_xml, ExternalResourceKind};

let html = r#"
    <html>
      <head>
        <link rel="stylesheet" href="/site.css">
        <style>@import "/print.css";</style>
      </head>
      <body>
        <img src="/banner.png" srcset="/banner@2x.png 2x">
        <a href="/manual.pdf">manual</a>
      </body>
    </html>
"#;

let doc = parse_xml(html).unwrap();
let resources = doc.scan_external_resources();

for r in resources.as_ref() {
    println!("{:?} {} (from <{} {}>)",
        r.kind, r.url.as_str(),
        r.source_element.as_str(),
        r.source_attribute.as_str());
}
```

The classification heuristics live in `core/src/xml.rs:662-720`: extension first, then a `category` hint derived from the source element. Use it to bundle assets, prefetch fonts, or audit a third-party fragment before letting your renderer load anything from the network.

## Components

> **WIP** — the `ComponentMap` API in `core/src/xml.rs:2172` replaces the older `XmlComponentTrait`; the migration is partial. Treat names below as load-bearing but signatures as moving targets.

The XML format supports custom tags (`<my-card title="hi">…</my-card>`) that expand to user-defined components. Components are registered in a `ComponentMap`; each `ComponentId` is a `(collection, name)` pair so you can scope sets — `builtin:div`, `shadcn:avatar`, `myproject:my-card`. The parser does not instantiate components on its own; that step happens in `domxml_from_str` and is consumed by tooling like the `.azul` compile pipeline.

For the standalone library use cases on this page (parsing, validation, resource scraping), the component layer can be ignored — call `parse_xml_string` or `parse_xml_to_styled_dom` directly and you get raw HTML semantics with no custom expansion.

## When to choose which entry point

- Need a generic XML tree to walk, mutate, or serialize → `parse_xml_string` / `parse_xml`.
- Need to render the document headlessly or feed it to layout → `parse_xml_to_styled_dom`.
- Need both styled output *and* the original tree — parse twice, or call `parse_xml_string` and convert via `domxml_from_str` (slower; allocates the intermediate tree).
- Need to scan a document's outbound URLs → `parse_xml` then `Xml::scan_external_resources`.
