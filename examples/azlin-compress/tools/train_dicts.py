#!/usr/bin/env python3
"""Trains the shipped zstd dictionaries from PUBLIC corpora, and measures them on held-out files.

    python3 tools/train_dicts.py <corpora> <out-dir>

<corpora> holds one folder per corpus (see the crate README for the sources and their licenses):
json/ (SchemaStore), csv/ (the datasets collection), html/ and text/ (Project Gutenberg),
office/ (Apache POI's test documents; their XML parts are unpacked here), plus the local sources
listed in CODE_SOURCES. Every file goes to training or to the held-out test set by a hash of its
name (one in five is held out), so the figures below come from files the dictionary never saw.

The training is `zstd --train` (fastcover) with fixed parameters and a fixed dictionary id per
kind: the same corpora and the same zstd version give the same bytes. A shipped dictionary is
never retrained in place; a new one gets a new id.
"""
import glob, hashlib, io, os, subprocess, sys, zipfile

# id, kind, file name, max bytes, corpus folders
# CSV has none: a dictionary trained on the public tables lost to plain zstd on held-out ones
# (82.4 % against 83.1 % saved), since what repeats in a table is its own columns.
DICTS = [
    (1, "json", "json-1.zdict", 64 * 1024, ["json"]),
    (2, "markup", "markup-1.zdict", 64 * 1024, ["office-xml", "html"]),
    (3, "text", "text-1.zdict", 64 * 1024, ["text", "markdown"]),
    (4, "code", "code-1.zdict", 64 * 1024, ["code"]),
]
# zstd's dictionary ids below 32768 are reserved for registered dictionaries; ours sit at
# 0x415A_0000 ("AZ") plus the id. Segments name the dictionary by our id anyway.
DICT_ID_BASE = 0x415A0000
SAMPLE = 16 * 1024  # large files are cut into samples of this size: what a small file looks like
CODE_SOURCES = {
    # public, permissively licensed sources already on this machine
    "rust": os.path.expanduser("~/.cargo/registry/src/*/*/src/**/*.rs"),
    "c": os.path.expanduser("~/.cargo/registry/src/*/zstd-sys-*/zstd/lib/**/*.[ch]"),
}


def held_out(name):
    return hashlib.blake2b(name.encode(), digest_size=2).digest()[0] % 5 == 0


def office_xml(corpora):
    """The XML parts of the Office documents (what a DOCX/XLSX/PPTX holds once inflated)."""
    out = os.path.join(corpora, "office-xml")
    if os.path.isdir(out):
        return
    os.makedirs(out)
    for path in sorted(glob.glob(os.path.join(corpora, "office", "*"))):
        # POI's test data holds deliberately broken and encrypted files: skip what does not open.
        try:
            archive = zipfile.ZipFile(path)
            names = archive.namelist()
        except Exception:
            continue
        for member in names:
            if not member.endswith((".xml", ".rels")):
                continue
            try:
                data = archive.read(member)
            except Exception:
                continue
            name = os.path.basename(path) + "__" + member.replace("/", "__")
            open(os.path.join(out, name[-200:]), "wb").write(data)


def markdown(corpora):
    out = os.path.join(corpora, "markdown")
    if os.path.isdir(out):
        return
    os.makedirs(out)
    for i, path in enumerate(sorted(glob.glob(os.path.expanduser("~/.cargo/registry/src/*/*/README.md")))):
        crate = os.path.basename(os.path.dirname(path))
        open(os.path.join(out, crate + ".md"), "wb").write(open(path, "rb").read())


def code(corpora):
    out = os.path.join(corpora, "code")
    if os.path.isdir(out):
        return
    os.makedirs(out)
    import sysconfig
    stdlib = sysconfig.get_paths()["stdlib"]
    sources = {k: sorted(glob.glob(v, recursive=True)) for k, v in CODE_SOURCES.items()}
    sources["python"] = sorted(p for p in glob.glob(os.path.join(stdlib, "*.py")))
    for kind, paths in sources.items():
        for path in sorted(paths, key=lambda p: hashlib.blake2b(p.encode()).digest())[:4000]:
            name = kind + "__" + path.replace("/", "__")[-150:]
            open(os.path.join(out, name), "wb").write(open(path, "rb").read())


def samples(folders, corpora, train, piece=None):
    """Training or test samples: small files whole, large files cut into pieces (SAMPLE bytes,
    or `piece`)."""
    piece = piece or SAMPLE
    out = []
    for folder in folders:
        for path in sorted(glob.glob(os.path.join(corpora, folder, "*"))):
            if held_out(os.path.basename(path)) == train:
                continue
            data = open(path, "rb").read()
            if not data:
                continue
            for at in range(0, min(len(data), 64 * piece), piece):
                out.append(data[at:at + piece])
    return out


def train(dict_id, max_bytes, train_samples, out_path, scratch):
    folder = os.path.join(scratch, "samples-%d" % dict_id)
    os.makedirs(folder, exist_ok=True)
    for i, s in enumerate(train_samples):
        open(os.path.join(folder, "%06d" % i), "wb").write(s)
    subprocess.run(["zstd", "--train", "-q", "-r", folder, "-o", out_path, "--maxdict=%d" % max_bytes,
                    "--dictID=%d" % (DICT_ID_BASE + dict_id), "-19"], check=True)


def size(data, args):
    return len(subprocess.run(args, input=data, capture_output=True, check=True).stdout)


def evaluate(dict_path, test_samples):
    """Saving on held-out samples, each compressed alone: zstd 19, brotli 11, zstd 19 + dict."""
    raw = z = b = zd = 0
    for s in test_samples[:400]:
        raw += len(s)
        z += size(s, ["zstd", "-19", "-c", "-q"])
        b += size(s, ["brotli", "-q", "11", "-c"])
        zd += size(s, ["zstd", "-19", "-c", "-q", "-D", dict_path])
    pct = lambda x: 100.0 * (1 - x / raw)
    return len(test_samples[:400]), raw, pct(z), pct(b), pct(zd)


def main():
    corpora, out_dir = sys.argv[1], sys.argv[2]
    scratch = os.path.join(corpora, "_dict_scratch")
    office_xml(corpora)
    markdown(corpora)
    code(corpora)
    print("| id | kind | bytes | train samples | held-out pieces | piece | zstd 19 | brotli 11 | zstd 19 + dict |")
    print("|---|---|---|---|---|---|---|---|---|")
    for dict_id, kind, name, max_bytes, folders in DICTS:
        out_path = os.path.join(out_dir, name)
        train_samples = samples(folders, corpora, True)
        if not os.path.exists(out_path):
            train(dict_id, max_bytes, train_samples, out_path, scratch)
        for piece in (16 * 1024, 4 * 1024):
            n, raw, z, b, zd = evaluate(out_path, samples(folders, corpora, False, piece))
            print("| %d | %s | %d | %d | %d | %d KiB | %.1f %% | %.1f %% | %.1f %% |" % (
                dict_id, kind, os.path.getsize(out_path), len(train_samples), n, piece // 1024, z, b, zd))


main()
