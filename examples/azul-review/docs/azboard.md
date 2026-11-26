# azboard/1: a whiteboard of code as a JSON file

AzReview's whiteboard is a canvas of cutouts (a picture of some lines of a file) and the arrows between them: the
architecture of a project as a person explains it. `azboard/1` is the one document a board is kept as and published as:
plain JSON, versioned, self-describing, with a link to the code behind every cutout, so that a web viewer can show the
board and jump from a box to the very lines it is about. The file a board is kept in on the disk (`*.azboard.zip`) holds
this document as `board.json` and the pictures as `img-<id>.png`; "Export..." in the list of boards writes it as a file of
its own (`Name.azboard.json`) with the pictures inside it, or in a folder beside it.

Files here: `azboard-1.schema.json` (JSON Schema, draft 2020-12), `azboard-1.example.json` (a board of the Azul tree at a
real commit; its pictures are small stand-ins), `make_example.py` (what wrote the example).

## The document

```text
{
  "format": "azboard/1",                 the name and the major version; a reader of 1 refuses azboard/2
  "generator": "AzReview 0.1.0",
  "board":     { "id": "b1", "name": "...", "created_ms": 0, "modified_ms": 0 },
  "workspace": { "name": "azul",         the folder's own name, never its path
                 "path_in_repo": "...",  where the folder lies in the repository (absent at its root)
                 "repo": { "remote", "web_url", "host", "rev", "branch", "dirty" } },   absent outside a git repository
  "author":    "...",                    the setting whiteboard-author: git's user.name, a name, or none
  "canvas":    { "unit": "px", "origin": "top-left", "view": {x, y, zoom}, "bounds": {x, y, width, height} },
  "next_id":   10,
  "assets":    { "img-1": { "type": "image/png", "width", "height", "src" } },
  "items":     [ cutouts and arrows ],
  "snapshots": [ the history: save points with the reason they were taken ]
}
```

**Canvas.** px, x to the right, y downwards, from the top-left corner; the canvas has no edge and its origin is
arbitrary. `bounds` is the rectangle that holds everything (where a viewer fits the picture); `view` is how the person last
looked at it. **Ids** are positive integers that name an item for ever: an id is never given twice on a board, so an arrow's
`item` is stable, and so is a comment that refers to an item.

**Cutout.** `x`, `y`, `width`, `height` (how large it is shown), `label` (what the person wrote under it), `image`
(`asset`: a key of `assets`; `scale`: device pixels of the picture for one px of the canvas), and `source`:

| field | what |
| --- | --- |
| `path` | the file, relative to the repository's root |
| `line_start`, `line_end` | the lines the cutout shows, from 1, both included |
| `hash` | `sha256:` and the hash of the covered lines (below) |
| `rev` | the commit the lines were circled at |
| `permalink` | the address of these lines at `rev` on the host |
| `language` | by the file's name, for colouring |
| `code` | the lines as the board kept them (at most 200 of at most 300 characters) |
| `azreview` | what only AzReview itself uses; leave it alone |

`hash` is the SHA-256, as 64 lower-case hex digits after `sha256:`, of the covered lines, each followed by a line feed (a
carriage return at the end of a line is not part of it), as UTF-8. All the lines that were covered, also when `code` holds
fewer. To check a reference: cut lines `line_start` to `line_end` out of the file `path` at `rev`, write a line feed after
each, hash.

**Arrow.** `from` and `to` (the head), each an `{ "item": <id>, "anchor": "auto" }` (the line meets the cutout's edge), `"side"`
with `"side": "top|right|bottom|left"`, or `"point"` with `"u"` and `"v"` (fractions of the cutout's width and height), or
`{ "free": [x, y] }` for a loose end; `bend` (0 is straight, the sign says to which side it curves) and `label`.

**Pictures.** An asset's `src` is a `data:image/png;base64,...` address (the export's default: one file to hand on), or a
name relative to the document: a file in the folder beside it (`Name.assets/img-1.png`) or an entry of the zip the board is
kept in (`img-1.png`). Never an address on the web: AzReview does not fetch one, and a viewer should not need to.

## Links to the code

A permalink is made from the repository's web address, the commit, the path and the lines, in the form of the host:

| host | form |
| --- | --- |
| GitHub | `https://github.com/owner/repo/blob/REV/PATH#L10-L20` |
| GitLab | `https://gitlab.com/group/project/-/blob/REV/PATH#L10-20` |
| Bitbucket | `https://bitbucket.org/owner/repo/src/REV/PATH#lines-10:20` |
| Gitea, Forgejo, Codeberg | `https://codeberg.org/owner/repo/src/commit/REV/PATH#L10-L20` |
| any other | the template of the setting `whiteboard-permalink-template` (`{web}`, `{rev}`, `{path}`, `{start}`, `{end}`), else the GitHub form |

A workspace that is not a git repository has no links: only the path, the lines and the hash. A reference whose revision is
not known has none either.

**How a viewer opens a reference.** It follows the `permalink`: the host's own page for that commit, that file and those
lines, so it shows the code the board's author circled and not whatever the branch holds today. A viewer that wants the text
itself (to show it beside the box, to search it) has `code`; one that wants to be sure fetches the file at `rev` from the
host, cuts the lines out, hashes them as above and compares with `hash`: equal means the box still shows what the link
shows. Every reference carries the `rev` it was circled at, and a link names a commit, never a branch: a board made over
weeks keeps pointing at the right lines in each of its references, and a document published today is as valid next year,
however far the code has moved. When the code changes, a newer snapshot records the new revisions and says why; the older
snapshots keep linking to the old ones.

## What is personal

The document holds the folder's own name and never its path; the remote's address as git says it, without a password or token;
paths relative to the repository; and the author: the name git is configured with (`user.name`, never the e-mail address),
or the one the setting `whiteboard-author` gives, or none (`off`). The export sheet says which, before anything is written.

## Reading

A reader ignores a field it does not know and leaves out (and counts) an item of a kind it does not know. A document whose
`format` names a newer major version is refused with a message and left as it is, never rewritten. What is hostile is capped,
not followed: a path that leaves the folder (`..`, absolute, a backslash, a drive), a picture that is not a `data:` address
or a plain file beside the document, a document or a picture larger than a board is, more than 5000 items, nesting deeper
than 16 levels, numbers that are none or are out of range. AzReview keeps 500 items on a board.
