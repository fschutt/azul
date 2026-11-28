#!/usr/bin/env python3
"""The agents share ONE GitHub token of 5000 REST calls an hour (it hit 0 on 2026-11-28 because runs were polled in loops). Nothing in scripts/ci polls the API:
a job downloads the artifact of a finished run ONCE (`gh run download`); status, logs and the listing of runs belong to scripts/remote.sh, which waits 180 s between looks
(python3 -I scripts/ci/test_no_api_polling.py)."""
import re
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
FORBIDDEN = re.compile(r"\bgh\s+(run\s+(view|watch|list)|api|pr\s+(checks|view)|workflow\s+view)\b")


def sources():
    for path in sorted(HERE.glob("*.sh")) + sorted(p for p in HERE.glob("*.py") if not p.name.startswith("test_")):
        yield path


class NoApiPollingTests(unittest.TestCase):
    def test_no_script_of_the_harness_reads_run_status_or_lists_runs(self):
        found = []
        for path in sources():
            for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
                code = line.split("#", 1)[0] if path.suffix == ".sh" else line
                if FORBIDDEN.search(code):
                    found.append(f"{path.name}:{number}: {line.strip()[:110]}")
        self.assertEqual(found, [], "scripts/ci must not poll the GitHub API (one shared token): use scripts/remote.sh")

    def test_a_job_downloads_an_artifact_once_and_never_in_a_loop(self):
        for path in sources():
            text = path.read_text(encoding="utf-8", errors="replace")
            if path.suffix != ".sh":
                continue
            for block in re.finditer(r"\b(while|until)\b.*?\bdone\b", text, re.S):
                self.assertNotIn("gh run download", block.group(0), f"{path.name}: gh run download inside a loop")

    def test_the_checker_itself_sees_a_poll(self):
        self.assertTrue(FORBIDDEN.search("  gh run view 123 --json status"))
        self.assertTrue(FORBIDDEN.search("while true; do gh api repos/x/y; done"))
        self.assertFalse(FORBIDDEN.search("gh run download 123 -n remote-Linux"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
