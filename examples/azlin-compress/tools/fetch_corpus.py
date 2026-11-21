#!/usr/bin/env python3
"""Fetches the fixed measurement corpus of tools/corpus.tsv into a folder outside the repository
and checks every file's size and SHA-256 (a changed or missing file is an error, never a
substitute). Public files only; a neutral User-Agent, nothing about the person running it.

    python3 tools/fetch_corpus.py <folder>     # makes <folder>/{silesia,jpeg,docs}/

The files are not committed: Silesia's and the photos' licences allow research use, not
redistribution from here, and the measurement only needs them on the machine that measures.
"""
import hashlib
import io
import os
import sys
import time
import urllib.request
import zipfile

UA = 'azul-supply-chain-gate (+https://' + 'github.com/fschutt/azul)'
MANIFEST = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'corpus.tsv')


def fetch(url):
    req = urllib.request.Request(url, headers={'User-Agent': UA})
    with urllib.request.urlopen(req, timeout=300) as r:
        return r.read()


def main():
    if len(sys.argv) != 2:
        sys.exit('usage: fetch_corpus.py <folder>')
    root = sys.argv[1]
    rows = [line.rstrip('\n').split('\t') for line in open(MANIFEST, encoding='utf-8')
            if line.strip() and not line.startswith('#')]
    silesia = None
    bad = 0
    for kind, name, size, sha, url in rows:
        path = os.path.join(root, kind, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if os.path.exists(path):
            data = open(path, 'rb').read()
        elif kind == 'silesia':
            if silesia is None:
                silesia = zipfile.ZipFile(io.BytesIO(fetch(url)))
            data = silesia.read(name)
        else:
            data = fetch(url)
            time.sleep(1)
        if len(data) != int(size) or hashlib.sha256(data).hexdigest() != sha:
            print('MISMATCH %s/%s (%d bytes)' % (kind, name, len(data)))
            bad += 1
            continue
        if not os.path.exists(path):
            open(path, 'wb').write(data)
    print('%d files checked, %d mismatched' % (len(rows), bad))
    sys.exit(1 if bad else 0)


if __name__ == '__main__':
    main()
