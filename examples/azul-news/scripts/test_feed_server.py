#!/usr/bin/env python3
"""Tests of the local feed server AzNews' end-to-end test talks to (no AzNews needed):

    python3 examples/azul-news/scripts/test_feed_server.py
"""

import os
import sys
import threading
import unittest
import urllib.error
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import feed_server  # noqa: E402


class FeedServerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server, cls.site = feed_server.make_server(0)
        cls.base = "http://127.0.0.1:%d" % cls.server.server_port
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def get(self, path, headers=None, method="GET"):
        request = urllib.request.Request(self.base + path, headers=headers or {}, method=method)
        try:
            with urllib.request.urlopen(request, timeout=5) as response:
                return response.status, dict(response.headers), response.read()
        except urllib.error.HTTPError as e:
            return e.code, dict(e.headers), e.read()

    def test_the_feed_has_validators_and_answers_304_when_they_come_back(self):
        status, headers, body = self.get("/feed.xml")
        self.assertEqual(status, 200)
        self.assertIn(b"<rss version=\"2.0\">", body)
        etag, last_modified = headers["ETag"], headers["Last-Modified"]
        status, _, body = self.get("/feed.xml", {"If-None-Match": etag})
        self.assertEqual(status, 304)
        self.assertEqual(body, b"")
        status, _, _ = self.get("/feed.xml", {"If-Modified-Since": last_modified})
        self.assertEqual(status, 304)
        self.assertTrue(any("inm=%s" % etag in r and r.endswith("304") for r in self.site.requests))

    def test_a_bump_is_a_new_article_and_a_new_etag(self):
        _, headers, body = self.get("/feed.xml")
        before = body.count(b"<item>")
        status, _, _ = self.get("/bump", method="POST")
        self.assertEqual(status, 200)
        status, after_headers, after = self.get("/feed.xml", {"If-None-Match": headers["ETag"]})
        self.assertEqual(status, 200, "the old ETag no longer matches")
        self.assertEqual(after.count(b"<item>"), before + 1)
        self.assertNotEqual(after_headers["ETag"], headers["ETag"])

    def test_every_article_has_a_topic(self):
        body = self.get("/feed.xml")[2]
        self.assertEqual(body.count(b"<category>"), body.count(b"<item>"))
        self.assertIn(b"<category>Odd news</category>", body)
        self.assertIn(b"<category>Even news</category>", body)

    def test_the_other_resources(self):
        self.assertEqual(self.get("/missing")[0], 404)
        status, headers, body = self.get("/img/red.png")
        self.assertEqual(status, 200)
        self.assertTrue(body.startswith(b"\x89PNG\r\n\x1a\n"))
        page = self.get("/page.html")[2]
        self.assertEqual(page.count(b"rel=\"alternate\""), 4)
        self.assertIn(b"jsonfeed.org/version/1.1", self.get("/feed.json")[2])
        self.assertIn(b"<feed xmlns=\"http://www.w3.org/2005/Atom\"", self.get("/atom.xml")[2])
        broken = self.get("/broken.xml")[2]
        self.assertIn(b"&y=2", broken, "a bare ampersand, on purpose")
        opml = self.get("/subs.opml")[2]
        self.assertEqual(opml.count(b"xmlUrl="), 4, "three feeds, one of them twice")


if __name__ == "__main__":
    unittest.main(verbosity=2)
