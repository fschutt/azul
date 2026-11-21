#!/usr/bin/env python3
"""(Re)download the public email templates of tests/mail_corpus/ at pinned commits.

    python3 scripts/refci/fetch_mail_corpus.py

Every entry is MIT licensed (checked 2026-09-30 through the GitHub API and the
repository's LICENSE file, which is copied next to the templates). Skipped on
purpose:
- mjmlio/email-templates: the repository declares no licence;
- foundation/foundation-emails: MIT, but its templates are Inky source
  (<container>, <row>, ...) that only render after the Inky compiler (Node);
- hteumeuleu/email-bugs: no licence; linked from the report instead.

tests/mail_corpus/exploration/ holds the 8 synthetic mails of the AzMail
exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md 1.3); they are
azul's own and are not fetched.
"""

import os
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(os.path.dirname(HERE)), "tests", "mail_corpus")

# (directory, repository, commit, [(source path, file name)])
SOURCES = [
    ("cerberus", "emailmonday/Cerberus", "af3cec9dd12fbbea0c6e9a1b0bf82a0479726957", [
        ("cerberus-fluid.html", "cerberus-fluid.html"),
        ("cerberus-hybrid.html", "cerberus-hybrid.html"),
        ("cerberus-responsive.html", "cerberus-responsive.html"),
        ("LICENSE", "LICENSE"),
    ]),
    ("leemunroe", "leemunroe/responsive-html-email-template", "60b5b6ddf9600e4b7d7e1ef60e5a3a8f1e56e186", [
        ("email-inlined.html", "email-inlined.html"),
        ("license.txt", "LICENSE"),
    ]),
    ("mailgun", "mailgun/transactional-email-templates", "e05e59e8c1e0fa7ffbacc7a165a609fd37c590b5", [
        ("templates/inlined/action.html", "action.html"),
        ("templates/inlined/alert.html", "alert.html"),
        ("templates/inlined/billing.html", "billing.html"),
        ("LICENSE", "LICENSE"),
    ]),
    ("postmark", "ActiveCampaign/postmark-templates", "fa73527acb4eb59cf673a54572d213e908f1515c", [
        ("templates-inlined/basic-full/receipt/content.html", "receipt.html"),
        ("templates-inlined/basic-full/invoice/content.html", "invoice.html"),
        ("templates-inlined/basic-full/welcome/content.html", "welcome.html"),
        ("LICENSE", "LICENSE"),
    ]),
]


def main():
    rows = ["# file\tsource\tlicence"]
    for directory, repo, commit, files in SOURCES:
        for src, name in files:
            url = "https://raw.githubusercontent.com/%s/%s/%s" % (repo, commit, src)
            with urllib.request.urlopen(url, timeout=30) as f:
                data = f.read()
            dest = os.path.join(OUT, directory, name)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with open(dest, "wb") as f:
                f.write(data)
            if name != "LICENSE":
                rows.append("%s/%s\thttps://github.com/%s/blob/%s/%s\tMIT (%s/LICENSE)"
                            % (directory, name, repo, commit, src, directory))
            print("fetched", directory + "/" + name, len(data))
    for name in sorted(os.listdir(os.path.join(OUT, "exploration"))):
        if name.endswith(".html"):
            rows.append("exploration/%s\tscripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md 1.3 (synthetic)\tMIT (azul)" % name)
    with open(os.path.join(OUT, "SOURCES.tsv"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(rows) + "\n")


if __name__ == "__main__":
    main()
