#!/usr/bin/env python3
"""A local web of feeds for AzNews' end-to-end test (scripts/aznews_e2e.py).

    python3 examples/azul-news/scripts/feed_server.py [--port 8790]

Serves, on 127.0.0.1 only:

    /feed.xml      RSS 2.0, with ETag and Last-Modified; a request that sends them back
                   (If-None-Match / If-Modified-Since) gets 304 Not Modified
    /atom.xml      Atom 1.0 (xml:base, html content)
    /feed.json     JSON Feed 1.1
    /broken.xml    a hand-written RSS feed that is not well-formed (a bare &, unclosed tags)
    /missing       404
    /page.html     a web page naming the feeds in <link rel="alternate"> (and one that is gone)
    /img/red.png   a small PNG (the articles' picture)
    /subs.opml     an OPML list of three of the feeds (one of them twice)
    POST /bump     a new article in /feed.xml (a new ETag)

Every request is printed as `REQUEST <method> <path> inm=<If-None-Match> ims=<If-Modified-Since>
-> <status>` so the test can see that a refresh asked conditionally.
"""

import argparse
import http.server
import json
import struct
import sys
import threading
import zlib


def png(width=8, height=8, rgb=(220, 40, 40)):
    """A tiny solid PNG."""
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + bytes(rgb) * width for _ in range(height))
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw))
            + chunk(b"IEND", b""))


class FeedSite:
    """What the server serves; `version` grows with every POST /bump."""

    def __init__(self):
        self.version = 1
        self.lock = threading.Lock()
        self.requests = []

    def etag(self):
        return '"aznews-v%d"' % self.version

    def last_modified(self):
        return "Wed, %02d Sep 2026 08:%02d:00 GMT" % (30, min(self.version, 59))

    def rss(self, base):
        items = []
        for n in range(self.version + 2, 0, -1):
            items.append(
                "<item><title>Local article %d</title><link>%s/posts/%d</link>"
                "<guid isPermaLink=\"false\">local-%d</guid>"
                "<pubDate>Wed, 30 Sep 2026 %02d:00:00 GMT</pubDate>"
                "<description><![CDATA[<p>Article %d is about local feeds.</p>"
                "<p><img src=\"/img/red.png\" alt=\"A red square\"></p>"
                "<p>More text so the reader has something to show.</p>]]></description></item>"
                % (n, base, n, n, min(n, 23), n))
        return ("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\"><channel>"
                "<title>Local Test Feed</title><link>%s/</link><description>Served by feed_server.py"
                "</description>%s</channel></rss>\n" % (base, "".join(items)))

    def atom(self, base):
        return ("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n"
                "<feed xmlns=\"http://www.w3.org/2005/Atom\" xml:base=\"%s/\">"
                "<title>Local Atom</title><link rel=\"alternate\" href=\"%s/\"/>"
                "<updated>2026-09-30T10:00:00Z</updated><id>urn:local:atom</id>"
                "<entry><title>Atom entry</title><id>urn:local:atom:1</id><link href=\"atom/1\"/>"
                "<updated>2026-09-30T10:00:00Z</updated>"
                "<content type=\"html\">&lt;p&gt;An Atom entry &amp;amp; its text.&lt;/p&gt;</content>"
                "</entry></feed>\n" % (base, base))

    def json_feed(self, base):
        return json.dumps({
            "version": "https://jsonfeed.org/version/1.1",
            "title": "Local JSON",
            "home_page_url": base + "/",
            "items": [{
                "id": "json-1",
                "url": base + "/json/1",
                "title": "A JSON item",
                "content_html": "<p>JSON Feed content.</p>",
                "date_published": "2026-09-30T09:00:00Z",
            }],
        })

    def broken(self, base):
        return ("<rss version=\"2.0\"><channel><title>Broken & Local</title><link>%s/</link>"
                "<item><title>Unclosed</title><link>%s/b?x=1&y=2</link>"
                "<description><p>Unescaped <b>markup<br></p></description></item>"
                "<item><title>Second</item></channel></rss>" % (base, base))

    def page(self, base):
        return ("<!DOCTYPE html><html><head><title>Local site</title>"
                "<link rel=\"alternate\" type=\"application/rss+xml\" title=\"Local RSS\" href=\"/feed.xml\">"
                "<link rel=\"alternate\" type=\"application/atom+xml\" title=\"Local Atom\" href=\"/atom.xml\">"
                "<link rel=\"alternate\" type=\"application/feed+json\" title=\"Local JSON\" href=\"/feed.json\">"
                "<link rel=\"alternate\" type=\"application/rss+xml\" title=\"Gone\" href=\"/missing\">"
                "</head><body><p>Welcome & read the feeds.</p></body></html>")

    def opml(self, base):
        return ("<?xml version=\"1.0\"?><opml version=\"2.0\"><head><title>Local</title></head><body>"
                "<outline text=\"Tech\">"
                "<outline type=\"rss\" text=\"Local Test Feed\" xmlUrl=\"%s/feed.xml\" htmlUrl=\"%s/\"/>"
                "<outline type=\"rss\" text=\"Local Atom\" xmlUrl=\"%s/atom.xml\"/>"
                "</outline>"
                "<outline type=\"rss\" text=\"Broken & Local\" xmlUrl=\"%s/broken.xml\"/>"
                "<outline type=\"rss\" text=\"Twice\" xmlUrl=\"%s/feed.xml\"/>"
                "</body></opml>\n" % (base, base, base, base, base))


def make_handler(site):
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def base(self):
            return "http://%s" % (self.headers.get("Host") or "127.0.0.1:%d" % self.server.server_port)

        def answer(self, status, body=b"", content_type="text/plain", headers=None):
            inm = self.headers.get("If-None-Match", "")
            ims = self.headers.get("If-Modified-Since", "")
            line = "REQUEST %s %s inm=%s ims=%s -> %d" % (self.command, self.path, inm or "-", ims or "-", status)
            with site.lock:
                site.requests.append(line)
            print(line, flush=True)
            self.send_response(status)
            for k, v in (headers or {}).items():
                self.send_header(k, v)
            if status != 304:
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            if status != 304 and self.command != "HEAD":
                self.wfile.write(body)

        def conditional(self, body, content_type):
            etag = site.etag()
            last_modified = site.last_modified()
            headers = {"ETag": etag, "Last-Modified": last_modified}
            if self.headers.get("If-None-Match") == etag or self.headers.get("If-Modified-Since") == last_modified:
                self.answer(304, headers=headers)
            else:
                self.answer(200, body.encode("utf-8"), content_type, headers)

        def do_GET(self):
            base = self.base()
            path = self.path.split("?", 1)[0]
            with site.lock:
                if path == "/feed.xml":
                    body = site.rss(base)
                elif path == "/atom.xml":
                    body = site.atom(base)
                elif path == "/feed.json":
                    body = site.json_feed(base)
                else:
                    body = None
            if path == "/feed.xml":
                self.conditional(body, "application/rss+xml; charset=utf-8")
            elif path == "/atom.xml":
                self.conditional(body, "application/atom+xml")
            elif path == "/feed.json":
                self.conditional(body, "application/feed+json")
            elif path == "/broken.xml":
                self.answer(200, site.broken(base).encode("utf-8"), "application/rss+xml")
            elif path == "/page.html" or path == "/":
                self.answer(200, site.page(base).encode("utf-8"), "text/html; charset=utf-8")
            elif path == "/img/red.png":
                self.answer(200, png(), "image/png")
            elif path == "/subs.opml":
                self.answer(200, site.opml(base).encode("utf-8"), "text/x-opml")
            else:
                self.answer(404, b"not found")

        def do_HEAD(self):
            self.do_GET()

        def do_POST(self):
            if self.path == "/bump":
                with site.lock:
                    site.version += 1
                    version = site.version
                self.answer(200, ("version %d" % version).encode("utf-8"))
            else:
                self.answer(404, b"not found")

    return Handler


def make_server(port=0):
    """A server on 127.0.0.1:`port` (0: any free port) and its site; serve with
    `server.serve_forever()` (on a thread in the tests)."""
    site = FeedSite()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), make_handler(site))
    return server, site


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--port", type=int, default=8790)
    args = parser.parse_args(argv)
    server, _site = make_server(args.port)
    print("FEED_SERVER http://127.0.0.1:%d" % server.server_port, flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
