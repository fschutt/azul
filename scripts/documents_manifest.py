#!/usr/bin/env python3
"""documents_manifest.py: what an app says it opens, as the entries a phone's manifest needs (standard library only).

The ONE place an app declares the files and links it opens is its entry in scripts/demo_apps.toml:

    [[app.documents]]
    name = "Markdown document"            # what the system's "Open with" list shows (iOS: CFBundleTypeName)
    role = "editor"                       # "viewer" | "editor" (iOS CFBundleTypeRole; Android: an editor also takes ACTION_EDIT)
    extensions = ["md", "markdown"]       # the names it recognises, without the dot (matched by name where a provider gives no type)
    mime = ["text/markdown"]              # the MIME types of the files (Android intent filters, iOS public.mime-type)
    uti = "net.daringfireball.markdown"   # the Uniform Type Identifier iOS knows the type by
    uti_conforms_to = []                  # SET it only for a type the app DEFINES (its uti is then declared to the system as an
                                          # imported type, conforming to these: ["public.data"]); a type the system already knows
                                          # (public.*, com.apple.*, org.idpf.epub-container, net.daringfireball.markdown) has none
    send = true                           # also a target of "Share": ACTION_SEND / ACTION_SEND_MULTIPLE (default true)

    [app.links]
    schemes = ["mailto", "geo"]           # links of these schemes open the app (Android intent filters; iOS CFBundleURLTypes)

`demo_build.py` writes the app's two tables as JSON to the file `AZ_DOCUMENTS` names; scripts/build-android.sh turns it into
`<intent-filter>`s of the app's activity (`python3 scripts/documents_manifest.py android $AZ_DOCUMENTS`) and scripts/build-ios.sh
into Info.plist keys (`... ios $AZ_DOCUMENTS`: CFBundleDocumentTypes, UTImportedTypeDeclarations, LSSupportsOpeningDocumentsInPlace,
CFBundleURLTypes). An app outside this repository writes the same JSON by hand. Nothing is generated for an app that declares nothing.

The module is pure: it reads a dictionary and returns text or a dictionary, so scripts/test_documents_manifest.py pins every rule.
"""
import json
import re
import sys
from xml.sax.saxutils import escape, quoteattr

ROLES = ("viewer", "editor")
EXTENSION = re.compile(r"[A-Za-z0-9]{1,16}")
# type "/" subtype, the tokens of RFC 6838 (and the wildcards an intent filter takes)
MIME = re.compile(r"(\*|[A-Za-z0-9!#$&^_.+-]{1,127})/(\*|[A-Za-z0-9!#$&^_.+-]{1,127})")
UTI = re.compile(r"[A-Za-z0-9][A-Za-z0-9.-]{0,126}")
SCHEME = re.compile(r"[A-Za-z][A-Za-z0-9+.-]{0,31}")
# the schemes a phone gives to the system's own apps: an app cannot be the target of them on iOS
IOS_SYSTEM_SCHEMES = ("http", "https")
MAX_DOCUMENTS = 32


def validate(documents, links, where="[[app.documents]]"):
    """-> the list of problems (strings) of an app's declarations; empty when they are fine."""
    errors = []
    if not isinstance(documents, list):
        return [f"{where} must be an array of tables"]
    if len(documents) > MAX_DOCUMENTS:
        errors.append(f"{where}: {len(documents)} document types; at most {MAX_DOCUMENTS}")
    seen_names = set()
    for i, d in enumerate(documents):
        at = f"{where}[{i}]"
        if not isinstance(d, dict):
            errors.append(f"{at} must be a table")
            continue
        name = d.get("name")
        if not isinstance(name, str) or not name.strip() or len(name) > 80 or any(ord(c) < 32 for c in name):
            errors.append(f"{at}: `name` is the text the system's list shows (1 to 80 characters)")
        elif name in seen_names:
            errors.append(f"{at}: `name` {name!r} is used twice")
        else:
            seen_names.add(name)
        if d.get("role", "viewer") not in ROLES:
            errors.append(f"{at}: `role` is one of {ROLES}, not {d.get('role')!r}")
        exts = d.get("extensions", [])
        mimes = d.get("mime", [])
        if not isinstance(exts, list) or not all(isinstance(e, str) and EXTENSION.fullmatch(e) for e in exts):
            errors.append(f"{at}: `extensions` are names without the dot, letters and digits only ({EXTENSION.pattern})")
        if not isinstance(mimes, list) or not all(isinstance(m, str) and MIME.fullmatch(m) for m in mimes):
            errors.append(f"{at}: `mime` are types of the form type/subtype ({MIME.pattern})")
        if not exts and not mimes:
            errors.append(f"{at}: a document type needs `extensions` or `mime` (or both): nothing would match")
        uti = d.get("uti")
        if uti is not None and not (isinstance(uti, str) and UTI.fullmatch(uti)):
            errors.append(f"{at}: `uti` is a Uniform Type Identifier (letters, digits, dots, hyphens)")
        conforms = d.get("uti_conforms_to", [])
        if not isinstance(conforms, list) or not all(isinstance(c, str) and UTI.fullmatch(c) for c in conforms):
            errors.append(f"{at}: `uti_conforms_to` is a list of identifiers")
        if conforms and not uti:
            errors.append(f"{at}: `uti_conforms_to` says what a type CONFORMS to, and the type has no `uti`")
        if not isinstance(d.get("send", True), bool):
            errors.append(f"{at}: `send` is true or false")
        unknown = sorted(set(d) - {"name", "role", "extensions", "mime", "uti", "uti_conforms_to", "send"})
        if unknown:
            errors.append(f"{at}: unknown key(s) {unknown}")
    links = links or {}
    if not isinstance(links, dict):
        errors.append("[app.links] must be a table")
        return errors
    schemes = links.get("schemes", [])
    if not isinstance(schemes, list) or not all(isinstance(s, str) and SCHEME.fullmatch(s) for s in schemes):
        errors.append(f"[app.links] `schemes` are URL schemes ({SCHEME.pattern})")
    unknown = sorted(set(links) - {"schemes"})
    if unknown:
        errors.append(f"[app.links] unknown key(s) {unknown}")
    return errors


def _attr(name, value):
    return f"{name}={quoteattr(str(value))}"


def _ext_patterns(extension):
    """The `android:pathPattern`s of one extension: a content URI rarely ends in the file's name, a file URI does; Android's
    pattern matcher has no alternation, so the name may have several dots (up to three) and either case."""
    out = []
    for ext in dict.fromkeys((extension.lower(), extension.upper())):
        for dots in range(0, 3):
            out.append(".*" + "\\\\..*" * dots + "\\\\." + ext)
    return out


def android_filters(documents, links=None, indent="            "):
    """-> the `<intent-filter>` elements of the app's activity for its declarations (a string; empty for none)."""
    pad = indent
    out = []

    def block(lines):
        out.append(f"{pad}<intent-filter>")
        out.extend(f"{pad}    {line}" for line in lines)
        out.append(f"{pad}</intent-filter>")

    for d in documents or []:
        mimes = list(dict.fromkeys(d.get("mime", [])))
        exts = list(dict.fromkeys(d.get("extensions", [])))
        actions = ["android.intent.action.VIEW"] + (["android.intent.action.EDIT"] if d.get("role") == "editor" else [])
        head = [f'<action {_attr("android:name", a)} />' for a in actions] + [
            '<category android:name="android.intent.category.DEFAULT" />',
            '<data android:scheme="content" />',
            '<data android:scheme="file" />',
        ]
        out.append(f"{pad}<!-- {escape(str(d['name']).replace('--', '-'))} -->")
        if mimes:
            block(head + [f'<data {_attr("android:mimeType", m)} />' for m in mimes])
        if exts:
            # by name, for a provider that says nothing or `application/octet-stream` of a file it serves
            block(
                head
                + ['<data android:host="*" />', '<data android:mimeType="*/*" />']
                + [f'<data {_attr("android:pathPattern", p)} />' for e in exts for p in _ext_patterns(e)]
            )
        if d.get("send", True) and mimes:
            block(
                [f'<action {_attr("android:name", a)} />' for a in ("android.intent.action.SEND", "android.intent.action.SEND_MULTIPLE")]
                + ['<category android:name="android.intent.category.DEFAULT" />']
                + [f'<data {_attr("android:mimeType", m)} />' for m in mimes]
            )
    schemes = list(dict.fromkeys((links or {}).get("schemes", [])))
    if schemes:
        actions = ["android.intent.action.VIEW"] + (["android.intent.action.SENDTO"] if "mailto" in schemes else [])
        block(
            [f'<action {_attr("android:name", a)} />' for a in actions]
            + [
                '<category android:name="android.intent.category.DEFAULT" />',
                '<category android:name="android.intent.category.BROWSABLE" />',
            ]
            + [f'<data {_attr("android:scheme", s)} />' for s in schemes]
        )
    return "\n".join(out) + ("\n" if out else "")


def ios_plist(documents, links=None, bundle_id="", file_sharing=False):
    """-> the Info.plist keys (a dict for plistlib) of the app's declarations; empty for none."""
    plist = {}
    types, imported, in_place = [], [], False
    for d in documents or []:
        utis = [d["uti"]] if d.get("uti") else []
        entry = {
            "CFBundleTypeName": d["name"],
            "CFBundleTypeRole": "Editor" if d.get("role") == "editor" else "Viewer",
            "LSHandlerRank": "Alternate",
        }
        if utis:
            entry["LSItemContentTypes"] = utis
        else:
            entry["CFBundleTypeExtensions"] = list(d.get("extensions", []))
            entry["CFBundleTypeMIMETypes"] = list(d.get("mime", []))
        types.append(entry)
        in_place = in_place or d.get("role") == "editor"
        uti = d.get("uti")
        if uti and d.get("uti_conforms_to"):
            imported.append(
                {
                    "UTTypeIdentifier": uti,
                    "UTTypeDescription": d["name"],
                    "UTTypeConformsTo": list(d.get("uti_conforms_to", [])),
                    "UTTypeTagSpecification": {
                        "public.filename-extension": list(d.get("extensions", [])),
                        "public.mime-type": list(d.get("mime", [])),
                    },
                }
            )
    if types:
        plist["CFBundleDocumentTypes"] = types
        if imported:
            plist["UTImportedTypeDeclarations"] = imported
        if in_place:
            # an editor is handed the ORIGINAL file (security-scoped), so that what it writes back lands where the person has it
            plist["LSSupportsOpeningDocumentsInPlace"] = True
    if file_sharing:
        # the app's Documents folder shows in the Files app (AzDrive's "On this iPhone")
        plist["UIFileSharingEnabled"] = True
        plist["LSSupportsOpeningDocumentsInPlace"] = True
    schemes = [s for s in dict.fromkeys((links or {}).get("schemes", [])) if s.lower() not in IOS_SYSTEM_SCHEMES]
    if schemes:
        plist["CFBundleURLTypes"] = [{"CFBundleURLName": bundle_id or "app", "CFBundleURLSchemes": schemes}]
    return plist


def main(argv):
    if len(argv) < 2 or argv[0] not in ("android", "ios", "check"):
        print("usage: documents_manifest.py android|ios|check <documents.json> [bundle-id] [--file-sharing]", file=sys.stderr)
        return 2
    mode, path = argv[0], argv[1]
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    documents, links = data.get("documents", []), data.get("links", {})
    problems = validate(documents, links)
    if problems:
        for p in problems:
            print(f"documents_manifest: {p}", file=sys.stderr)
        return 1
    if mode == "android":
        sys.stdout.write(android_filters(documents, links))
    elif mode == "ios":
        bundle = argv[2] if len(argv) > 2 and not argv[2].startswith("--") else ""
        sys.stdout.write(json.dumps(ios_plist(documents, links, bundle, "--file-sharing" in argv)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
