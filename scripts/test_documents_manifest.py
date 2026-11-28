#!/usr/bin/env python3
"""Tests of scripts/documents_manifest.py: the intent filters and the plist keys of an app's declared document types and links."""
import json
import re
import sys
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import documents_manifest as dm  # noqa: E402

ANDROID = "{http://schemas.android.com/apk/res/android}"

MARKDOWN = {
    "name": "Markdown document",
    "role": "editor",
    "extensions": ["md", "markdown"],
    "mime": ["text/markdown"],
    "uti": "net.daringfireball.markdown",
}
EPUB = {"name": "E-book", "role": "viewer", "extensions": ["epub"], "mime": ["application/epub+zip"], "uti": "org.idpf.epub-container"}


def filters(documents, links=None):
    """The generated filters parsed as XML: [(actions, categories, [data attributes])]."""
    text = dm.android_filters(documents, links)
    root = ET.fromstring(f'<activity xmlns:android="http://schemas.android.com/apk/res/android">{text}</activity>')
    out = []
    for f in root.iter("intent-filter"):
        out.append((
            [a.get(ANDROID + "name") for a in f.iter("action")],
            [c.get(ANDROID + "name") for c in f.iter("category")],
            [{k.replace(ANDROID, ""): v for k, v in d.attrib.items()} for d in f.iter("data")],
        ))
    return out


class Validation(unittest.TestCase):
    def test_a_good_declaration_has_no_problem(self):
        self.assertEqual(dm.validate([MARKDOWN, EPUB], {"schemes": ["mailto", "geo"]}), [])

    def test_nothing_declared_is_fine_and_generates_nothing(self):
        self.assertEqual(dm.validate([], {}), [])
        self.assertEqual(dm.android_filters([], {}), "")
        self.assertEqual(dm.ios_plist([], {}), {})

    def test_hostile_values_are_refused(self):
        bad = [
            {"name": "x", "extensions": ["m d"], "mime": []},
            {"name": "x", "extensions": ["md\""], "mime": []},
            {"name": "x", "extensions": [".md"], "mime": []},
            {"name": "x", "extensions": [], "mime": ["text/plain\"><evil/>"]},
            {"name": "x", "extensions": [], "mime": ["notamime"]},
            {"name": "x", "extensions": [], "mime": []},
            {"name": "", "extensions": ["a"], "mime": []},
            {"name": "x", "role": "owner", "extensions": ["a"], "mime": []},
            {"name": "x" * 200, "extensions": ["a"], "mime": []},
            {"name": "x", "extensions": ["a" * 40], "mime": []},
            {"name": "x", "extensions": ["a"], "mime": [], "uti_conforms_to": ["public.data"]},
            {"name": "x", "extensions": ["a"], "mime": [], "surprise": 1},
            {"name": "x\u0001", "extensions": ["a"], "mime": []},
        ]
        for entry in bad:
            self.assertTrue(dm.validate([entry], {}), entry)

    def test_a_name_used_twice_is_refused(self):
        self.assertTrue(dm.validate([EPUB, dict(EPUB)], {}))

    def test_too_many_types_are_refused(self):
        many = [{"name": f"t{i}", "extensions": [f"e{i}"], "mime": []} for i in range(dm.MAX_DOCUMENTS + 1)]
        self.assertTrue(dm.validate(many, {}))

    def test_a_scheme_must_be_a_scheme(self):
        for scheme in ["", "1abc", "a b", "ma:ilto", "a" * 40, "x/y"]:
            self.assertTrue(dm.validate([], {"schemes": [scheme]}), scheme)
        self.assertTrue(dm.validate([], {"hosts": ["a"]}))


class Android(unittest.TestCase):
    def test_a_viewer_takes_view_by_type_by_name_and_share(self):
        result = filters([EPUB])
        # 1. view of content / file URIs of the type; 2. view by the name; 3. share of the type
        self.assertEqual(len(result), 3)
        view, by_name, send = result
        self.assertEqual(view[0], ["android.intent.action.VIEW"])
        self.assertIn("android.intent.category.DEFAULT", view[1])
        schemes = [d["scheme"] for d in view[2] if "scheme" in d]
        self.assertEqual(sorted(schemes), ["content", "file"])
        self.assertEqual([d["mimeType"] for d in view[2] if "mimeType" in d], ["application/epub+zip"])
        self.assertEqual(by_name[0], ["android.intent.action.VIEW"])
        self.assertIn({"host": "*"}, by_name[2])
        self.assertIn({"mimeType": "*/*"}, by_name[2])
        patterns = [d["pathPattern"] for d in by_name[2] if "pathPattern" in d]
        self.assertIn(".*\\\\.epub", patterns)
        self.assertIn(".*\\\\..*\\\\.epub", patterns)
        self.assertIn(".*\\\\.EPUB", patterns)
        self.assertEqual(send[0], ["android.intent.action.SEND", "android.intent.action.SEND_MULTIPLE"])
        self.assertEqual(send[2], [{"mimeType": "application/epub+zip"}])

    def test_an_editor_also_takes_edit(self):
        view = filters([MARKDOWN])[0]
        self.assertEqual(view[0], ["android.intent.action.VIEW", "android.intent.action.EDIT"])

    def test_a_type_that_is_not_shared_to_the_app_gets_no_send_filter(self):
        quiet = dict(EPUB, send=False)
        actions = [a for f in filters([quiet]) for a in f[0]]
        self.assertNotIn("android.intent.action.SEND", actions)
        self.assertIn("android.intent.action.VIEW", actions)

    def test_a_type_without_a_mime_is_found_by_its_name_only(self):
        shown = {"name": "Show", "role": "editor", "extensions": ["show"], "mime": []}
        result = filters([shown])
        self.assertEqual(len(result), 1, "no mime: no type filter and no send filter")
        self.assertTrue(any("pathPattern" in d for d in result[0][2]))

    def test_links_are_browsable_view_filters_of_their_schemes(self):
        result = filters([], {"schemes": ["mailto", "geo"]})
        self.assertEqual(len(result), 1)
        actions, categories, data = result[0]
        self.assertEqual(actions, ["android.intent.action.VIEW", "android.intent.action.SENDTO"])
        self.assertIn("android.intent.category.BROWSABLE", categories)
        self.assertEqual([d["scheme"] for d in data], ["mailto", "geo"])

    def test_the_filters_are_well_formed_xml_whatever_the_name_holds(self):
        odd = {"name": "Odd <&> \"name\" -- end", "extensions": ["a"], "mime": ["text/a+b"], "role": "viewer"}
        text = dm.android_filters([odd])
        ET.fromstring(f'<a xmlns:android="http://schemas.android.com/apk/res/android">{text}</a>')
        comment = text.split("<!--")[1].split("-->")[0]
        self.assertNotIn("--", comment, "a comment may not hold two hyphens")

    def test_nothing_the_app_did_not_declare_is_in_the_output(self):
        text = dm.android_filters([EPUB], {})
        self.assertNotIn("mailto", text)
        self.assertNotIn("text/markdown", text)

    def test_repeated_values_are_written_once(self):
        twice = dict(EPUB, mime=["application/epub+zip"] * 3, extensions=["epub", "epub"])
        text = dm.android_filters([twice])
        self.assertEqual(text.count('android:mimeType="application/epub+zip"'), 2, "once in the view filter, once in the send filter")


class Ios(unittest.TestCase):
    def test_a_document_type_with_a_system_uti_is_a_document_type_and_nothing_else(self):
        plist = dm.ios_plist([EPUB], {})
        (entry,) = plist["CFBundleDocumentTypes"]
        self.assertEqual(entry["CFBundleTypeName"], "E-book")
        self.assertEqual(entry["CFBundleTypeRole"], "Viewer")
        self.assertEqual(entry["LSItemContentTypes"], ["org.idpf.epub-container"])
        self.assertNotIn("UTImportedTypeDeclarations", plist, "a type the system knows is not imported")
        self.assertNotIn("LSSupportsOpeningDocumentsInPlace", plist, "a viewer is not handed the original")

    def test_a_type_the_app_defines_is_imported_with_its_tags(self):
        show = {"name": "Show", "role": "editor", "extensions": ["show"], "mime": ["application/vnd.azul.show"],
                "uti": "com.azul.show", "uti_conforms_to": ["public.data"]}
        plist = dm.ios_plist([show], {})
        (declared,) = plist["UTImportedTypeDeclarations"]
        self.assertEqual(declared["UTTypeIdentifier"], "com.azul.show")
        self.assertEqual(declared["UTTypeConformsTo"], ["public.data"])
        self.assertEqual(declared["UTTypeTagSpecification"]["public.filename-extension"], ["show"])
        self.assertEqual(declared["UTTypeTagSpecification"]["public.mime-type"], ["application/vnd.azul.show"])
        self.assertTrue(plist["LSSupportsOpeningDocumentsInPlace"], "an editor is handed the original")
        self.assertEqual(plist["CFBundleDocumentTypes"][0]["CFBundleTypeRole"], "Editor")

    def test_a_type_without_a_uti_is_described_by_extensions_and_types(self):
        plain = {"name": "Notes", "role": "viewer", "extensions": ["notes"], "mime": ["text/x-notes"]}
        (entry,) = dm.ios_plist([plain], {})["CFBundleDocumentTypes"]
        self.assertEqual(entry["CFBundleTypeExtensions"], ["notes"])
        self.assertEqual(entry["CFBundleTypeMIMETypes"], ["text/x-notes"])
        self.assertNotIn("LSItemContentTypes", entry)

    def test_schemes_become_url_types_but_not_the_ones_the_system_keeps(self):
        plist = dm.ios_plist([], {"schemes": ["https", "azmeet", "mailto", "http"]}, bundle_id="com.azul.AzMeet")
        (url,) = plist["CFBundleURLTypes"]
        self.assertEqual(url["CFBundleURLSchemes"], ["azmeet", "mailto"])
        self.assertEqual(url["CFBundleURLName"], "com.azul.AzMeet")
        self.assertEqual(dm.ios_plist([], {"schemes": ["https"]}), {})

    def test_file_sharing_is_opt_in_and_shows_the_documents_folder_in_files(self):
        self.assertNotIn("UIFileSharingEnabled", dm.ios_plist([EPUB], {}))
        shared = dm.ios_plist([], {}, file_sharing=True)
        self.assertTrue(shared["UIFileSharingEnabled"] and shared["LSSupportsOpeningDocumentsInPlace"])

    def test_the_output_is_a_plist_plistlib_writes(self):
        import plistlib
        data = plistlib.dumps(dm.ios_plist([MARKDOWN, EPUB], {"schemes": ["x"]}, "com.example"))
        self.assertIn(b"CFBundleDocumentTypes", data)


class Cli(unittest.TestCase):
    def run_cli(self, mode, data, *extra):
        import io, contextlib, tempfile
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump(data, f)
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = dm.main([mode, f.name, *extra])
        Path(f.name).unlink()
        return code, out.getvalue(), err.getvalue()

    def test_android_prints_the_filters(self):
        code, out, _ = self.run_cli("android", {"documents": [EPUB], "links": {}})
        self.assertEqual(code, 0)
        self.assertIn("android.intent.action.VIEW", out)

    def test_ios_prints_json(self):
        code, out, _ = self.run_cli("ios", {"documents": [EPUB], "links": {"schemes": ["az"]}}, "com.example.app")
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out)["CFBundleURLTypes"][0]["CFBundleURLName"], "com.example.app")

    def test_a_bad_declaration_stops_the_build_with_its_reason(self):
        code, out, err = self.run_cli("android", {"documents": [{"name": "x", "extensions": ["a b"], "mime": []}]})
        self.assertEqual(code, 1)
        self.assertEqual(out, "")
        self.assertIn("extensions", err)


if __name__ == "__main__":
    unittest.main()
