use crate::meta::{
    keys, Kind, MetaError, Mode, ObjectId, Objects, PackIndex, PackWriter, TestSealer, Tree,
    TreeEntry,
};

const KEY: [u8; 32] = [3; 32];

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A folder `Holiday photos` with a pointer-sized file, and the root above it.
fn sample() -> PackWriter {
    let mut writer = PackWriter::new();
    let blob = writer.add(Kind::Blob, b"object data/ab/0123\nsize 4711\n".to_vec());
    let mut folder = Tree::new();
    folder
        .insert(TreeEntry {
            name: "beach.jpg".to_string(),
            mode: Mode::File,
            id: blob,
        })
        .unwrap();
    let folder_id = writer.add(Kind::Tree, folder.encode());
    let mut root = Tree::new();
    root.insert(TreeEntry {
        name: "Holiday photos".to_string(),
        mode: Mode::Tree,
        id: folder_id,
    })
    .unwrap();
    writer.add(Kind::Tree, root.encode());
    writer
}

#[test]
fn a_pack_brings_back_every_object_it_was_given() {
    let sealer = TestSealer::new(KEY);
    let writer = sample();
    let sealed = writer.seal(&sealer).unwrap();
    assert_eq!(sealed.objects, 3);
    let index = PackIndex::open(&sealer, &sealed.name, &sealed.idx).unwrap();
    assert_eq!(index.entries().len(), 3);
    let mut objects = Objects::new();
    index.read_into(&sealer, &sealed.pack, &mut objects).unwrap();
    assert_eq!(objects.len(), 3);
    for entry in index.entries() {
        let (kind, body) = objects.get(&entry.id).unwrap();
        assert_eq!(kind, entry.kind);
        assert_eq!(ObjectId::of(kind, body), entry.id);
    }
}

#[test]
fn the_same_objects_make_the_same_pack_name_on_every_device_holding_the_key() {
    let a = sample().name(&TestSealer::new(KEY));
    let b = sample().name(&TestSealer::new(KEY));
    let other_key = sample().name(&TestSealer::new([4; 32]));
    assert_eq!(a, b);
    assert_ne!(a, other_key);
    assert_eq!(a.len(), 64);
    let mut more = sample();
    more.add(Kind::Blob, b"another".to_vec());
    assert_ne!(more.name(&TestSealer::new(KEY)), a);
}

#[test]
fn neither_the_pack_nor_its_index_shows_a_name_or_a_git_id() {
    let sealer = TestSealer::new(KEY);
    let writer = sample();
    let sealed = writer.seal(&sealer).unwrap();
    let index = PackIndex::open(&sealer, &sealed.name, &sealed.idx).unwrap();
    for bytes in [&sealed.pack, &sealed.idx] {
        assert!(!contains(bytes, b"Holiday"));
        assert!(!contains(bytes, b"beach"));
        assert!(!contains(bytes, b"data/ab"));
        for entry in index.entries() {
            assert!(!contains(bytes, &entry.id.0));
            assert!(!contains(bytes, entry.id.to_hex().as_bytes()));
        }
    }
    for entry in index.entries() {
        assert!(!sealed.name.contains(&entry.id.to_hex()[..16]));
    }
}

#[test]
fn a_big_pack_is_cut_into_chunks_that_open_one_at_a_time() {
    let sealer = TestSealer::new(KEY);
    let mut writer = PackWriter::new();
    let mut ids = Vec::new();
    for i in 0..300u32 {
        let body = format!("object data/{i:04}\n").repeat(100).into_bytes();
        ids.push(writer.add(Kind::Blob, body));
    }
    let sealed = writer.seal(&sealer).unwrap();
    let index = PackIndex::open(&sealer, &sealed.name, &sealed.idx).unwrap();
    assert!(index.chunk_count() > 1, "{} chunks", index.chunk_count());
    assert_eq!(index.pack_len(), sealed.pack.len() as u64);

    // One object through one ranged read of its chunk (what C6 does).
    let wanted = index.find(&ids[123]).copied().unwrap();
    let range = index.chunk_range(wanted.chunk).unwrap();
    let sealed_chunk = &sealed.pack[range.start as usize..=range.end.unwrap() as usize];
    let chunk = index.open_chunk(&sealer, wanted.chunk, sealed_chunk).unwrap();
    let body = index.object_in_chunk(&wanted, &chunk).unwrap();
    assert_eq!(body, format!("object data/{:04}\n", 123).repeat(100).into_bytes());
}

#[test]
fn a_chunk_moved_to_another_place_does_not_open() {
    let sealer = TestSealer::new(KEY);
    let mut writer = PackWriter::new();
    for i in 0..300u32 {
        writer.add(Kind::Blob, format!("{i}").repeat(500).into_bytes());
    }
    let sealed = writer.seal(&sealer).unwrap();
    let index = PackIndex::open(&sealer, &sealed.name, &sealed.idx).unwrap();
    let range = index.chunk_range(1).unwrap();
    let chunk_one = &sealed.pack[range.start as usize..=range.end.unwrap() as usize];
    assert!(index.open_chunk(&sealer, 1, chunk_one).is_ok());
    assert!(matches!(
        index.open_chunk(&sealer, 0, chunk_one),
        Err(MetaError::Sealed { .. })
    ));
}

#[test]
fn a_cut_or_changed_pack_is_refused() {
    let sealer = TestSealer::new(KEY);
    let sealed = sample().seal(&sealer).unwrap();
    let index = PackIndex::open(&sealer, &sealed.name, &sealed.idx).unwrap();
    let mut objects = Objects::new();
    let cut = &sealed.pack[..sealed.pack.len() - 1];
    assert!(index.read_into(&sealer, cut, &mut objects).is_err());
    let mut changed = sealed.pack.clone();
    changed[30] ^= 1;
    assert!(index.read_into(&sealer, &changed, &mut objects).is_err());
    assert!(objects.is_empty());
}

#[test]
fn an_index_does_not_open_under_another_packs_name() {
    let sealer = TestSealer::new(KEY);
    let sealed = sample().seal(&sealer).unwrap();
    let mut other = sample();
    other.add(Kind::Blob, b"x".to_vec());
    let other_name = other.name(&sealer);
    assert!(matches!(
        PackIndex::open(&sealer, &other_name, &sealed.idx),
        Err(MetaError::Sealed { .. })
    ));
    // The keys the pack and its index go to.
    assert_eq!(keys::pack(&sealed.name), format!(".azlin/meta/wal/{}.pack", sealed.name));
    assert_eq!(keys::idx(&sealed.name), format!(".azlin/meta/wal/{}.idx", sealed.name));
}

#[test]
fn an_empty_pack_is_refused() {
    let sealer = TestSealer::new(KEY);
    assert!(PackWriter::new().seal(&sealer).is_err());
}
