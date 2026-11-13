# AzDrive compression format (azlin-compress)

AzDrive compresses every file on the client, before it encrypts it (D17). The server only ever
stores ciphertext. This page specifies the bytes `azlin-compress` (azul, `examples/azlin-compress`) writes: the
per-segment codec bytes (the same table AZL1 uses), the shipped dictionaries, the whole-file
transforms and their records, the pack member records, and the standalone AZC1 container.

All integers are little endian unless a section says otherwise.

## The rule: decoders forever

A file written today must still open in twenty years. So:

- Every format version, transform id and version, record type, codec byte and dictionary id that
  a released build ever **wrote** is decoded by every later build. Decoders are never removed.
- A writer may stop writing something. A reader never stops reading it.
- Code that gives stored bytes their meaning is pinned and frozen:
  - `lepton_jpeg = "=0.5.8"`. Moving to another Lepton version needs either a proof that the new
    version decodes every stream the old one wrote (checked against the golden vectors), or a new
    transform version that keeps the old crate as a second, renamed dependency.
  - `zstd = "=0.14.3"` and `brotli`: standard formats; any later decoder reads them.
  - `examples/azlin-preflate` (vendored, see below): its rebuild turns corrections into deflate
    bytes, so it **is** transform 2 version 1. A change that alters what it rebuilds from any
    valid corrections needs transform 2 version 2, with the version 1 code kept beside it. Changes
    that only refuse damaged input sooner are allowed.
- A shipped dictionary file is never changed. A better dictionary gets a new id; the old one stays
  in `dicts/`, pinned by its SHA-256 in `tests/dicts.rs`.
- `tests/golden.rs` decodes the containers and member records under `tests/golden/`, written by
  format version 1, and compares them with their originals byte for byte. A change that breaks
  one of them fails the build. Vectors are never edited or deleted; a new version adds its own.
- What a reader does not know (a newer version, flag, transform, record type, codec or
  dictionary) it refuses **by name** (`Error::Unsupported`). It never guesses and never skips.
- The writer never writes beyond what `Limits::default()` decodes (JPEG images of at most
  `MAX_JPEG_PIXELS`, 26 million; deflate plain text of at most 64 MiB per stream). A reader that
  passes smaller limits may refuse large files; with the defaults it reads every file.

## Segment codec bytes

A file, or a transform's output, is cut into segments (1 MiB by default). Each segment is stored
with its codec byte first:

| byte | codec | the data after the byte |
|---|---|---|
| 0 | stored | the segment as it is |
| 1 | zstd | one zstd frame, any level (1 to 19), no dictionary |
| 2 | brotli | one brotli stream, any quality, window 2^16 to 2^24 |
| 3 | reserved (JPEG XL) | never written; refused by name |
| 4 | zstd with a shipped dictionary | the dictionary id (u16), then one zstd frame |
| 5 | a whole AZC1 container | pack member records only, never in a segment (see below) |
| 6 to 254 | free | refused as a newer version |
| 255 | end of the segments (AZC1 only) | the end record |

A compressed segment is kept only when it is at least 5 % smaller than the segment. Otherwise it
is stored. The writer decodes every compressed segment and compares it with the input before it
keeps it.

Bytes 0, 1 and 2 mean exactly what they mean in AZL1 (azul-storage `crypto/codec.rs`) today. AZL1
takes byte 4 over unchanged.

## Shipped dictionaries

| id | kind | file | trained on (public corpora only) |
|---|---|---|---|
| 1 | JSON | `dicts/json-1.zdict` | SchemaStore's schemas and test files (Apache-2.0) |
| 2 | XML, HTML, SVG | `dicts/markup-1.zdict` | the XML parts of Apache POI's test documents (Apache-2.0), Project Gutenberg HTML |
| 3 | other text | `dicts/text-1.zdict` | Project Gutenberg texts, Markdown |
| 4 | source code | `dicts/code-1.zdict` | Rust crate sources and zstd's C sources (MIT / Apache-2.0 / BSD) |

Each is 64 KiB, trained by `tools/train_dicts.py` (`zstd --train`, fixed parameters). Small text
files (up to 128 KiB) try their kind's dictionary as a third candidate, next to plain zstd and
brotli; the smallest is kept. CSV has no dictionary: one trained on public tables lost to plain
zstd on held-out tables. A dictionary contains pieces of its training data, so none is ever
trained on anybody's files.

## Transforms

A transform rewrites the whole file into a stream that compresses better, and back, bit for
bit. It runs before the file is cut into segments, because what it reaches (a whole JPEG image,
a whole deflate stream) crosses segment boundaries.

| id | version | transform | for |
|---|---|---|---|
| 0 | 0 | none | everything else |
| 1 | 1 | JPEG images through Lepton (`lepton_jpeg` 0.5.8) | JPEG files; TIFF and camera RAW files (their JPEG previews) |
| 2 | 1 | containers: deflate streams rebuilt by azlin-preflate, JPEG images through Lepton | PDF, ZIP and the formats built on it (DOCX, XLSX, PPTX, ODF, EPUB, JAR), PNG, gzip |

The writer keeps a transform only after it has decoded the transform's output and compared it
with the input. A piece that does not come back (an image, a stream) becomes a literal and the
stream is checked again; on any other error, mismatch, time budget or memory cap the file goes
through with no transform.

### The records (transforms 1 and 2)

The transformed stream is a list of records that covers the file from its first byte to its
last:

```text
0x00 literal   u32 length, then that many bytes, copied as they are
0x01 lepton    u32 length of the JPEG, u16 width, u16 height, u32 length of the Lepton data,
               then the Lepton data (lepton_jpeg's format)
0x02 deflate   u32 length of the deflate stream it rebuilds, u32 length of the reconstruction
               data, the reconstruction data, u32 length of the plain text's records, then the
               plain text as records (literal and lepton only, closed by their own 0xFF)
0x03 png idat  u16 number of IDAT chunks (1 to 65535), u32 data length of each chunk, u32 length
               of the joined data's records, then the chunks' data joined, as records (literal
               and deflate only, closed by their own 0xFF)
0xFF end       nothing may follow
```

Transform 1 reads and writes records 0x00 and 0x01 only; transform 2 all four, nested as shown.
Any other record type, or one where it may not be, is refused.

- **Lepton** (0x01). A JPEG file is usually one Lepton record. A second picture that phones append
  (a gain map, a depth map) gets its own. Inside other files every JPEG of at least 16 KiB is
  tried. The decoder refuses a record before it allocates for it when it would take the file
  past `max_decoded_len`, or when width x height is over `max_jpeg_pixels`. It passes width,
  height and length to Lepton as hard limits and checks that the JPEG it gets back has exactly
  the length the record says.
- **Deflate** (0x02). Where the streams are: ZIP local file headers with method 8 (not
  encrypted); PDF `stream` keywords followed by a zlib header (FlateDecode); gzip members. The
  reconstruction data is 48 bytes of predictor parameters (layout version 1: flags, strategy,
  block type strategy, window bits, nice length, the largest token count, the farthest
  3-byte match, the chain depth, the shortest match, the dictionary add policy, the matching type
  and the hash algorithm, each checked on read) followed by the corrections, coded with an
  adaptive binary range coder (11-bit probabilities, the one LZMA uses). Rebuilding runs the
  predictor over the plain text and applies the corrections; the result must be exactly the
  stated length. The plain text of one record is at most 64 MiB, and pictures in it (a DOCX's
  deflated JPEG) are Lepton records.
- **PNG IDAT** (0x03). A PNG's IDAT chunks hold one zlib stream cut into pieces. The record keeps
  the pieces' lengths; the joined data is the zlib header as a literal, the deflate stream as a
  deflate record, and the Adler-32 (and anything after it) as a literal. The decoder writes every
  chunk back with its length (big endian), `IDAT`, its data and its CRC-32. Only the first run of
  IDAT chunks is transformed; every other chunk is literal.

A stream stays literal when its plain text does not compress with zstd level 3 to at least 2 %
less than the stream plus its reconstruction data, or when it is under 256 bytes.

Lepton's output is dense, so the segments of transform 1 are stored without a trial. The plain
text of transform 2 is compressed per segment as usual, also for PNG and gzip files.

## Pack member records

Small files (AzDrive's packs: under 512 KiB, several to an object) are one record each: a codec
byte, then data. The codec bytes are the segment table's 0, 1, 2 and 4, where the record is one
segment of the whole file; and 5, where the data is a whole AZC1 container (a small JPEG, PNG or
document whose transform paid). The pack entry knows the file's length: a record never decodes
to more.

## The AZC1 container

AZC1 is the standalone form: one file in one byte string. The tests, the fuzzers, pack member
records (codec 5) and any caller without AZL1's framing use it. AZL1 objects (azul-storage) do
not use AZC1: an untransformed object stays AZL1 version 1, byte for byte, with the codec byte
(0, 1, 2, 4) first in each sealed segment's plaintext. A transformed object is AZL1 version 2
(azul-storage `crypto/azl1.rs`): its segments hold the transformed stream, and one more block in
the header - its 24-byte nonce at byte 300, its 48 sealed bytes and tag at 324..388 - is sealed
with the info key, the associated data being the object's context (header bytes 0..88) followed
by `AZL1 transform v2`. It carries the transform id (u8), its version (u8), six zero bytes, the
transformed length (u64 LE) and the transformed stream's BLAKE3 (32 bytes). Its info's size and
BLAKE3 stay those of the original file; its segment count is the transformed stream's. A reader
checks the transformed stream's BLAKE3, runs the inverse transform (never past the sealed size,
on files of at most 64 MiB), then checks the original's.

```text
[header]
   0  4  magic "AZC1"
   4  1  format version (1)
   5  1  flags (0; a set bit is a newer version's)
   6  1  transform id
   7  1  transform version
   8  4  segment size: transformed bytes per segment (1 KiB to 16 MiB)
  12  2  length of the transform's parameters, then the parameters (no transform of version 1
         has any: 0, and a reader of version 1 refuses any other length by name)
[segments], each:
   0  1  codec byte (0, 1, 2 or 4)
   1  4  its transformed bytes: the segment size; less in the last segment; never 0
   5  4  its stored bytes: equal for codec 0, fewer for the others
   9     the stored bytes
[end]
   0  1  0xFF
   1  8  transformed length (the sum of the segments' bytes)
   9  8  original length
  17 32  BLAKE3 of the original
```

The lengths and the hash come last, so a stream can be written in one pass. A decoder:

- checks the header and refuses unknown versions, flags, transforms and parameters by name;
- checks each segment's lengths before it allocates (at most one segment, 16 MiB);
- stops reading segments past the most a transformed stream may hold for its transform (the
  file's cap without a transform; a little more for transform 1; 1032 times it for transform 2,
  deflate's largest expansion);
- decodes the transform from the segments as they come;
- at the end, compares the transformed length, the original length and the BLAKE3 of what it
  produced with the end record.

A damaged container is therefore an error, never other bytes. Bytes after the end record are an
error too.

## Golden vectors of format version 1

`examples/azlin-compress/tests/golden/`: each `<name>.orig` is an original and `<name>.azc1` its
container (`<name>.member` a member record). `tests/golden.rs` pins each original's BLAKE3, the
transform and the codec bytes each container uses, and that the folder holds nothing else.

TODO(final round): the vectors are not written yet (the crate had no build when this version was
specified). `AZC_WRITE_GOLDEN=1 cargo test -p azlin-compress --test golden -- --ignored --nocapture`
writes them once; their BLAKE3 lines and codec bytes go into the test's table, and both are
committed. Until then `tests/golden.rs` fails, as a RED test should.

| vector | what it covers |
|---|---|
| v1-stored | codec 0, no transform |
| v1-zstd | codec 1 over several 4 KiB segments with a short last one |
| v1-brotli | codec 2 |
| v1-dictionary | codec 4 with dictionary 1 (JSON) |
| v1-jpeg | transform 1: one Lepton record |
| v1-gzip | transform 2: a deflate record between the gzip header and trailer literals |
| v1-png | transform 2: a PNG IDAT record holding a deflate record |
| v1-docx | transform 2: deflate records of ZIP members, one holding a Lepton record |
| v1-pdf | transform 2: a FlateDecode deflate record and a DCTDecode Lepton record |
| v1-member-dictionary | member record, codec 4 |
| v1-member-jpeg | member record, codec 5 (an AZC1 container with transform 1) |

## The vendored deflate reconstruction (azlin-preflate)

`examples/azlin-preflate` is preflate-rs 0.7.6 (Microsoft, Apache-2.0) with three changes:

- the corrections are coded with an LZMA-style range coder (adapted from lzma-rs 0.3.0, MIT)
  instead of the `cabac` crate. Every published preflate-rs version (0.1.0 to 0.7.6) depends on
  `cabac`, and every `cabac` release is LGPL-3.0-or-later, which this project does not link. So
  the crates.io crate cannot be used;
- the predictor parameters are written as the explicit 48 bytes above instead of with `bitcode`;
- the rebuild refuses damaged corrections (out-of-range lengths and counts, oversubscribed Huffman
  codes, output past the stated length, corrections that run out) with an error instead of a
  panic or an endless loop.

Its output is not compatible with upstream preflate-rs, and does not need to be: only this
format reads it.

## Version history

| date | what changed |
|---|---|
| 2026-10-24 | AZC1 format version 1: codec bytes 0, 1, 2, 4 (member records also 5); dictionaries 1 to 4; transform 1 version 1 (Lepton 0.5.8); transform 2 version 1 (azlin-preflate from preflate-rs 0.7.6, and Lepton 0.5.8); golden vectors v1-* |
