#!/usr/bin/env python3
"""Lenient HTML (or XHTML) in, the strict XHTML azul's XML loader reads out.

azul's loader (`azul_layout::xml::parse_xml`) is an XML tokenizer with a few
HTML5 conveniences (void elements, some implied end tags). Test pages written
for browsers are HTML: unquoted and bare attributes, `<p>` left open, no
`<html>`/`<body>`, CSS inside `<![CDATA[ ]]>` in the .xht files, `<style>` in
the body, stylesheets pulled in with `<link>`. This module reads such a page
with the standard library's lenient `html.parser`, builds the tree the way an
HTML5 parser would for the markup these tests use, and writes:

    <html [attrs]><head><style>ALL THE CSS</style></head><body [attrs]>...</body></html>

What it does, in the order a browser would:
- tags and attribute names are lower-cased; entities are decoded (all of HTML's
  named ones, not just the six azul knows) and the text is re-escaped;
- `<title>`, `<meta>`, `<link>`, `<base>` and scripts are left out (they are
  metadata; `Document.links` and `Document.metas` keep what a caller needs);
- every `<style>` element, and every `<link rel=stylesheet>` a caller can
  resolve (`resolve_css`), is concatenated in document order into ONE head
  `<style>`, because azul only reads `<head><style>`;
- HTML mode only: implied end tags for `p`, `li`, `dt`/`dd`, `tr`, `td`/`th`,
  `option`, row groups; implied `<tbody>` for a `<tr>` directly in a `<table>`
  and implied `<tr>` for a cell directly in a row group; a stray `</p>` makes an
  empty `<p>`; `</br>` is a `<br>`; one newline right after `<pre>` is dropped;
- XHTML mode (.xht, .xhtml): no implied tags, `<x/>` is an empty element, and
  CDATA wrappers around CSS are removed.

Not a full HTML5 tree builder: no foster parenting, no adoption agency, no
foreign content. The WPT subset is chosen to not need them; `Document.issues`
records where the input needed something this module does not do.

Also used for the WPT editing data (`fragment_to_xhtml`) and as the stand-in
sanitizer of `mail_boxes.py --sanitizer normalize`.
"""

import html
import re
from html.parser import HTMLParser

VOID = {
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
    "param", "source", "track", "wbr", "keygen",
}
# Metadata elements that belong to the head.
HEAD_ELEMENTS = {"title", "meta", "link", "style", "base", "script", "noscript", "template"}
# Start tags that close an open <p> (HTML: "close a p element").
CLOSES_P = {
    "address", "article", "aside", "blockquote", "center", "details", "dialog", "dir", "div",
    "dl", "fieldset", "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5",
    "h6", "header", "hgroup", "hr", "li", "listing", "main", "menu", "nav", "ol", "p", "pre",
    "section", "summary", "table", "ul", "dd", "dt", "plaintext", "xmp",
}
# Elements whose content is raw text for html.parser (it already handles these).
RAW_TEXT = {"script", "style"}
# Scope boundaries: an implied end tag never closes past one of these.
SCOPE = {"table", "td", "th", "caption", "html", "body", "marquee", "object", "applet", "template"}
ROW_GROUPS = {"tbody", "thead", "tfoot"}


class Node:
    __slots__ = ("tag", "attrs", "children", "parent")

    def __init__(self, tag, attrs=None, parent=None):
        self.tag = tag
        self.attrs = list(attrs or [])
        self.children = []
        self.parent = parent

    def append(self, child):
        if isinstance(child, Node):
            child.parent = self
        self.children.append(child)
        return child

    def text(self):
        out = []
        for c in self.children:
            out.append(c if isinstance(c, str) else c.text())
        return "".join(out)


class Document:
    """The result of `parse()`."""

    def __init__(self):
        self.html = Node("html")
        self.body = Node("body", parent=self.html)
        # The <title> text (kept in the output: azul-doc's reftest report shows it).
        self.title = ""
        # (rel, href) of every <link>, in order; rel lower-cased and split.
        self.links = []
        # (name, content) of every <meta name=...>.
        self.metas = []
        # CSS chunks in document order: ("inline", text) or ("href", url).
        self.css = []
        self.has_script = False
        # Things this reader could not model (for skip decisions).
        self.issues = []

    def root_classes(self):
        for k, v in self.html.attrs:
            if k == "class" and v:
                return v.split()
        return []

    def stylesheet_text(self, resolve_css=None):
        """Every stylesheet, in order. `resolve_css(href)` returns CSS text or None."""
        parts = []
        for kind, value in self.css:
            if kind == "inline":
                parts.append(value)
            elif resolve_css is not None:
                text = resolve_css(value)
                if text is None:
                    self.issues.append("unresolved stylesheet " + value)
                else:
                    parts.append(text)
            else:
                self.issues.append("unresolved stylesheet " + value)
        return "\n".join(p.strip("\n") for p in parts if p.strip())

    def to_xhtml(self, resolve_css=None):
        css = self.stylesheet_text(resolve_css)
        # The XHTML namespace: a browser reading the file as XML (Chrome, for a
        # .xht in `azul-doc reftest`) styles the elements only when they are in
        # it; azul's loader ignores the attribute.
        html_attrs = list(self.html.attrs)
        if not any(k == "xmlns" for k, _ in html_attrs):
            html_attrs.insert(0, ("xmlns", "http://www.w3.org/1999/xhtml"))
        out = ["<html", attrs_xml(html_attrs), ">\n<head>\n"]
        if self.title:
            out.append("<title>" + escape_text(self.title) + "</title>\n")
        for name, content in self.metas:
            if name in ("assert", "flags"):
                out.append('<meta name="%s" content="%s"/>\n' % (name, escape_attr(content)))
        if css:
            out.append("<style>\n")
            out.append(escape_text(css))
            out.append("\n</style>\n")
        out.append("</head>\n<body")
        out.append(attrs_xml(self.body.attrs))
        out.append(">")
        for c in self.body.children:
            serialize(c, out)
        out.append("</body>\n</html>\n")
        return "".join(out)


def escape_text(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def escape_attr(s):
    return (s.replace("&", "&amp;").replace("<", "&lt;").replace('"', "&quot;")
            .replace("\n", "&#10;").replace("\t", "&#9;"))


def attrs_xml(attrs):
    seen = set()
    out = []
    for k, v in attrs:
        if k in seen or not re.fullmatch(r"[A-Za-z_][\w.\-]*", k):
            continue
        seen.add(k)
        out.append(' %s="%s"' % (k, escape_attr("" if v is None else v)))
    return "".join(out)


def serialize(node, out, with_attrs=True):
    if isinstance(node, str):
        out.append(escape_text(node))
        return
    out.append("<" + node.tag + (attrs_xml(node.attrs) if with_attrs else ""))
    if node.tag in VOID:
        out.append("/>")
        return
    out.append(">")
    for c in node.children:
        serialize(c, out, with_attrs)
    out.append("</" + node.tag + ">")


def _strip_cdata(css):
    css = css.strip()
    css = re.sub(r"^\s*(/\*\s*)?<!\[CDATA\[(\s*\*/)?", "", css)
    css = re.sub(r"(/\*\s*)?\]\]>(\s*\*/)?\s*$", "", css)
    return css


class _TreeBuilder(HTMLParser):
    def __init__(self, xhtml):
        super().__init__(convert_charrefs=True)
        self.xhtml = xhtml
        self.doc = Document()
        self.in_body = False
        self.stack = [self.doc.body]
        self.raw_tag = None  # inside <style>/<script>: the tag, collecting text
        self.raw_text = []
        self.head_title = False
        self.pre_newline = False

    # -- helpers ---------------------------------------------------------
    @property
    def cur(self):
        return self.stack[-1]

    def open_names(self):
        return [n.tag for n in self.stack]

    def in_scope(self, tag, boundaries=SCOPE):
        for n in reversed(self.stack[1:]):
            if n.tag == tag:
                return True
            if n.tag in boundaries:
                return False
        return False

    def pop_until(self, tag):
        while len(self.stack) > 1:
            n = self.stack.pop()
            if n.tag == tag:
                return

    def enter_body(self):
        self.in_body = True

    # -- html.parser callbacks -------------------------------------------
    def handle_starttag(self, tag, attrs):
        self._start(tag, attrs, self_closing=False)

    def handle_startendtag(self, tag, attrs):
        self._start(tag, attrs, self_closing=True)

    def _start(self, tag, attrs, self_closing):
        doc = self.doc
        attrs = [(k.lower(), v) for k, v in attrs]
        if tag == "html":
            doc.html.attrs.extend(a for a in attrs if a[0] not in dict(doc.html.attrs))
            return
        if tag == "head":
            return
        if tag == "body":
            doc.body.attrs.extend(a for a in attrs if a[0] not in dict(doc.body.attrs))
            self.enter_body()
            return
        if tag in ("style", "script"):
            if tag == "script":
                doc.has_script = True
            if not self_closing:
                self.raw_tag = tag
                self.raw_text = []
            return
        if tag == "link":
            d = dict(attrs)
            rels = (d.get("rel") or "").lower().split()
            href = (d.get("href") or "").strip()
            for rel in rels:
                doc.links.append((rel, href))
            if "stylesheet" in rels and "alternate" not in rels and href:
                doc.css.append(("href", href))
            return
        if tag == "meta":
            d = dict(attrs)
            if d.get("name"):
                doc.metas.append((d["name"].lower(), d.get("content") or ""))
            return
        if tag in ("title", "base", "noscript", "template"):
            if tag == "title" and not self_closing:
                self.raw_tag = "title"
                self.raw_text = []
            if tag in ("noscript", "template"):
                doc.issues.append("<%s>" % tag)
            return
        if not self.in_body:
            self.enter_body()

        if not self.xhtml:
            self._implied_end_tags(tag)
            # implied row group / row
            if tag == "tr" and self.cur.tag == "table":
                self.stack.append(self.cur.append(Node("tbody")))
            if tag in ("td", "th"):
                if self.cur.tag == "table":
                    self.stack.append(self.cur.append(Node("tbody")))
                if self.cur.tag in ROW_GROUPS:
                    self.stack.append(self.cur.append(Node("tr")))
            if tag == "col" and self.cur.tag == "table":
                self.stack.append(self.cur.append(Node("colgroup")))

        node = self.cur.append(Node(tag, attrs))
        if tag in VOID or (self_closing and self.xhtml):
            return
        self.stack.append(node)
        if tag in ("pre", "listing", "textarea") and not self.xhtml:
            self.pre_newline = True

    def _implied_end_tags(self, tag):
        names = self.open_names()
        if tag in CLOSES_P and self.in_scope("p", SCOPE | {"button"}):
            self.pop_until("p")
        if tag == "li" and self.in_scope("li", SCOPE | {"ul", "ol"}):
            self.pop_until("li")
        if tag in ("dt", "dd"):
            for t in ("dt", "dd"):
                if self.in_scope(t, SCOPE | {"dl"}):
                    self.pop_until(t)
        if tag in ("option", "optgroup") and "option" in names:
            self.pop_until("option")
        if tag in {"td", "th", "tr"} | ROW_GROUPS:
            for t in ("td", "th"):
                if self.in_scope(t, {"table"}):
                    # close the cell (and whatever is still open in it)
                    self.pop_until(t)
        if tag in {"tr"} | ROW_GROUPS and self.in_scope("tr", {"table"}):
            self.pop_until("tr")
        if tag in ROW_GROUPS:
            for t in ROW_GROUPS:
                if self.in_scope(t, {"table"}):
                    self.pop_until(t)
        if tag == "caption" and self.in_scope("caption", {"table"}):
            self.pop_until("caption")

    def handle_endtag(self, tag):
        if self.raw_tag == tag:
            text = "".join(self.raw_text)
            if tag == "title":
                self.doc.title = " ".join(text.split())
            if tag == "style":
                if self.xhtml:
                    text = _strip_cdata(text)
                    if "&" in text:
                        text = html.unescape(text)
                self.doc.css.append(("inline", text))
            self.raw_tag = None
            self.raw_text = []
            return
        if tag in ("html", "head", "body"):
            return
        if tag in ("style", "script", "title", "link", "meta", "base", "noscript", "template"):
            return
        if tag == "br" and not self.xhtml:
            self.cur.append(Node("br"))
            return
        if tag == "p" and not self.xhtml and not self.in_scope("p", SCOPE | {"button"}):
            self.cur.append(Node("p"))
            return
        if tag in VOID:
            return
        if tag in self.open_names()[1:]:
            self.pop_until(tag)
        else:
            self.doc.issues.append("stray </%s>" % tag)

    def handle_data(self, data):
        if self.raw_tag:
            self.raw_text.append(data)
            return
        if not data:
            return
        if not self.in_body:
            if not data.strip():
                return
            self.enter_body()
        if self.pre_newline:
            self.pre_newline = False
            if data.startswith("\r\n"):
                data = data[2:]
            elif data.startswith("\n"):
                data = data[1:]
            if not data:
                return
        children = self.cur.children
        if children and isinstance(children[-1], str):
            children[-1] += data
        else:
            self.cur.append(data)

    def unknown_decl(self, data):
        if data.startswith("CDATA["):
            self.handle_data(data[len("CDATA["):])

    def handle_comment(self, data):
        pass

    def handle_decl(self, decl):
        pass

    def handle_pi(self, data):
        pass


def parse(source, xhtml=False):
    """Parse a whole page. `xhtml=True` for .xht / .xhtml files."""
    b = _TreeBuilder(xhtml)
    source = source.lstrip("﻿")
    if xhtml:
        source = re.sub(r"<\?xml[^>]*\?>", "", source, count=1)
    b.feed(source)
    b.close()
    if b.raw_tag == "style":
        b.doc.css.append(("inline", "".join(b.raw_text)))
    return b.doc


def fragment_to_xhtml(fragment, with_attrs=True):
    """The body content of an HTML fragment, serialized canonically (no
    wrapper): used for the editing data, whose markup is innerHTML.

    `with_attrs=False` is the form the editing runner compares in
    (`layout/tests/wpt/editing.rs` serializes azul's tree the same way):
    tags and text only, void elements as `<br/>`, text escaped as `&amp;`,
    `&lt;`, `&gt;`."""
    doc = parse("<body>" + fragment + "</body>", xhtml=False)
    out = []
    for c in doc.body.children:
        serialize(c, out, with_attrs)
    return "".join(out)


if __name__ == "__main__":
    import sys

    path = sys.argv[1]
    src = open(path, encoding="utf-8", errors="replace").read()
    d = parse(src, xhtml=path.endswith((".xht", ".xhtml")))
    sys.stdout.write(d.to_xhtml())
