//! decode(encode(x)) == x for every file of a corpus on disk (the measurement corpus: Silesia,
//! photos, documents), with the upload and the idle options. The corpus is not in the
//! repository: point `AZLIN_COMPRESS_CORPUS` at a folder and run the ignored test.
//!
//!     AZLIN_COMPRESS_CORPUS=/path/to/corpus cargo test -p azlin-compress --test corpus -- --ignored

use std::path::{Path, PathBuf};

use azlin_compress::{decode, encode, Options};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "needs a corpus folder in AZLIN_COMPRESS_CORPUS"]
fn every_corpus_file_round_trips() {
    let Some(root) = std::env::var_os("AZLIN_COMPRESS_CORPUS") else {
        return;
    };
    let mut all = Vec::new();
    files(Path::new(&root), &mut all);
    assert!(!all.is_empty(), "no files under {root:?}");
    let (mut bytes_in, mut bytes_out) = (0u64, 0u64);
    for path in &all {
        let data = std::fs::read(path).unwrap();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        for options in [Options::upload(), Options::idle()] {
            let options = options.with_extension(ext);
            let encoded = encode(&data, &options).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let decoded = decode(&encoded.bytes, &Options::default())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(decoded[..] == data[..], "{}: not bit-exact", path.display());
            bytes_in += data.len() as u64;
            bytes_out += encoded.bytes.len() as u64;
        }
    }
    println!("{} files, {bytes_in} -> {bytes_out} bytes (both option sets)", all.len());
}
