#!/usr/bin/env python3
"""REFACTOR13: split one big Rust source file into a module directory, VERBATIM.

    scripts/refactor/split.py PLAN.json [--root DIR] [--index-bin PATH] [--dry-run]

The plan (see fc_plan.json) names, for every top-level item of the source file,
the new file it goes to. Items are found by NAME through `item_index` (a syn
based lister, scripts/refactor/item_index), never by line number, so the same
plan re-runs on a newer version of the file; an item the plan does not place is
an error, never a guess.

What the script writes, and the ONLY changes it makes to the code:

* `<dir>/mod.rs`: the file's preamble (module docs), every `use` item verbatim,
  the plan's plumbing lines, `mod <m>;` + `pub use <m>::*;` per new file, the
  items the plan keeps in mod.rs, the declarations of the outlined modules
  and the file's trailing lines.
* `<dir>/<m>.rs` per plan module: a `//!` doc line, a copy of the original
  private `use` items (the crate allows `unused_imports`; a `pub use`
  re-export stays in mod.rs only), the plan's `glob_preamble`
  lines (the crate's split pattern: `#[allow(clippy::wildcard_imports)]` and a
  comment) and `use super::*;`, then its items in their original order. Each
  item carries the lines above it (blank lines and plain comments, which are
  no tokens) - every line of the old file lands in exactly one place.
* An item that had no visibility of its own gets `pub(super) ` (only that
  token) so its siblings can still name it; so do the struct fields and impl
  members the plan lists in `member_visibility` - nothing else.
* A moved item's path that starts `super::x`, x in the plan's `super_paths`
  (a sibling of the old file's module), becomes `super::super::x` - the
  same item, named from one module further down (found as tokens by
  item_index, so never in a comment or string).
* mod.rs re-exports each module with `pub use` if it holds a `pub` item,
  `pub(crate) use` otherwise; a module holding `pub(crate)` items gets the
  plan's `pub_crate_module_attrs` on its `mod` line (see the comment there).
* An inline `mod x { .. }` listed in `outline_modules` becomes `mod x;` (its
  attributes stay on the declaration) and its body moves verbatim - indentation
  included, so no string literal can change - to `<dir>/x.rs`. Its module path,
  so every test name in it, stays the same.

The source file is removed (a module cannot be both `fc.rs` and `fc/mod.rs`).
`scripts/refactor/verify_moved.py` then proves that nothing but this plumbing
changed.
"""
import argparse
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
DEFAULT_INDEX_BIN = "/Users/fschutt/Development/azul-refactor-target/release/item_index"
PUB_SUPER = "pub(super) "


class PlanError(Exception):
    pass


def run_index(index_bin, path, super_names=()):
    cmd = [index_bin, path, "--members"]
    if super_names:
        cmd += ["--super-paths", ",".join(super_names)]
    out = subprocess.run(cmd, capture_output=True, text=True)
    if out.returncode != 0:
        raise PlanError(f"item_index failed on {path}:\n{out.stderr}")
    items, members, supers = [], {}, {}
    for line in out.stdout.splitlines():
        f = line.split("\t")
        if f[0] == "SUPER":
            supers.setdefault(int(f[1]), []).append((int(f[2]), int(f[3]), f[4]))
        elif f[0] == "ITEM":
            items.append(
                {
                    "idx": int(f[1]),
                    "kind": f[2],
                    "name": f[3],
                    "start": int(f[4]),
                    "end": int(f[5]),
                    "vis": f[6],
                    "vis_at": None if f[7] == "-" else tuple(int(x) for x in f[7].split(":")),
                    "extra": f[8],
                }
            )
        elif f[0] == "MEMBER":
            members.setdefault(int(f[1]), []).append(
                {
                    "kind": f[2],
                    "name": f[3],
                    "line": int(f[4]),
                    "vis": f[5],
                    "vis_at": None if f[6] == "-" else tuple(int(x) for x in f[6].split(":")),
                }
            )
    return items, members, supers


def preamble_of(plan, doc, use_items):
    """The generated head of a new module file: its `//!` line, a copy of the
    old file's `use` items (attributes included, lines verbatim), the plan's
    `glob_preamble` lines and `use super::*;`. `verify_moved.py` rebuilds
    exactly this from the old file to prove the copy."""
    out = [f"//! {doc}\n", "\n"]
    if plan.get("copy_use_block", True):
        for item_lines in use_items:
            out += item_lines
    out += [l + "\n" for l in plan.get("glob_preamble", [])]
    out.append("use super::*;\n")
    return out


def key_of(item):
    if item["kind"] == "impl":
        return item["name"]  # "impl Foo" / "impl Trait for Foo"
    return f"{item['kind']} {item['name']}"


def extra_field(item, name):
    for part in item["extra"].split(" "):
        if part.startswith(name + "="):
            return part[len(name) + 1 :]
    return None


def split(plan, root, index_bin, dry_run):
    src_rel = plan["source"]
    src = os.path.join(root, src_rel)
    out_dir = os.path.join(root, plan["target_dir"])
    with open(src, encoding="utf-8") as fh:
        text = fh.read()
    if not text.endswith("\n"):
        raise PlanError(f"{src_rel} does not end with a newline")
    lines = text.splitlines(keepends=True)
    items, members, supers = run_index(index_bin, src, plan.get("super_paths", []))

    # ---- chunks: every line belongs to exactly one place -----------------
    prev_end = 0
    for it in items:
        if it["start"] <= prev_end or it["end"] < it["start"]:
            raise PlanError(f"overlapping item ranges at {key_of(it)}")
        it["chunk_start"] = prev_end + 1 if prev_end else it["start"]
        prev_end = it["end"]
    preamble = lines[: items[0]["start"] - 1]
    trailing = lines[items[-1]["end"] :]

    # ---- where each item goes ---------------------------------------------
    keys = {}
    for it in items:
        keys.setdefault(key_of(it), []).append(it)
    dest = {}  # item idx -> module name | "mod.rs" | "outline"

    def place(key, where):
        if key not in keys:
            raise PlanError(f"plan names `{key}`, which {src_rel} does not have")
        # Every item under the key goes to the same place: two `impl Foo`
        # blocks, or the cfg variants of one fn.
        group = keys[key]
        for it in group:
            if it["idx"] in dest:
                raise PlanError(f"`{key}` is placed twice")
            dest[it["idx"]] = where

    for it in items:
        if it["kind"] == "use":
            dest[it["idx"]] = "mod.rs"
    for key in plan.get("keep_in_mod_rs", []):
        place(key, "mod.rs")
    for name in plan.get("outline_modules", []):
        place(f"mod {name}", "outline")
    module_names = [m["name"] for m in plan["modules"]]
    if len(set(module_names)) != len(module_names):
        raise PlanError("duplicate module names in the plan")
    for m in plan["modules"]:
        for key in m["items"]:
            place(key, m["name"])
    # An impl the plan does not place follows its self type.
    type_home = {}
    for it in items:
        if it["kind"] in ("struct", "enum", "union", "trait", "type") and it["idx"] in dest:
            type_home[it["name"]] = dest[it["idx"]]
    for it in items:
        if it["idx"] in dest or it["kind"] != "impl":
            continue
        self_ty = extra_field(it, "self")
        if self_ty not in type_home:
            raise PlanError(f"`{key_of(it)}`: its self type is not placed here - place it explicitly")
        dest[it["idx"]] = type_home[self_ty]
    unplaced = [key_of(it) for it in items if it["idx"] not in dest]
    if unplaced:
        raise PlanError("the plan does not place:\n  " + "\n  ".join(unplaced))

    # ---- visibility tokens and `super::` paths -------------------------------
    inserts = {}  # 0-based line index -> list of (char column, text)

    def insert_at(pos, what, text=PUB_SUPER):
        line0, col = pos[0] - 1, pos[1]
        line = lines[line0]
        if text == PUB_SUPER:
            before = line[col - 1] if col > 0 else " "
            at = line[col] if col < len(line) else ""
            # The token goes between whitespace and the start of a keyword or
            # identifier - anything else means the index and the text disagree.
            if before not in " \t" or not (at.isalpha() or at == "_"):
                raise PlanError(f"{what}: no token boundary at line {pos[0]} col {col}: {line!r}")
        elif not line[col:].startswith("super::"):
            raise PlanError(f"{what}: no `super::` at line {pos[0]} col {col}: {line!r}")
        inserts.setdefault(line0, []).append((col, text))

    # A moved item's `super::x` (x a sibling of the old file's module) names
    # the same item one module further down as `super::super::x`.
    path_log = []
    for it in items:
        if dest[it["idx"]] in ("mod.rs", "outline"):
            continue
        for line, col, name in supers.get(it["idx"], []):
            insert_at((line, col), f"{key_of(it)}: super::{name}", "super::")
            path_log.append(f"{key_of(it)}: line {line} super::{name} -> super::super::{name}")

    vis_log = []
    for it in items:
        where = dest[it["idx"]]
        if where in ("mod.rs", "outline") or it["vis"] != "inherited" or it["vis_at"] is None:
            continue
        insert_at(it["vis_at"], key_of(it))
        vis_log.append(f"{key_of(it)} -> pub(super)")
    for key, names in plan.get("member_visibility", {}).items():
        if key not in keys or len(keys[key]) != 1:
            raise PlanError(f"member_visibility: `{key}` is not exactly one item")
        it = keys[key][0]
        if dest[it["idx"]] in ("mod.rs", "outline"):
            raise PlanError(f"member_visibility: `{key}` stays in mod.rs - its members need nothing")
        have = {m["name"]: m for m in members.get(it["idx"], [])}
        for name in names:
            m = have.get(name)
            if m is None:
                raise PlanError(f"member_visibility: `{key}` has no member `{name}`")
            if m["vis"] != "inherited":
                raise PlanError(f"member_visibility: `{key}::{name}` is already `{m['vis']}`")
            insert_at(m["vis_at"], f"{key}::{name}")
            vis_log.append(f"{key}::{name} -> pub(super)")

    def emit(line0):
        line = lines[line0]
        for col, text in sorted(inserts.get(line0, []), reverse=True):
            line = line[:col] + text + line[col:]
        return line

    def chunk(it, upto=None):
        last = it["end"] if upto is None else upto
        return [emit(i) for i in range(it["chunk_start"] - 1, last)]

    # ---- the new files -------------------------------------------------------
    use_items = [it for it in items if it["kind"] == "use"]
    files = {}
    mod_rs = list(preamble)
    for it in use_items:
        mod_rs += chunk(it)
    mod_rs += [l + "\n" for l in plan.get("mod_rs_plumbing", [])]
    mod_rs.append("\n")
    # Each module is re-exported with `pub use` when it holds a `pub` item (the
    # old file's public paths), with `pub(crate) use` otherwise: a `pub` glob
    # wider than its items is `unreachable_pub`, a private one
    # `clippy::wildcard_imports` (CI denies warnings), and the siblings name
    # the items through `use super::*` either way. A module holding
    # `pub(crate)` items gets the plan's `pub_crate_module_attrs` (clippy's
    # `redundant_pub_crate` would rather call them `pub`, which a `pub use`
    # glob would publish): the items keep the visibility they had.
    mod_rs += [l + "\n" for l in plan.get("mod_block_comment", [])]
    for name in module_names:
        vis = {it["vis"] for it in items if dest[it["idx"]] == name}
        reexport = "pub use" if "pub" in vis else "pub(crate) use"
        if "pub(crate)" in vis:
            mod_rs += [l + "\n" for l in plan.get("pub_crate_module_attrs", [])]
        mod_rs.append(f"mod {name};\n{reexport} {name}::*;\n")
    for it in items:
        if dest[it["idx"]] == "mod.rs" and it["kind"] != "use":
            mod_rs += chunk(it)
    for it in items:
        if dest[it["idx"]] != "outline":
            continue
        body = extra_field(it, "body")
        if body is None or body == "external":
            raise PlanError(f"`{key_of(it)}` has no inline body to outline")
        open_line, close_line = (int(x) for x in body.split(":"))
        head = chunk(it, upto=open_line)
        if not head[-1].rstrip().endswith("{") or head[-1].rstrip()[:-1].rstrip() == "":
            raise PlanError(f"`{key_of(it)}`: `{{` does not end the `mod` line")
        if lines[close_line - 1].strip() != "}" or close_line != it["end"]:
            raise PlanError(f"`{key_of(it)}`: its closing `}}` is not alone on its last line")
        head[-1] = head[-1].rstrip()[:-1].rstrip() + ";\n"
        mod_rs += head
        files[f"{it['name']}.rs"] = [lines[i] for i in range(open_line, close_line - 1)]
    mod_rs += trailing
    files["mod.rs"] = mod_rs

    # The submodules' copy of the imports: the PRIVATE `use` items only - a
    # `pub use` is a re-export of the module, which mod.rs keeps.
    private_uses = [lines[it["start"] - 1 : it["end"]] for it in use_items if it["vis"] == "inherited"]
    for m in plan["modules"]:
        out = preamble_of(plan, m["doc"], private_uses)
        mine = [it for it in items if dest[it["idx"]] == m["name"]]
        if not mine:
            raise PlanError(f"module `{m['name']}` gets no item")
        body = []
        for it in mine:
            body += chunk(it)
        if body and body[0].strip() != "":
            out.append("\n")
        out += body
        files[f"{m['name']}.rs"] = out

    for name in files:
        if name != "mod.rs" and name[:-3] in module_names and name[:-3] in plan.get("outline_modules", []):
            raise PlanError(f"`{name}` is both a plan module and an outlined module")

    # ---- write ---------------------------------------------------------------
    summary = [f"split {src_rel} -> {plan['target_dir']}/ ({len(items)} items, {len(lines)} lines)"]
    for name, content in sorted(files.items()):
        if name == "mod.rs":
            n_items = sum(1 for it in items if dest[it["idx"]] in ("mod.rs", "outline"))
        elif name[:-3] in module_names:
            n_items = sum(1 for it in items if dest[it["idx"]] == name[:-3])
        else:
            n_items = 0  # an outlined module's body: its items are inside it
        summary.append(f"  {name:<50} {len(content):>6} lines  {n_items:>3} items")
    summary.append(f"  visibility: {len(vis_log)} `pub(super)` tokens")
    summary.append(f"  paths: {len(path_log)} `super::x` -> `super::super::x`")
    vis_log += path_log
    if not dry_run:
        os.makedirs(out_dir, exist_ok=True)
        for name, content in files.items():
            path = os.path.join(out_dir, name)
            if os.path.exists(path):
                raise PlanError(f"{path} exists - split a clean tree")
        for name, content in files.items():
            with open(os.path.join(out_dir, name), "w", encoding="utf-8") as fh:
                fh.write("".join(content))
        os.remove(src)
    return summary, vis_log


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("plan")
    ap.add_argument("--root", default=REPO, help="tree the plan's paths are relative to")
    ap.add_argument("--index-bin", default=os.environ.get("ITEM_INDEX_BIN", DEFAULT_INDEX_BIN))
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--verbose", action="store_true", help="list every visibility change")
    args = ap.parse_args()
    with open(args.plan, encoding="utf-8") as fh:
        plan = json.load(fh)
    try:
        summary, vis_log = split(plan, args.root, args.index_bin, args.dry_run)
    except PlanError as e:
        print(f"split.py: {e}", file=sys.stderr)
        return 1
    print("\n".join(summary))
    if args.verbose:
        print("\n".join("    " + v for v in vis_log))
    return 0


if __name__ == "__main__":
    sys.exit(main())
