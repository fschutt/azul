---
slug: xml
title: XML Parsing (Standalone)
language: en
canonical_slug: xml
audience: external
maturity: wip
guide_order: 230
topic_only: false
short_desc: Loading a Dom from an XML string
prerequisites: []
tracked_files:
  - core/src/xml.rs
  - layout/src/xml/mod.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# XML Parsing (Standalone)

> WIP. Public surface is stable. The component-instantiation pipeline (`<my-widget>` style custom tags via `ComponentMap`) is mid-migration; treat names as load-bearing but signatures as moving targets.

Azul ships an XML/HTML5-lite parser that runs without a window, a renderer, or any GPU dependency. Use it as a tooling library: validate `.azul` files in CI, scrape resource URLs from a static site, build a DOM for headless rendering, or feed XHTML into custom layout code.

The parser is exposed in two layers:

- Tree (`Xml::from_str`) returns an `Xml` value with a `root: Vec<XmlNodeChild>`. A generic XML tree to walk, mutate, or scan.
- Direct-to-DOM (`Dom::create_from_parsed_xml`) consumes an `XmlNode` and returns a `Dom` ready for layout.

Both layers handle the same HTML5-lite quirks: BOM stripping, `<?xml?>` and `<!DOCTYPE>` skipping, void elements (`<br>`, `<img>`, `<meta>`), entity decoding, and `<style>` text extraction.

## Parsing into an XML tree

`Xml::from_str` consumes a string and returns an `Xml` whose `root` is the document's child list. The parser tolerates multiple root nodes so you can paste a fragment without an enclosing element.

```rust,no_run
use azul::xml::{Xml, XmlNodeChild};

let src = r#"
    <article id="p1" class="post">
      <h1>Hello</h1>
      <p>First &amp; only paragraph.</p>
    </article>
"#;

let doc = Xml::from_str(src.into()).unwrap();
assert_eq!(doc.root.as_ref().len(), 1);
match &doc.root.as_ref()[0] {
    XmlNodeChild::Element(node) => {
        assert_eq!(node.node_type.as_str(), "article");
        assert_eq!(node.children.as_ref().len(), 2); // <h1>, <p>
    }
    _ => unreachable!(),
}
```

`XmlNode` carries the tag name (`node_type`), an attribute map (`StringPairVec`), and an ordered list of `XmlNodeChild`. A child is either another `Element` or a `Text` leaf. The parser decodes the standard XML entities (`&lt;`, `&gt;`, `&amp;`, `&apos;`, `&quot;`, `&nbsp;`, plus numeric and hex character references) on the way in. You read decoded UTF-8 strings.

```rust,no_run
use azul::xml::{Xml, XmlNodeChild};

let xml = "<a href=\"/foo?x=1&amp;y=2\">link</a>";
let doc = Xml::from_str(xml.into()).unwrap();
if let XmlNodeChild::Element(a) = &doc.root.as_ref()[0] {
    let href = a.attributes.as_ref().iter()
        .find(|p| p.key.as_str() == "href")
        .map(|p| p.value.as_str())
        .unwrap();
    assert_eq!(href, "/foo?x=1&y=2");
}
```

## Parsing into a Dom

`Dom::create_from_parsed_xml` takes one `XmlNode` and returns a `Dom`. Pull the root element from your `Xml` first:

```rust,ignore
use azul::xml::{Xml, XmlNodeChild};
use azul::dom::Dom;

let doc = Xml::from_str(html.into()).unwrap();
let root = doc.root.as_ref().iter().find_map(|c| match c {
    XmlNodeChild::Element(node) => Some(node.clone()),
    _ => None,
}).unwrap();

let dom: Dom = Dom::create_from_parsed_xml(root);
```rust

Errors during DOM construction surface as `DomXmlParseError`: `Xml`, `MalformedHierarchy`, `MultipleHtmlRootNodes`, `MultipleBodyNodes`, `NoHtmlNode`, `NoBodyInHtml`, `Component`, `Css`, `RenderDom`.

## HTML5-lite quirks

The tokenizer is strict XML, but the tree-builder layer adds enough HTML5 leniency that most real-world fragments parse without an `xmlns` declaration or strict closing rules.

- Void elements auto-close: `area`, `base`, `br`, `col`, `embed`, `hr`, `img`, `input`, `link`, `meta`, `param`, `source`, `track`, `wbr`. So `<br>` is the same as `<br/>`.
- Auto-close on conflict: `<li>` before `<li>`, `<p>` before block-level, `<td>`/`<tr>`/`<th>` siblings, `<dt>`/`<dd>`, `<option>`/`<optgroup>`. So `<p>a<p>b</p>` parses as two siblings.
- Mismatched close tags walk the tag stack to find the matching opener and pop everything between. `<a><b></a>` closes both.
- BOM, `<?xml?>`, `<!DOCTYPE …>`, and leading `<!-- … -->` are skipped before tokenization.

## Errors

Parse errors carry line and column positions. The top-level enum is `XmlError`. It wraps tokenizer errors, structural errors (`MalformedHierarchy`, `UnexpectedCloseTag`, `NoRootNode`), and a few resource-limit variants (`NodesLimitReached`, `AttributesLimitReached`).

```rust,no_run
use azul::xml::{Xml, XmlError};

let bad = "<a><b></a>"; // mismatched close
match Xml::from_str(bad.into()) {
    Ok(_) => {} // tolerated by HTML5-lite walking
    Err(XmlError::MalformedHierarchy(e)) => {
        eprintln!("expected </{}>, got </{}>", e.expected, e.got);
    }
    Err(other) => eprintln!("{:?}", other),
}
```

`MalformedHierarchyError` carries `expected` and `got`. `UnexpectedCloseTagError` carries `expected`, `actual`, and `pos`.

## Scanning external resources

`Xml::scan_external_resources` walks the parsed document and returns every URL it would need to fetch to render the page: `<img src>`, `<link href>`, `<script src>`, `<video|audio|source src>`, `<a href>` when the URL has a resource-like extension, plus `url(…)` and `@import` inside `<style>` blocks and inline `style=` attributes.

```rust,no_run
use azul::xml::{Xml, ExternalResourceKind};

let html = r#"
    <html>
      <head>
        <link rel="stylesheet" href="/site.css">
        <style>@import "/print.css";</style>
      </head>
      <body>
        <img src="/banner.png">
        <a href="/manual.pdf">manual</a>
      </body>
    </html>
"#;

let doc = Xml::from_str(html.into()).unwrap();
let resources = doc.scan_external_resources();

for r in resources.as_ref() {
    println!("{:?} {} (from <{} {}>)",
        r.kind, r.url.as_str(),
        r.source_element.as_str(),
        r.source_attribute.as_str());
}
```

`ExternalResource` carries `kind`, `mime_type`, `source_attribute`, `source_element`, and `url`. `ExternalResourceKind` is one of `Audio`, `Font`, `Icon`, `Image`, `Script`, `Stylesheet`, `Unknown`, `Video`. Use it to bundle assets, prefetch fonts, or audit a third-party fragment before letting your renderer load anything from the network.

## Components

> WIP. The component layer (`ComponentMap`, `ComponentId`) is the entry point for custom tags like `<my-card title="hi">…</my-card>`. The migration to it is partial.

Each `ComponentId` is a `(collection, name)` pair so you can scope sets: `builtin:div`, `myproject:my-card`. The parser doesn't instantiate components on its own. For the standalone library use cases on this page (parsing, validation, resource scraping), the component layer can be ignored: call `Xml::from_str` or `Dom::create_from_parsed_xml` directly and you get raw HTML semantics with no custom expansion.

## When to choose which entry point

- Need a generic XML tree to walk, mutate, or serialize. Use `Xml::from_str`.
- Need to render the document or feed it to layout. Use `Xml::from_str` then `Dom::create_from_parsed_xml` on the root.
- Need to scan a document's outbound URLs. Use `Xml::from_str` then `scan_external_resources`.

## Coming Up Next

- [Components](dom/components.md) — Reusable UI fragments - named functions of (args) -> Dom
- [Styling with CSS](styling.md) — Stylesheets, selectors, and the cascade
- [Document Object Model](dom.md) — The Dom tree - node types, hierarchy, and CSS
