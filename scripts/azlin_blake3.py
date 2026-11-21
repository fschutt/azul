#!/usr/bin/env python3
"""BLAKE3 in plain Python (hash mode, 32-byte output), for the scripts that act as "another
device" of a synced folder: the sync names every blob by its BLAKE3 (azcloud-kit's sync, the
plain index `<prefix>.azlin/index.json` and its blobs), and Python's hashlib has no BLAKE3.

A port of the reference implementation (github.com/BLAKE3-team/BLAKE3, reference_impl, CC0 /
Apache-2.0): slow, but the scripts hash a few small files. `python3 scripts/azlin_blake3.py`
checks it against the official test vectors (input: bytes 0, 1, ..., 250, 0, 1, ...).

    import azlin_blake3
    azlin_blake3.hex_digest(b"bytes")   # 64 lowercase hex digits
"""

import sys

OUT_LEN = 32
BLOCK_LEN = 64
CHUNK_LEN = 1024

CHUNK_START = 1 << 0
CHUNK_END = 1 << 1
PARENT = 1 << 2
ROOT = 1 << 3

IV = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
    0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
]
MSG_PERMUTATION = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
MASK = 0xFFFFFFFF


def _rotr(x, n):
    return ((x >> n) | (x << (32 - n))) & MASK


def _g(state, a, b, c, d, mx, my):
    state[a] = (state[a] + state[b] + mx) & MASK
    state[d] = _rotr(state[d] ^ state[a], 16)
    state[c] = (state[c] + state[d]) & MASK
    state[b] = _rotr(state[b] ^ state[c], 12)
    state[a] = (state[a] + state[b] + my) & MASK
    state[d] = _rotr(state[d] ^ state[a], 8)
    state[c] = (state[c] + state[d]) & MASK
    state[b] = _rotr(state[b] ^ state[c], 7)


def _round(state, m):
    _g(state, 0, 4, 8, 12, m[0], m[1])
    _g(state, 1, 5, 9, 13, m[2], m[3])
    _g(state, 2, 6, 10, 14, m[4], m[5])
    _g(state, 3, 7, 11, 15, m[6], m[7])
    _g(state, 0, 5, 10, 15, m[8], m[9])
    _g(state, 1, 6, 11, 12, m[10], m[11])
    _g(state, 2, 7, 8, 13, m[12], m[13])
    _g(state, 3, 4, 9, 14, m[14], m[15])


def _compress(chaining_value, block_words, counter, block_len, flags):
    state = [
        chaining_value[0], chaining_value[1], chaining_value[2], chaining_value[3],
        chaining_value[4], chaining_value[5], chaining_value[6], chaining_value[7],
        IV[0], IV[1], IV[2], IV[3],
        counter & MASK, (counter >> 32) & MASK, block_len, flags,
    ]
    block = list(block_words)
    for i in range(7):
        _round(state, block)
        if i < 6:
            block = [block[p] for p in MSG_PERMUTATION]
    for i in range(8):
        state[i] ^= state[i + 8]
        state[i + 8] ^= chaining_value[i]
    return state


def _words(block):
    block = block + bytes(BLOCK_LEN - len(block))
    return [int.from_bytes(block[i:i + 4], "little") for i in range(0, BLOCK_LEN, 4)]


class _Output:
    def __init__(self, cv, words, counter, block_len, flags):
        self.cv, self.words, self.counter = cv, words, counter
        self.block_len, self.flags = block_len, flags

    def chaining_value(self):
        return _compress(self.cv, self.words, self.counter, self.block_len, self.flags)[:8]

    def root_bytes(self):
        out = _compress(self.cv, self.words, 0, self.block_len, self.flags | ROOT)
        return b"".join(w.to_bytes(4, "little") for w in out)[:OUT_LEN]


def _chunk_output(chunk, counter):
    """The output of one chunk (up to 1024 bytes) whose index is `counter`."""
    cv = list(IV)
    blocks = [chunk[i:i + BLOCK_LEN] for i in range(0, len(chunk), BLOCK_LEN)] or [b""]
    for n, block in enumerate(blocks):
        flags = CHUNK_START if n == 0 else 0
        if n == len(blocks) - 1:
            flags |= CHUNK_END
            return _Output(cv, _words(block), counter, len(block), flags)
        cv = _compress(cv, _words(block), counter, BLOCK_LEN, flags)[:8]
    raise AssertionError("unreachable")


def _parent_output(left_cv, right_cv):
    return _Output(list(IV), list(left_cv) + list(right_cv), 0, BLOCK_LEN, PARENT)


def digest(data):
    """The 32-byte BLAKE3 hash of `data`."""
    data = bytes(data)
    chunks = [data[i:i + CHUNK_LEN] for i in range(0, len(data), CHUNK_LEN)] or [b""]
    # The chaining values of the complete subtrees so far, as the reference's stack keeps them.
    stack = []
    for counter, chunk in enumerate(chunks[:-1]):
        cv = _chunk_output(chunk, counter).chaining_value()
        total = counter + 1
        while total & 1 == 0:
            cv = _parent_output(stack.pop(), cv).chaining_value()
            total >>= 1
        stack.append(cv)
    output = _chunk_output(chunks[-1], len(chunks) - 1)
    while stack:
        output = _parent_output(stack.pop(), output.chaining_value())
    return output.root_bytes()


def hex_digest(data):
    """The BLAKE3 hash of `data` in lowercase hex (the sync's blob names)."""
    return digest(data).hex()


# The official vectors' hashes (input i % 251) of some lengths.
VECTORS = {
    0: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
    1: "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213",
    1023: "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11",
    1024: "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
    1025: "d00278ae47eb27b34faecf67b4fe263f82d5412916c1ffd97c8cb7fb814b8444",
    2049: "5f4d72f40d7a5f82b15ca2b2e44b1de3c2ef86c426c95c1af0b6879522563030",
    5000: "ee78d92070de3df1c57c37002abf0a6b1a6589acdeef4d8ffac7cf3d9e8f2836",
    31744: "62b6960e1a44bcc1eb1a611a8d6235b6b4b78f32e7abc4fb4c6cdcce94895c47",
}


def main():
    bad = 0
    for length, want in VECTORS.items():
        got = hex_digest(bytes(i % 251 for i in range(length)))
        ok = got == want
        bad += 0 if ok else 1
        print("%s %6d %s" % ("ok  " if ok else "FAIL", length, got))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
