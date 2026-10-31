# azlin-compress

AzDrive's lossless compression, on the client only. The server stores ciphertext and never sees a
plaintext byte, so whatever saves space happens here, before encryption. Every
file comes back bit for bit: each transform and each compressed segment is decoded and compared
with its input before it is kept.

## What it does to a file

1. **Detect** the kind from the first 4 KiB (the extension only breaks ties).
2. **Transform** the whole file when its kind has one (files up to 64 MiB):
   - JPEG photos (and TIFF / camera RAW previews): Lepton (`lepton_jpeg` 0.5.8) re-codes the
     picture's entropy layer, about a fifth smaller, bit-exact;
   - PDF, DOCX / XLSX / PPTX / ODF / EPUB / ZIP, PNG and gzip: their deflate streams become the
     plain text plus a few bytes that rebuild the exact stream (`azlin-preflate`), so zstd and
     brotli compress the text instead of meeting deflated bytes; JPEG pictures inside go
     through Lepton.
3. **Compress per segment** (1 MiB): stored, zstd (the level the time budget allows, up to 19),
   brotli (text, when it is smaller), or zstd with a shipped dictionary (small text files).
   Dense kinds (video, HEIC, archives) are stored without a trial. A segment stays compressed
   only when it saves 5 %.

The result is one self-describing AZC1 container ([format](FORMAT.md)), or,
for AZL1 objects, the transformed stream and the per-segment codec bytes that AZL1 seals (the
format document's "The AZC1 container" section says what an AZL1 object carries).

## Savings

TODO(final round): this crate has not been measured yet. Run the fixed corpus (Silesia, 24
Galaxy S10 JPEGs from Wikimedia Commons, 20 GOV.UK PDF/DOCX/XLSX/PPTX files; 282 MB) with
`examples/bench.rs` (see "Measuring" below), and replace this section with its table: files, MB,
today's AZL1 saving, this crate's saving, per-file p10 / median / p90, encode and decode MB/s,
files transformed, peak memory.

Until then these are the research's figures (2026-10-22, measured with the upstream tools on comparable files), saving in % of the bytes put in:

| Files | Today's AZL1 | Expected with this crate |
|---|---|---|
| JPEG photos (24 camera originals) | 0 % (skip list) | 21 % (per file 8 to 31 %, median 23 %) |
| PDF | 0 % (skip list) | 30 % |
| DOCX / XLSX / PPTX | 0 % (skip list) | 24 % / 39 % / 45 % |
| PNG | 0 % (skip list) | 24 % |
| Text, code, logs (Silesia text) | 68 % (zstd 3) | 73 to 75 % (idle pass) |
| A mixed family drive (persona model) | 1 % | 11 % (8 to 13 %) |

Advertise per kind ("about 20 % less for JPEG photos, 20 to 40 % for PDF and Office documents")
and show each user their own saving; never a general factor.

## Using it

```rust
use azlin_compress::{decode, encode, plan, Options};

let options = Options::idle().with_extension("pdf");
let encoded = encode(&file, &options)?;          // transform + segments, verified
println!("{} -> {} bytes", encoded.report.original_len, encoded.report.encoded_len);
let back = decode(&encoded.bytes, &Options::default())?;
assert_eq!(&back[..], &file[..]);
```

- `plan(start, len, &options)` says up front whether a transform wants the whole file in memory.
- `StreamEncoder` encodes a stream without a transform, one segment in memory at a time.
- `encode_member` / `decode_member` write and read the records of small packed files.
- `estimate(sample, len, &options)` guesses the saving cheaply, for the UI.
- `Options::upload()` keeps up with a transfer; `Options::idle()` is the recompression pass on an
  idle computer on mains power. Both run every transform; `Limits` caps memory (150 MB by
  default), time and sizes. A piece that does not fit stays as it is.

Decode with `Options::default()` limits (or larger): the writer never writes past what they
read, so every container opens on every device.

## Safety

- `#![forbid(unsafe_code)]` here and in `azlin-preflate`; `lepton_jpeg` forbids it too. libzstd
  (C) is the one non-Rust part, as it already is in azul-storage.
- Every decoder checks lengths before it allocates and returns an error on damaged input; the
  property tests (`tests/`) and the fuzz targets (the private repository's fuzz package: `compress_decode`,
  `compress_roundtrip`, `compress_records`, `preflate_recreate`, `preflate_analyze`) are there
  to hold it to that (TODO(final round): their first runs).
- Lepton and the deflate analysis parse untrusted files. Both run behind `catch_unwind` as a last
  line, with time, memory and pixel caps.

## Decoders forever

Every format version, transform, record type, codec byte and dictionary ever written stays
decodable. `tests/golden.rs` decodes the committed vectors of format version 1 on every build
(TODO(final round): write them once with its ignored writer, then commit them); `tests/dicts.rs`
pins every shipped dictionary. See the format document for the rules
(`lepton_jpeg` pinned exactly; `azlin-preflate`'s rebuild frozen per transform version).

## The dictionaries

`dicts/*.zdict`, trained by `tools/train_dicts.py` on public corpora only: SchemaStore (JSON,
Apache-2.0), Apache POI's test documents (their XML parts, Apache-2.0), Project Gutenberg (texts
and HTML, public domain in the US), and Rust crate and zstd C sources (MIT / Apache-2.0 / BSD). A
dictionary holds pieces of its training data, so none is trained on anybody's files. Held-out
savings at 4 KiB files: JSON 78 -> 85 %, markup 74 -> 90 %, text 53 -> 63 %, code 68 -> 73 %.

## Measuring

```sh
python3 tools/fetch_corpus.py ~/azc-corpus      # the fixed corpus of tools/corpus.tsv, hash-checked
cargo build --release -p azlin-compress --example bench
sh tools/measure.sh ~/azc-corpus idle 4         # the table above, one process per set
```

`examples/bench.rs` prints one table row per set of files (files, MB, today's AZL1 saving, this
crate's saving, per-file p10 / median / p90, encode and decode MB/s, files transformed) and checks
every file bit-exact; `measure.sh` adds each set's peak memory.
