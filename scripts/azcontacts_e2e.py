#!/usr/bin/env python3
"""AzContacts end to end, headless, over the debug server.

    1. writes a fixture .vcf (vCard 3.0, a folded line, an escaped comma; one new card,
       one that duplicates a sample contact, one more new card) and starts AzContacts
       (AZ_BACKEND=headless, AZ_DEBUG=--debug-port) on a fresh data folder with --sample
       and the fixture as its file argument;
    2. waits for the 300 sample contacts (300 files contacts/<uuid>.vcf on disk) and the
       import preview of the fixture (3 rows: 2 new, 1 possible duplicate); Import writes 2
       more files;
    3. search: "krug" finds Ben Krüger only; a click on a row selects it;
    4. a new contact: a bad email is refused with a message; "Add a birthday" shows the
       birthday's calendar (#edit-birthday-picker, "Year unknown"); the fixed one is saved as
       contacts/<uid>.vcf (vCard 4.0, FN, EMAIL and BDAY:--0101 in it);
    5. possible duplicates: the merge screen merges the first pair; one file goes;
    6. the A-Z bar scrolls to a section; the settings page saves Flora / Dark;
    7. deleting the new contact removes its file;
    8. an Outlook CSV file: the preview maps its 5 columns (#import-column-<n>) and shows 2
       new people; Import writes 2 more files. Screenshots on the way.

AzContacts' DOM ids carry its prefix `__azcontacts_` (examples/azul-contacts/src/ids.rs): the
`#name`s above are `#__azcontacts_name` (`app.sel`); an older build's bare names are detected.

Usage (after building libazul with the debug server and AzContacts, one app at a time):

    python3 scripts/azcontacts_e2e.py [--bin target/release/AzContacts]
        [--debug-port 8782] [--timeout 240] [--out <dir>] [--keep]
"""

import json
import os
import shutil

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azcontacts"

FIXTURE = (
    "BEGIN:VCARD\r\n"
    "VERSION:3.0\r\n"
    "FN:Mara Neumeier\r\n"
    "N:Neumeier;Mara;;;\r\n"
    "EMAIL;TYPE=INTERNET,HOME:mara.neumeier@example.org\r\n"
    "TEL;TYPE=CELL,VOICE:+49 170 0000 9001\r\n"
    "NOTE:A note long enough to be folded by any careful vCard writer\\, with an es\r\n"
    " caped comma in it.\r\n"
    "END:VCARD\r\n"
    "BEGIN:VCARD\r\n"
    "VERSION:3.0\r\n"
    "FN:Anna Berg\r\n"
    "N:Berg;Anna;;;\r\n"
    "EMAIL;TYPE=INTERNET,WORK:anna@example.org\r\n"
    "END:VCARD\r\n"
    "BEGIN:VCARD\r\n"
    "VERSION:4.0\r\n"
    "FN:Zoe Zylinski\r\n"
    "N:Zylinski;Zoe;;;\r\n"
    "BDAY:--0704\r\n"
    "END:VCARD\r\n"
)


def contact_files(data_dir):
    folder = os.path.join(data_dir, "contacts")
    try:
        return sorted(n for n in os.listdir(folder) if n.endswith(".vcf"))
    except OSError:
        return []


def body(args, logs, out):
    binary = e2e.find_binary("AzContacts", args.bin, "AZCONTACTS_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    fixture = os.path.join(logs, "friends.vcf")
    with open(fixture, "w", encoding="utf-8", newline="") as f:
        f.write(FIXTURE)
    app = e2e.App(TAG, binary, ["--data-dir", data_dir, "--sample", "--size", "1100x720", fixture],
                  args.debug_port, logs, args.timeout)
    try:
        # 1-2: the sample and the import preview.
        app.until("the contacts to load", lambda: app.printed("AZCONTACTS_LOADED", r"\d+"))
        loaded = int(app.printed("AZCONTACTS_LOADED", r"\d+")[-1])
        if loaded != 300:
            raise Failure("--sample on an empty folder gives 300 contacts, got %d" % loaded)
        app.until("the 300 sample files", lambda: app.printed("AZCONTACTS_SAMPLE_WRITTEN", r"\d+"))
        files = contact_files(data_dir)
        if len(files) != 300:
            raise Failure("expected 300 contact files, found %d" % len(files))
        app.log("300 sample contacts, one file each")
        app.until("the import preview", lambda: app.printed("AZCONTACTS_IMPORT_PREVIEW", r".+"))
        app.detect_naming("__azcontacts_", "toolbar-new")
        preview = app.last("AZCONTACTS_IMPORT_PREVIEW")
        if not preview.startswith("3 ") or "2 new" not in preview or "1 possible duplicate" not in preview:
            raise Failure("the import preview should be 3 rows, 2 new, 1 duplicate: %r" % preview)
        app.frame(2)
        app.screenshot(os.path.join(out, "import-preview.png"))
        app.click(selector=app.sel("import-run"))
        app.expect_line("AZCONTACTS_IMPORTED", "2", "Import imports the two new cards")
        app.until("302 files", lambda: len(contact_files(data_dir)) == 302)
        app.screenshot(os.path.join(out, "list.png"))

        # 3: search and select.
        app.text_input(app.sel("contacts-search"), "krug")
        app.expect_line("AZCONTACTS_VIEW", "1", "searching krug")
        app.click(text="Ben Krüger")
        app.until("Ben to be selected", lambda: (app.last("AZCONTACTS_SELECTED") or "").endswith("Ben Krüger"))
        app.key("a", primary=True)
        app.key("backspace")
        app.until("the whole list again", lambda: app.last("AZCONTACTS_VIEW") == "302")
        app.screenshot(os.path.join(out, "card.png"))

        # 4: a new contact, a bad email first.
        app.click(selector=app.sel("toolbar-new"))
        app.until("the edit form", lambda: app.has_id(app.name("contact-edit")))
        app.text_input(app.sel("edit-given"), "Test")
        app.text_input(app.sel("edit-family"), "Person")
        app.click(selector=app.sel("edit-add-email"))
        app.text_input(app.sel("edit-email-0"), "not-an-email")
        app.click(selector=app.sel("edit-save"))
        app.until("the form to refuse the email",
                  lambda: "not-an-email" in (app.last("AZCONTACTS_PROBLEMS") or ""))
        app.screenshot(os.path.join(out, "edit-problems.png"))
        app.click(selector=app.sel("edit-email-remove-0"))
        app.click(selector=app.sel("edit-add-email"))
        app.text_input(app.sel("edit-email-0"), "test.person@example.org")
        # The birthday on a calendar: "Add a birthday" starts 1 January, year unknown.
        app.click(selector=app.sel("edit-birthday-add"))
        app.until("the birthday calendar", lambda: app.has_id(app.name("edit-birthday-picker")))
        if not app.has_id(app.name("edit-birthday-no-year")):
            raise Failure("the birthday has no 'Year unknown' box")
        saved_before = len(app.printed("AZCONTACTS_SAVED"))
        app.click(selector=app.sel("edit-save"))
        app.until("the new contact's file", lambda: len(app.printed("AZCONTACTS_SAVED")) > saved_before)
        uid = app.printed("AZCONTACTS_SAVED")[-1]
        path = os.path.join(data_dir, "contacts", uid + ".vcf")
        with open(path, "r", encoding="utf-8") as f:
            card = f.read()
        if "VERSION:4.0" not in card or "FN:Test Person" not in card or "test.person@example.org" not in card:
            raise Failure("%s does not hold the new contact:\n%s" % (path, card))
        if "BDAY:--0101" not in card:
            raise Failure("%s does not hold the birthday picked (1 January, no year):\n%s" % (path, card))
        app.log("saved %s" % path)

        # 5: duplicates and merge.
        app.click(selector=app.sel("toolbar-duplicates"))
        app.until("the duplicates", lambda: app.printed("AZCONTACTS_DUPLICATES", r"\d+"))
        pairs = int(app.printed("AZCONTACTS_DUPLICATES", r"\d+")[-1])
        if pairs < 3:
            raise Failure("the sample has 3 duplicate pairs, the finder saw %d" % pairs)
        app.screenshot(os.path.join(out, "merge.png"))
        before = len(contact_files(data_dir))
        app.click(selector=app.sel("merge-run"))
        app.until("the merge", lambda: app.printed("AZCONTACTS_MERGED", r".+"))
        app.until("one file fewer", lambda: len(contact_files(data_dir)) == before - 1)
        app.log("merged; %d files" % (before - 1))

        # 6: the jump bar and the settings.
        app.click(text="All contacts (302)")
        app.click(selector=app.sel("jump-K"))
        app.until("the jump", lambda: app.printed("AZCONTACTS_JUMP", r".+"))
        app.key("comma", primary=True)
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.click(text="Appearance")
        app.until("the Appearance section", lambda: app.has_id("appkit-theme"))
        saved = len(app.printed("AZCONTACTS_SETTINGS_SAVED"))
        app.click(text="Flora")
        app.click(text="Dark")
        app.until("the settings file", lambda: len(app.printed("AZCONTACTS_SETTINGS_SAVED")) >= saved + 2)
        with open(os.path.join(data_dir, "contacts", "settings.json"), "r", encoding="utf-8") as f:
            settings = json.load(f)
        if settings.get("theme") != "flora" or settings.get("mode") != "dark":
            raise Failure("settings.json does not hold flora / dark: %s" % settings)
        app.screenshot(os.path.join(out, "settings-flora-dark.png"))
        app.key("escape")
        app.until("the settings to close", lambda: not app.has_id("appkit-settings"))
        app.screenshot(os.path.join(out, "list-flora-dark.png"))

        # 7: delete the new contact.
        app.text_input(app.sel("contacts-search"), "test person")
        app.expect_line("AZCONTACTS_VIEW", "1", "searching the new contact")
        app.click(text="Test Person")
        app.click(selector=app.sel("card-delete"))
        app.click(selector=app.sel("card-delete-confirm"))
        app.until("the file to go", lambda: not os.path.exists(path))

        # 8: a CSV file (Outlook's columns), mapped by its headers; Import writes its people.
        csv_path = os.path.join(logs, "outlook.csv")
        with open(csv_path, "w", encoding="utf-8", newline="") as f:
            f.write("First Name,Last Name,E-mail Address,Mobile Phone,Notes\r\n"
                    "Csv,Firstperson,csv.first@example.org,+49 170 0000 9101,\"From Outlook, quoted\"\r\n"
                    "Csv,Secondperson,csv.second@example.org,,\r\n")
        app.click(selector=app.sel("toolbar-import"))
        app.until("the import screen", lambda: app.has_id(app.name("import-path")))
        previews = len(app.printed("AZCONTACTS_IMPORT_PREVIEW", r".+"))
        app.text_input(app.sel("import-path"), csv_path)
        app.click(selector=app.sel("import-read"))
        app.until("the CSV preview",
                  lambda: len(app.printed("AZCONTACTS_IMPORT_PREVIEW", r".+")) > previews)
        preview = app.last("AZCONTACTS_IMPORT_PREVIEW")
        if not preview.startswith("2 ") or "2 new" not in preview:
            raise Failure("the CSV preview should be 2 new rows: %r" % preview)
        for n in range(5):
            if not app.has_id(app.name("import-column-%d" % n)):
                raise Failure("the CSV column %d has no mapping control" % n)
        app.screenshot(os.path.join(out, "import-csv.png"))
        before = len(contact_files(data_dir))
        app.click(selector=app.sel("import-run"))
        app.until("the CSV people's files", lambda: len(contact_files(data_dir)) == before + 2)
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
        if not args.keep:
            shutil.rmtree(data_dir, ignore_errors=True)


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8782)
