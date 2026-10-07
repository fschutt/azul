#!/usr/bin/env python3
"""AzNews end to end, headless, over the debug server, against a local web of feeds.

    1. starts examples/azul-news/scripts/feed_server.py on a free port (RSS with ETag /
       Last-Modified and 304s, Atom, JSON Feed, a malformed feed, a web page with feed links, a
       picture) and AzNews (AZ_BACKEND=headless, AZ_DEBUG=--debug-port) on a fresh data folder
       with the server's OPML list as its file argument;
    2. the import preview shows the 3 feeds (one listed twice); Import subscribes them and
       refreshes: 3 AZNEWS_REFRESHED lines, news/subscriptions.opml and news/feeds/<id>/
       {feed,items,state}.json on disk, the malformed feed read anyway; the source list has a
       section per feed, the RSS feed's topics (its <category>s) under it: a topic shows its 2
       articles; the table sorts by the column header clicked (title, then date again);
    3. an article opens from the table (a VirtualView: its rows are a DOM of their own) in the
       reading pane (its text is in the window), "Load pictures" fetches its picture on a
       Thread into the image cache (AZNEWS_PICTURE), the toolbar's Star writes state.json;
    4. Refresh again: the server is asked with If-None-Match and answers 304, AzNews prints
       `304`; after a new article (POST /bump) a refresh brings exactly one new article;
    5. Add feed with the web page's address finds its 3 readable feeds; one is subscribed;
    6. the sources page: Export writes news/exports/subscriptions-<time>.opml; the Atom feed is
       no longer followed (AZNEWS_FOLLOW ... off, azPaused in the list) and Get News leaves it
       out; followed again it is refreshed; Mark all as read; the settings page switches to
       Flora / Dark; screenshots on the way;
    7. a second run with --sample on another fresh folder: 42 feeds, 891 articles, 127 files;
       42 closed source sections, one opened shows its 4 topics.
    8. a third run (--sample --screen feed, a fresh folder): the first feed's page; its name is
       changed, then the window is closed: the subscription list is written first
       (AZNEWS_SAVED news/subscriptions.opml) and the window closes by itself; AzNews starts
       again on the same folder and shows the new name.

AzNews' DOM ids carry its prefix `__aznews_` (examples/azul-news/src/ids.rs): `app.sel(stem)`.

Usage (after building libazul with the debug server and AzNews, one app at a time):

    python3 scripts/aznews_e2e.py [--bin target/release/AzNews] [--debug-port 8793]
        [--timeout 240] [--out <dir>] [--keep]
"""

import glob
import os
import re
import shutil
import sys
import threading
import time
import urllib.request

import azlin_e2e as e2e
from azlin_e2e import Failure

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "examples", "azul-news", "scripts"))
import feed_server  # noqa: E402

TAG = "aznews"


def feed_files(data_dir, name):
    return sorted(glob.glob(os.path.join(data_dir, "news", "feeds", "*", name)))


def feed_id(data_dir, url):
    """The azId the subscription list gives the feed at `url`."""
    with open(os.path.join(data_dir, "news", "subscriptions.opml"), encoding="utf-8") as f:
        opml = f.read()
    for outline in re.findall(r"<outline [^>]*>", opml):
        if 'xmlUrl="%s"' % url in outline:
            found = re.search(r'azId="([^"]+)"', outline)
            if found:
                return found.group(1)
    raise Failure("no subscription for %s in:\n%s" % (url, opml))


def library_run(args, logs, out, base, site):
    binary = e2e.find_binary("AzNews", args.bin, "AZNEWS_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    opml_path = os.path.join(logs, "subs.opml")
    with urllib.request.urlopen(base + "/subs.opml", timeout=5) as response, open(opml_path, "wb") as f:
        f.write(response.read())
    app = e2e.App(TAG, binary, ["--data-dir", data_dir, "--size", "1200x760", opml_path],
                  args.debug_port, logs, args.timeout)
    try:
        # 1-2: the import.
        app.until("the empty library", lambda: app.printed("AZNEWS_LOADED", r"\d+ \d+"))
        app.detect_naming("__aznews_", "toolbar-refresh")
        app.expect_line("AZNEWS_IMPORT_PREVIEW", "3", "the OPML list's 3 feeds (one listed twice)")
        app.frame(2)
        app.screenshot(os.path.join(out, "import-preview.png"))
        app.click(selector=app.sel("opml-run"))
        app.expect_line("AZNEWS_IMPORTED", "3")
        app.until("3 refreshed feeds", lambda: len(app.printed("AZNEWS_REFRESHED", r"\S+ \S+")) >= 3)
        app.until("the refresh's end", lambda: app.printed("AZNEWS_REFRESH_DONE", r"\d+"))
        refreshed = app.printed("AZNEWS_REFRESHED", r"\S+ \S+")
        if any(r.endswith(" error") for r in refreshed):
            raise Failure("a feed failed: %s" % refreshed)
        broken = feed_id(data_dir, base + "/broken.xml")
        if not any(r.startswith(broken + " ") and r.split()[1] != "0" for r in refreshed):
            raise Failure("the malformed feed should bring its articles: %s" % refreshed)
        app.until("the feed files", lambda: len(feed_files(data_dir, "items.json")) == 3)
        app.log("3 feeds imported and refreshed: %s" % refreshed)
        app.frame(2)
        app.screenshot(os.path.join(out, "list.png"))

        # 2b: the source list - a section per feed, the RSS feed's topics under it (3 feeds:
        # every section open); the table's sort.
        for i in range(3):
            if not app.has(app.sel("section-%d" % i)):
                raise Failure("the source list has no section %d" % i)
        app.click(selector=app.sel("topic-0-0"))
        app.expect_line("AZNEWS_VIEW", "2", "the topic \"Odd news\": local articles 3 and 1")
        app.click(selector=app.sel("sort-title"))
        app.expect_line("AZNEWS_SORTED", "title asc", "the table sorted by title")
        app.frame(2)
        app.screenshot(os.path.join(out, "topic-by-title.png"))
        app.click(selector=app.sel("sort-date"))
        app.expect_line("AZNEWS_SORTED", "date desc", "the table sorted newest first again")

        # 3: the reader, its picture, a star. The table's rows live in its VirtualView's DOM;
        # newest first, "Local article 3" is the topic's first article.
        app.until("the topic's rows", lambda: app.has(app.sel("article-0"), every_dom=True))
        app.click(selector=app.sel("article-0"), every_dom=True)
        app.until("the article", lambda: app.printed("AZNEWS_SELECTED", r".+"))
        if not app.printed("AZNEWS_SELECTED", r"\S+ local-3"):
            raise Failure("the first row should be local article 3: %s"
                          % app.printed("AZNEWS_SELECTED", r".+"))
        app.until("the article's text in the reader", lambda: app.shows("Article 3 is about local feeds."))
        if not app.has(app.sel("reader")):
            raise Failure("the reader's body is not laid out")
        app.click(text="Load pictures")
        app.until("the picture", lambda: app.printed("AZNEWS_PICTURE", re.escape(base + "/img/red.png")))
        app.frame(2)
        app.screenshot(os.path.join(out, "reader.png"))
        rss = feed_id(data_dir, base + "/feed.xml")
        state_path = os.path.join(data_dir, "news", "feeds", rss, "state.json")
        saved = app.count("AZNEWS_SAVED", re.escape("news/feeds/%s/state.json" % rss))
        app.click(selector=app.sel("toolbar-star"))
        app.until("the star on disk", lambda: app.count(
            "AZNEWS_SAVED", re.escape("news/feeds/%s/state.json" % rss)) > saved)
        with open(state_path, encoding="utf-8") as f:
            state = f.read()
        if '"local-3"' not in state.split('"starred"')[1]:
            raise Failure("local-3 is not starred in %s:\n%s" % (state_path, state))

        # 4: conditional refreshes.
        before = len(app.printed("AZNEWS_REFRESHED", r"\S+ \S+"))
        app.click(selector=app.sel("toolbar-refresh"))
        app.until("the second refresh", lambda: len(app.printed("AZNEWS_REFRESHED", r"\S+ \S+")) >= before + 3)
        if "%s 304" % rss not in app.printed("AZNEWS_REFRESHED", r"\S+ \S+")[before:]:
            raise Failure("the RSS feed was not answered 304: %s" % app.printed("AZNEWS_REFRESHED", r"\S+ \S+"))
        with site.lock:
            conditional = [r for r in site.requests if r.startswith("REQUEST GET /feed.xml inm=\"") and r.endswith("304")]
        if not conditional:
            raise Failure("the server never saw If-None-Match: %s" % site.requests)
        app.log("a refresh asks conditionally: %s" % conditional[-1])
        urllib.request.urlopen(urllib.request.Request(base + "/bump", method="POST"), timeout=5).read()
        before = len(app.printed("AZNEWS_REFRESHED", r"\S+ \S+"))
        app.click(selector=app.sel("toolbar-refresh"))
        app.until("the third refresh", lambda: len(app.printed("AZNEWS_REFRESHED", r"\S+ \S+")) >= before + 3)
        if "%s 1" % rss not in app.printed("AZNEWS_REFRESHED", r"\S+ \S+")[before:]:
            raise Failure("the new article did not come: %s" % app.printed("AZNEWS_REFRESHED", r"\S+ \S+"))

        # 5: Add feed from a web page.
        app.click(selector=app.sel("toolbar-add"))
        app.until("the Add feed form", lambda: app.has(app.sel("add-url")))
        app.text_input(app.sel("add-url"), base + "/page.html")
        app.click(selector=app.sel("add-find"))
        app.expect_line("AZNEWS_FOUND", "3", "the page's three readable feeds")
        app.frame(2)
        app.screenshot(os.path.join(out, "add-feed.png"))
        app.click(selector=app.sel("found-2"))
        app.click(selector=app.sel("add-subscribe"))
        app.until("the JSON feed's subscription",
                  lambda: app.printed("AZNEWS_SUBSCRIBED", r"\S+ " + re.escape(base + "/feed.json")))

        # 6: the sources page - export, stop following a feed, follow it again; mark all,
        # settings.
        app.click(selector=app.sel("toolbar-sources"))
        app.until("the sources page", lambda: app.has(app.sel("sources")))
        app.frame(2)
        app.screenshot(os.path.join(out, "sources.png"))
        app.click(selector=app.sel("sources-export"))
        exported = app.until("the export", lambda: app.printed("AZNEWS_EXPORTED", r"news/exports/\S+\.opml"))[-1]
        if not os.path.exists(os.path.join(data_dir, exported)):
            raise Failure("%s was not written" % exported)
        with open(os.path.join(data_dir, exported), encoding="utf-8") as f:
            if f.read().count("xmlUrl=") != 4:
                raise Failure("the export should list 4 feeds")
        atom = feed_id(data_dir, base + "/atom.xml")
        app.click(selector=app.sel("source-follow-1"))
        app.expect_line("AZNEWS_FOLLOW", "%s off" % atom, "the Atom feed no longer followed")
        app.until("azPaused in the list", lambda: 'azPaused="true"' in open(
            os.path.join(data_dir, "news", "subscriptions.opml"), encoding="utf-8").read())
        before = len(app.printed("AZNEWS_REFRESHED", r"\S+ \S+"))
        app.after("a refresh of the followed feeds", "AZNEWS_REFRESH_DONE", r"\d+",
                  lambda: app.click(selector=app.sel("toolbar-refresh")))
        asked = app.printed("AZNEWS_REFRESHED", r"\S+ \S+")[before:]
        if len(asked) != 3 or any(r.startswith(atom + " ") for r in asked):
            raise Failure("Get News should ask the 3 followed feeds, not the Atom one: %s" % asked)
        before = len(app.printed("AZNEWS_REFRESHED", r"\S+ \S+"))
        app.click(selector=app.sel("source-follow-1"))
        app.expect_line("AZNEWS_FOLLOW", "%s on" % atom, "the Atom feed followed again")
        app.until("the Atom feed refreshed once followed again", lambda: any(
            r.startswith(atom + " ") for r in app.printed("AZNEWS_REFRESHED", r"\S+ \S+")[before:]))
        app.click(selector=app.sel("toolbar-mark-all"))
        app.click(selector=app.sel("mark-all-yes"))
        app.until("mark all", lambda: app.printed("AZNEWS_MARKED_ALL", r"\d+"))
        app.key("comma", primary=True)
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.click(text="General")
        app.click(text="Flora")
        app.click(text="Dark")
        app.settle()
        app.screenshot(os.path.join(out, "settings-flora-dark.png"))
        app.click(selector="#appkit-settings-ok")  # OK keeps them (Escape would cancel)
        app.settle()
        app.screenshot(os.path.join(out, "flora-dark.png"))
        app.log("PASS (library)")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(data_dir, ignore_errors=True)


def sample_run(args, logs, out):
    binary = e2e.find_binary("AzNews", args.bin, "AZNEWS_BIN")
    data_dir = os.path.join(logs, "sample-data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG + "-sample", binary, ["--data-dir", data_dir, "--sample", "--size", "1200x760"],
                  args.debug_port, logs, args.timeout)
    try:
        app.expect_line("AZNEWS_LOADED", "42 891", "the sample library")
        app.expect_line("AZNEWS_SAMPLE_WRITTEN", "127", "the sample's files (the list + 3 per feed)")
        if len(feed_files(data_dir, "items.json")) != 42:
            raise Failure("expected 42 items.json files")
        app.frame(3)
        app.settle()
        app.screenshot(os.path.join(out, "sample.png"))
        # 42 sources: their sections start closed; one opened shows its folder's 4 topics.
        if app.has(app.sel("topic-0-0")):
            raise Failure("with 42 sources the sections should start closed")
        app.click(selector=app.sel("section-0"))
        app.until("the first source's topics", lambda: app.has(app.sel("topic-0-3")))
        app.click(selector=app.sel("topic-0-0"))
        app.until("a topic's articles", lambda: app.has(app.sel("article-0"), every_dom=True))
        app.frame(2)
        app.screenshot(os.path.join(out, "sample-topic.png"))
        app.log("PASS (sample)")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(data_dir, ignore_errors=True)


RENAMED = "E2E renamed"
SUBSCRIPTIONS = "news/subscriptions.opml"


def wait_exit(app, seconds=20):
    """True once the app's process ended by itself within `seconds`."""
    end = time.time() + seconds
    while app.process.poll() is None and time.time() < end:
        time.sleep(0.25)
    return app.process.poll() is not None


def rename_run(args, logs, out):
    """8: a feed renamed on its page is written when the window closes, and read back."""
    binary = e2e.find_binary("AzNews", args.bin, "AZNEWS_BIN")
    data_dir = os.path.join(logs, "rename-data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG + "-rename", binary,
                  ["--data-dir", data_dir, "--sample", "--screen", "feed", "--size", "1200x760"],
                  args.debug_port, logs, args.timeout)
    try:
        app.expect_line("AZNEWS_LOADED", "42 891", "the sample library")
        app.expect_line("AZNEWS_SAMPLE_WRITTEN", "127", "the sample's files")
        # The first feed's page (its name field).
        app.detect_naming("__aznews_", "feed-title")
        app.text_input(app.sel("feed-title"), RENAMED)
        app.until("the new name on the page", lambda: app.shows(RENAMED))
        saved = app.count("AZNEWS_SAVED", re.escape(SUBSCRIPTIONS))
        # Close: the renamed list is written first, then the window closes by itself.
        app.op("close")
        if not wait_exit(app):
            raise Failure("the window did not close after the subscription list was written")
        if app.count("AZNEWS_SAVED", re.escape(SUBSCRIPTIONS)) <= saved:
            raise Failure("the window closed without writing %s" % SUBSCRIPTIONS)
        with open(os.path.join(data_dir, "news", "subscriptions.opml"), encoding="utf-8") as f:
            if RENAMED not in f.read():
                raise Failure("%s does not have the new name" % SUBSCRIPTIONS)
        app.log("the close wrote the renamed feed first")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    app = e2e.App(TAG + "-rename-2", binary,
                  ["--data-dir", data_dir, "--screen", "feed", "--size", "1200x760"],
                  args.debug_port, logs, args.timeout)
    try:
        app.expect_line("AZNEWS_LOADED", "42 891", "the library read back")
        app.until("the new name after the restart", lambda: app.shows(RENAMED))
        app.frame(2)
        app.settle()
        app.screenshot(os.path.join(out, "renamed-after-restart.png"))
        app.log("PASS (rename)")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(data_dir, ignore_errors=True)


def body(args, logs, out):
    server, site = feed_server.make_server(0)
    base = "http://127.0.0.1:%d" % server.server_port
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        return (library_run(args, logs, out, base, site) and sample_run(args, logs, out)
                and rename_run(args, logs, out))
    finally:
        server.shutdown()
        server.server_close()
        with open(os.path.join(logs, "feed_server.log"), "w", encoding="utf-8") as f:
            f.write("\n".join(site.requests) + "\n")


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8793)
