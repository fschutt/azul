#!/usr/bin/env python3
"""WIDGETS7 (DEDUP_WIDGETS_API F20): MessageList -> SummaryList, a mechanical rename.

The widget is a generic summary list (AzNotes lists notes with it), so its TYPES, its module
and its CSS classes lose the mail name. Field names (from / subject / unread / flagged) are
NOT renamed here - that is not mechanical (see the WIDGETS7 report).

Rules (plain substring replacements, in this order):
  MessageList  -> SummaryList     (MessageList, MessageListEvent(Kind), MessageListMark,
                                   MessageListOnEvent(+Callback, +CallbackType),
                                   OptionMessageListOnEvent, AzMessageListOnEvent*,
                                   AzApp_setMessageListOnEventCallbackInvoker, MessageListLook)
  MessageRow   -> SummaryRow      (MessageRow, MessageRowKind, MessageRowVec(+Slice, +Destructor,
                                   +DestructorType), OptionMessageRow)
  MESSAGE_LIST -> SUMMARY_LIST    (statics)
  __azul-native-message-list -> __azul-native-summary-list   (CSS classes, E2E selectors)
and, in the layout crate only (--layout): message_list -> summary_list, message-list ->
summary-list, message_rows -> summary_rows (module path, look fns, tests, class names). The apps'
own `message_list` functions / locals are not the widget's and are left alone.

Usage:
  python3 scripts/waves/wave7/widgets7_rename_summary_list.py --apps      # azul-mail, azul-notes,
                                                                          # their E2E scripts
  python3 scripts/waves/wave7/widgets7_rename_summary_list.py --api       # api.json (keys,
                                                                          # externals, fn bodies)
  python3 scripts/waves/wave7/widgets7_rename_summary_list.py --layout    # done by WIDGETS7
Each run is idempotent; it prints the files it changed.
"""
import os
import sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))

COMMON = [
    ("MessageList", "SummaryList"),
    ("MessageRow", "SummaryRow"),
    ("MESSAGE_LIST", "SUMMARY_LIST"),
    ("__azul-native-message-list", "__azul-native-summary-list"),
]
LAYOUT_ONLY = [
    ("message_list", "summary_list"),
    ("message-list", "summary-list"),
    ("message_rows", "summary_rows"),
]
API = [
    ("widgets::message_list::", "widgets::summary_list::"),
    ("themes::flat::message_list", "themes::flat::summary_list"),
    ("themes::flora::message_list", "themes::flora::summary_list"),
]

APPS = [
    "examples/azul-mail/src",
    "examples/azul-notes/src",
    "scripts/azmail_e2e.py",
    "scripts/aznotes_e2e.py",
]
LAYOUT = [
    "layout/src",
    "css/src/codegen/lower_types.rs",
    "examples/azul-widgets/src",
]


def files(paths):
    for p in paths:
        full = os.path.join(REPO, p)
        if os.path.isfile(full):
            yield full
        else:
            for root, _, names in os.walk(full):
                for n in names:
                    if n.endswith((".rs", ".py")):
                        yield os.path.join(root, n)


def apply(paths, rules):
    for f in files(paths):
        with open(f, encoding="utf-8") as h:
            text = h.read()
        new = text
        for a, b in rules:
            new = new.replace(a, b)
        if new != text:
            with open(f, "w", encoding="utf-8") as h:
                h.write(new)
            print("renamed in", os.path.relpath(f, REPO))


def main():
    args = set(sys.argv[1:])
    if not args:
        print(__doc__)
        return 2
    if "--layout" in args:
        apply(LAYOUT, COMMON + LAYOUT_ONLY)
    if "--apps" in args:
        apply(APPS, COMMON)
    if "--api" in args:
        path = os.path.join(REPO, "api.json")
        with open(path, encoding="utf-8") as h:
            text = h.read()
        new = text
        for a, b in API + COMMON:
            new = new.replace(a, b)
        if new != text:
            with open(path, "w", encoding="utf-8") as h:
                h.write(new)
            print("renamed in api.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
