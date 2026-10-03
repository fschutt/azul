#!/usr/bin/env python3
"""AzDashboard end to end, headless, over the debug server: azul's DataTable
over 500,000 generated orders x 25 columns.

    1. starts AzDashboard (AZ_BACKEND=headless, AZ_DEBUG=--debug-port), waits for the
       orders (AZDASH_READY 500000, generated on a Thread) and the table;
    2. SORT: a click on the "Customer" header sorts the 500,000 rows off the UI thread
       (AZDASH_SORT, then AZDASH_KEYS / AZDASH_SHOWN when the order arrives) - the first
       names shown ascend; a second click descends; End scrolls to the last columns and a
       click on "Sales" sorts by a number, descending after a second click;
    3. FILTER: Ctrl/Cmd+F on the Region column, "=Europe" typed - a sixth of the rows
       stay (AZDASH_SHOWN); a Quantity range "18..20" narrows it further;
    4. EDIT: F2 on a Quantity cell, "0" is refused by the app (AZDASH_REFUSED, the edit
       stays open), "7" is kept (AZDASH_EDIT);
    5. SCROLL TO THE END: "Clear filters", Ctrl/Cmd+End - the last page of 500,000 rows
       (AZDASH_TOP near 500,000), PageUp, Ctrl/Cmd+Home back to the top;
    6. a screenshot after each step, flat light; the mode switched to dark at the end.

Usage (after building libazul with the debug server and AzDashboard; ONE app at a time,
through scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azdashboard_e2e.py [--bin target/release/AzDashboard]
        [--debug-port 8783] [--timeout 240] [--out <dir>] [--keep]
"""

import os
import re

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azdash"
ROWS = 500000
UP = "▲"
DOWN = "▼"


def number(text):
    """1,234.56 -> 1234.56"""
    return float(text.replace(",", "").strip())


def shown(app):
    """The last AZDASH_SHOWN: (shown, rows)."""
    last = app.last("AZDASH_SHOWN")
    if not last:
        return None
    a, b = last.split()
    return int(a), int(b)


def wait_order(app, what, before):
    """Waits for an AZDASH_SHOWN line printed after `before` lines (the order of a new
    sort or filter arrived) and returns (shown, rows) and the AZDASH_KEYS line."""
    app.until(what, lambda: len(app.printed("AZDASH_SHOWN")) > before)
    return shown(app), app.last("AZDASH_KEYS")


def keys_of(line):
    """'Customer: a | b | c' -> ['a', 'b', 'c']"""
    if not line or ":" not in line:
        raise Failure("no AZDASH_KEYS line: %r" % line)
    return [k.strip() for k in line.split(":", 1)[1].split("|")]


def body(args, logs, out):
    binary = e2e.find_binary("AzDashboard", args.bin, "AZDASHBOARD_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--data-dir", data_dir, "--size", "1280x800", "--theme", "flat",
                                "--mode", "light"],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("the orders", lambda: app.printed("AZDASH_READY", r"\d+"))
        rows = int(app.printed("AZDASH_READY", r"\d+")[-1])
        if rows != ROWS:
            raise Failure("the dashboard generated %d orders, not %d" % (rows, ROWS))
        app.frame(3)
        if not app.has_id("__azdash_orders"):
            raise Failure("the orders table (#__azdash_orders) is not in the tree")
        if not app.shows("SO-0000001"):
            raise Failure("the first order is not shown")
        app.screenshot(os.path.join(out, "1-orders.png"))

        # ---- sort by a text column, then descending ----
        n = len(app.printed("AZDASH_SHOWN"))
        app.click(text="Customer")
        app.until("the sort to start", lambda: app.last("AZDASH_SORT") == "Sorted by Customer " + UP)
        (count, total), keys = wait_order(app, "the customers sorted", n)
        if (count, total) != (ROWS, ROWS):
            raise Failure("a sort shows every row: %r" % ((count, total),))
        names = [k.lower() for k in keys_of(keys)]
        if names != sorted(names):
            raise Failure("the first customers do not ascend: %r" % names)
        app.log("sorted by Customer: %s" % keys)
        n = len(app.printed("AZDASH_SHOWN"))
        app.click(text="Customer " + UP)
        (count, _), keys = wait_order(app, "the customers sorted descending", n)
        names = [k.lower() for k in keys_of(keys)]
        if names != sorted(names, reverse=True):
            raise Failure("the first customers do not descend: %r" % names)
        app.log("sorted by Customer descending: %s" % keys)
        app.screenshot(os.path.join(out, "2-sorted-customer.png"))

        # ---- sort by a number column (scrolled into view) ----
        # A click on the table focuses it (a cell in the middle of it).
        app.click(selector="#__azdash_orders")
        app.key("end")
        app.frame(2)
        n = len(app.printed("AZDASH_SHOWN"))
        app.click(text="Sales")
        (count, _), keys = wait_order(app, "the sales sorted", n)
        n = len(app.printed("AZDASH_SHOWN"))
        app.click(text="Sales " + UP)
        (count, _), keys = wait_order(app, "the sales sorted descending", n)
        sales = [number(k) for k in keys_of(keys)]
        if sales != sorted(sales, reverse=True):
            raise Failure("the first sales do not descend: %r" % sales)
        app.log("sorted by Sales descending: %s" % keys)
        app.screenshot(os.path.join(out, "3-sorted-sales.png"))

        # ---- filter: equals on a text column, a range on a number column ----
        app.key("home", primary=True)
        app.key("home")
        for _ in range(3):
            app.key("right")
        app.key("f", primary=True)
        n = len(app.printed("AZDASH_SHOWN"))
        app.must("text_input", text="=Europe")
        app.frame(2)
        (count, total), keys = wait_order(app, "the Europe filter", n)
        if not (ROWS // 8 < count < ROWS // 4):
            raise Failure("=Europe should keep about a sixth of %d rows, kept %d" % (total, count))
        app.log("=Europe: %d of %d rows" % (count, total))
        app.key("return")
        for _ in range(10):
            app.key("right")
        app.key("f", primary=True)
        n = len(app.printed("AZDASH_SHOWN"))
        app.must("text_input", text="18..20")
        app.frame(2)
        (narrow, _), _ = wait_order(app, "the quantity range", n)
        if not (0 < narrow < count):
            raise Failure("18..20 should narrow %d rows, kept %d" % (count, narrow))
        app.log("=Europe and 18..20: %d rows" % narrow)
        app.key("return")
        app.screenshot(os.path.join(out, "4-filtered.png"))

        # ---- edit: the app refuses 0 and keeps 7 ----
        app.key("down")
        app.key("f2")
        for _ in range(4):
            app.key("backspace")
        app.must("text_input", text="0")
        app.frame(2)
        app.key("return")
        app.until("the refusal", lambda: app.last("AZDASH_REFUSED"))
        app.log("refused: %s" % app.last("AZDASH_REFUSED"))
        app.key("backspace")
        app.must("text_input", text="7")
        app.frame(2)
        app.key("return")
        app.until("the edit", lambda: app.last("AZDASH_EDIT"))
        edit = app.last("AZDASH_EDIT")
        if not re.match(r"^SO-\d{7} 13 7$", edit or ""):
            raise Failure("the kept edit reads %r, not '<order> 13 7'" % edit)
        app.log("kept: %s" % edit)
        app.screenshot(os.path.join(out, "5-edited.png"))

        # ---- clear the filters, scroll to the end and back ----
        n = len(app.printed("AZDASH_SHOWN"))
        app.click(text="Clear filters")
        (count, total), _ = wait_order(app, "every row again", n)
        if count != ROWS:
            raise Failure("clearing the filters shows %d rows, not %d" % (count, ROWS))
        app.click(selector="#__azdash_orders")
        seen = len(app.printed("AZDASH_TOP"))
        app.key("end", primary=True)
        app.until("the last page", lambda: len(app.printed("AZDASH_TOP")) > seen)
        top = int(app.last("AZDASH_TOP").split()[0])
        if top < ROWS - 100:
            raise Failure("Ctrl+End scrolled to row %d of %d" % (top, ROWS))
        app.log("the last page starts at row %d" % top)
        app.screenshot(os.path.join(out, "6-the-end.png"))
        app.key("pageup")
        app.key("home", primary=True)
        app.until("the top", lambda: (app.last("AZDASH_TOP") or "x").split()[0] == "0")
        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "7-dark.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8783)
