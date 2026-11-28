#!/usr/bin/env python3
"""Tests of scripts/ci/android_azmail.py and scripts/ci/azmail_sample_data.py (python3 -I scripts/ci/test_android_azmail.py)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import android_azmail as a  # noqa: E402
import azmail_sample_data as d  # noqa: E402

REPO = Path(__file__).resolve().parents[2]

UI = (
    '<node text="Home" bounds="[0,130][200,190]"/>'
    '<node text="New E-mail" bounds="[0,200][300,260]"/>'
    '<node text="Garden Weekly: bulbs, frost and a sale" bounds="[0,600][1080,700]"/>'
    '<node content-desc="Re: Garden plan for October" bounds="[0,700][1080,800]"/>'
)


class LabelTests(unittest.TestCase):
    def test_a_subject_is_found_by_the_start_of_its_text(self):
        label, x, y = a.find_label(UI, a.SUBJECTS)
        self.assertEqual(label, "Garden Weekly: bulbs, frost and a sale")
        self.assertEqual((x, y), (540, 650))

    def test_the_exact_label_comes_before_a_substring(self):
        label, _x, _y = a.find_label(UI, ("New E-mail",))
        self.assertEqual(label, "New E-mail")

    def test_a_content_description_counts(self):
        self.assertEqual(a.find_label(UI, ("Re: Garden plan",))[0], "Re: Garden plan for October")

    def test_nothing_found_is_none(self):
        self.assertIsNone(a.find_label(UI, ("Spam",)))


class FolderTests(unittest.TestCase):
    def test_the_folder_is_the_one_the_app_logged(self):
        log = "I RustStdoutStderr: [azmail] 0 account(s) in /data/user/0/com.azul.AzMail/files/AzMail\n"
        self.assertEqual(a.mail_folder_of(log, "com.azul.AzMail"), "/data/user/0/com.azul.AzMail/files/AzMail")

    def test_the_last_line_wins_and_no_line_falls_back_to_the_files_directory(self):
        log = "[azmail] 0 account(s) in /a\n[azmail] 1 account(s) in /b\n"
        self.assertEqual(a.mail_folder_of(log, "p"), "/b")
        self.assertEqual(a.mail_folder_of("", "com.azul.AzMail"), "/data/user/0/com.azul.AzMail/files/AzMail")


class FakeDev:
    package = "com.azul.AzMail"

    def __init__(self):
        self.calls = []

    def adb(self, *args, timeout=90, binary=False):
        self.calls.append(("adb",) + args)
        return "1 file pushed, 0 skipped." if args and args[0] == "push" else ""

    def sh(self, command, timeout=90):
        self.calls.append(("sh", command))
        return "10123\n" if command.startswith("stat ") else "ada@example.org\n" if command.startswith("ls ") else ""


class SeedTests(unittest.TestCase):
    def test_the_account_folder_goes_into_the_apps_mail_folder_and_becomes_the_apps(self):
        with tempfile.TemporaryDirectory() as tmp:
            d.build(REPO, tmp)
            dev = FakeDev()
            real_sleep = a.time.sleep
            a.time.sleep = lambda s: None
            try:
                folder, note = a.seed(dev, Path(tmp) / "azmail-data", "/data/user/0/com.azul.AzMail/files/AzMail")
            finally:
                a.time.sleep = real_sleep
        self.assertEqual(folder, "/data/user/0/com.azul.AzMail/files/AzMail")
        pushes = [c for c in dev.calls if c[0] == "adb" and c[1] == "push"]
        self.assertEqual(len(pushes), 1)
        self.assertTrue(pushes[0][2].endswith("ada@example.org"))
        self.assertEqual(pushes[0][3], "/data/user/0/com.azul.AzMail/files/AzMail/")
        shells = [c[1] for c in dev.calls if c[0] == "sh"]
        self.assertIn("chown -R 10123:10123 /data/user/0/com.azul.AzMail/files/AzMail", shells)
        self.assertLess(shells.index("mkdir -p /data/user/0/com.azul.AzMail/files/AzMail"), shells.index("chown -R 10123:10123 /data/user/0/com.azul.AzMail/files/AzMail"))


class SampleDataTests(unittest.TestCase):
    def build(self):
        tmp = tempfile.mkdtemp(prefix="azmail-sample-test-")
        counts = d.build(REPO, tmp)
        return Path(tmp) / "azmail-data" / "ada@example.org", counts

    def test_the_folders_and_their_message_counts_are_those_of_the_sample_account(self):
        _root, counts = self.build()
        self.assertEqual(counts, {"inbox": 4, "sent": 1, "spam": 1, "drafts": 1, "Work": 0, "Work.Projects": 1})

    def test_the_account_file_is_the_version_1_imap_account_of_the_sample_server(self):
        root, _ = self.build()
        acc = json.loads((root / "account.json").read_text())
        self.assertEqual(acc["format"], "azmail.account")
        self.assertEqual(acc["version"], 1)
        self.assertEqual((acc["imap"], acc["smtp"]), ({"host": "localhost", "port": 1143}, {"host": "localhost", "port": 2525}))
        self.assertEqual((acc["security"], acc["auth"], acc["email"], acc["username"]), ("plain", "password", "ada@example.org", "ada@example.org"))

    def test_an_index_line_has_the_fields_of_the_rust_index_entry_and_its_file_is_there(self):
        root, _ = self.build()
        lines = (root / "mail" / "inbox" / "index.jsonl").read_text().splitlines()
        self.assertEqual(len(lines), 4)
        entry = json.loads(lines[1])
        self.assertEqual(set(entry), {"uid", "message_id", "date", "from", "to", "subject", "flags", "size", "path"})
        self.assertEqual(entry["subject"], "Garden Weekly: bulbs, frost and a sale")
        self.assertEqual(entry["flags"], [], "the inbox is unread")
        self.assertEqual(entry["path"], "mail/inbox/2026/09/2.eml")
        self.assertEqual((root / entry["path"]).stat().st_size, entry["size"])
        self.assertRegex(entry["date"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")

    def test_the_other_folders_are_filed_as_read(self):
        root, _ = self.build()
        entry = json.loads((root / "mail" / "sent" / "index.jsonl").read_text().splitlines()[0])
        self.assertEqual(entry["flags"], ["\\Seen"])

    def test_the_latin_1_message_subject_is_decoded(self):
        root, _ = self.build()
        subjects = [json.loads(l)["subject"] for l in (root / "mail" / "inbox" / "index.jsonl").read_text().splitlines()]
        self.assertIn("Grüße aus München", subjects)

    def test_a_folder_state_is_the_version_1_state_of_the_rust_store(self):
        root, _ = self.build()
        state = json.loads((root / "mail" / "Work.Projects" / "state.json").read_text())
        self.assertEqual((state["format"], state["version"], state["server_name"], state["last_uid"], state["messages"]), ("azmail.folder", 1, "Work/Projects", 1, 1))
        empty = json.loads((root / "mail" / "Work" / "state.json").read_text())
        self.assertEqual(empty["messages"], 0)
        self.assertEqual((root / "mail" / "Work" / "index.jsonl").read_text(), "")


if __name__ == "__main__":
    unittest.main(verbosity=1)
