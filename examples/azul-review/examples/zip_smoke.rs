//! Proves the ZIP surface works ACROSS THE C ABI, not just in Rust.
//!
//! `azul-dll`'s own unit tests call `Zip` as a Rust type; they would pass even
//! if codegen never emitted a single `AzZip_*` symbol. This links against
//! `libazul` the way any other binding does, so it fails if the api.json entry,
//! the generated shim or the export list is wrong - which is the part that
//! actually breaks.
//!
//! Run: `cargo run --release -p AzReview --example zip_smoke`

fn main() {
    let mut z = azul::zip::Zip::new();
    z.add_file("session.json", u8vec(br#"{"format":"azreview/1"}"#));
    z.add_file("clip-0.wav", u8vec(&vec![7u8; 1024]));
    assert_eq!(z.file_count(), 2, "entries did not accumulate");

    let dir = std::env::temp_dir().join("azreview-smoke");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.zip");
    assert!(z.to_file(path.to_string_lossy().as_ref()), "to_file failed");

    let disk = std::fs::read(&path).unwrap();
    assert_eq!(&disk[0..2], b"PK", "not a zip: {:?}", &disk[0..4]);

    let back = azul::zip::Zip::from_file(path.to_string_lossy().as_ref());
    assert_eq!(back.file_count(), 2, "round trip lost entries");
    assert_eq!(
        back.get_file("session.json").as_ref(),
        br#"{"format":"azreview/1"}"#
    );
    assert_eq!(back.get_file("clip-0.wav").as_ref().len(), 1024);
    println!(
        "OK  {} bytes on disk, {} entries back",
        disk.len(),
        back.file_count()
    );
}

fn u8vec(v: &[u8]) -> azul::vec::U8Vec {
    azul::vec::U8Vec::copy_from_bytes(&v[0], 0, v.len())
}
