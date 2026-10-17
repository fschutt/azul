//! git-remote-azlin's protocol and its packs: what git reads.

use std::{collections::HashSet, io::Cursor};

use sha2::{Digest, Sha256};

use crate::meta::{
    git::{adler32, git_pack, key_from_hex, reachable, serve, serve_bucket, zlib_stored},
    merge::keep_both,
    Change, Kind, MemoryBucket, MetaRepo, ObjectId, TestSealer,
};

type Repo = MetaRepo<MemoryBucket, TestSealer>;

/// Inflates a zlib stream of stored blocks, checking its header, every block's length and
/// the Adler-32; the bytes and how much of `data` the stream took.
fn inflate_stored(data: &[u8]) -> (Vec<u8>, usize) {
    assert_eq!(&data[..2], &[0x78, 0x01], "a zlib header");
    let mut at = 2;
    let mut out = Vec::new();
    loop {
        let header = data[at];
        at += 1;
        assert_eq!(header & 0x06, 0, "a stored block");
        let len = u16::from_le_bytes([data[at], data[at + 1]]);
        let nlen = u16::from_le_bytes([data[at + 2], data[at + 3]]);
        assert_eq!(len, !nlen);
        at += 4;
        out.extend_from_slice(&data[at..at + len as usize]);
        at += len as usize;
        if header & 1 == 1 {
            break;
        }
    }
    let adler = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    assert_eq!(adler, adler32(&out));
    (out, at + 4)
}

/// The objects of a git pack (version 2, no deltas): type number and body; its trailer
/// checked.
fn read_pack(pack: &[u8]) -> Vec<(u8, Vec<u8>)> {
    assert_eq!(&pack[..4], b"PACK");
    assert_eq!(u32::from_be_bytes([pack[4], pack[5], pack[6], pack[7]]), 2);
    let count = u32::from_be_bytes([pack[8], pack[9], pack[10], pack[11]]);
    let mut at = 12;
    let mut out = Vec::new();
    for _ in 0..count {
        let mut byte = pack[at];
        at += 1;
        let kind = (byte >> 4) & 7;
        let mut size = usize::from(byte & 0x0f);
        let mut shift = 4;
        while byte & 0x80 != 0 {
            byte = pack[at];
            at += 1;
            size |= usize::from(byte & 0x7f) << shift;
            shift += 7;
        }
        let (body, used) = inflate_stored(&pack[at..]);
        at += used;
        assert_eq!(body.len(), size);
        out.push((kind, body));
    }
    assert_eq!(&pack[at..], Sha256::digest(&pack[..at]).as_slice(), "the trailer");
    out
}

fn drive() -> Repo {
    let mut repo = Repo::create(MemoryBucket::new(), TestSealer::new([71; 32]), "laptop", "Laptop")
        .unwrap();
    for (i, path) in ["docs/a.txt", "docs/b.txt", "top.txt"].iter().enumerate() {
        let id = repo.write_blob(format!("pointer {i}\n").as_bytes());
        repo.commit(
            &[Change::Put {
                path: path.to_string(),
                id,
            }],
            &format!("add {path}\n"),
            &mut keep_both,
        )
        .unwrap();
    }
    repo
}

#[test]
fn adler32_is_zlibs_checksum() {
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    assert_eq!(adler32(b""), 1);
}

#[test]
fn stored_zlib_is_a_stream_any_inflater_reads() {
    for data in [Vec::new(), b"hello".to_vec(), vec![7u8; 70_000]] {
        let stream = zlib_stored(&data);
        let (inflated, used) = inflate_stored(&stream);
        assert_eq!(inflated, data);
        assert_eq!(used, stream.len());
    }
}

#[test]
fn a_git_pack_holds_every_object_the_head_reaches_under_its_id() {
    let repo = drive();
    let head = repo.head().unwrap();
    let ids = reachable(repo.objects(), &[head]).unwrap();
    // Three commits, their root trees, the docs trees, three blobs.
    assert!(ids.len() >= 9, "{}", ids.len());
    let pack = git_pack(repo.objects(), &ids).unwrap();
    let objects = read_pack(&pack);
    assert_eq!(objects.len(), ids.len());
    let wanted: HashSet<ObjectId> = ids.iter().copied().collect();
    for (code, body) in objects {
        let kind = Kind::from_code(code).unwrap();
        assert!(wanted.contains(&ObjectId::of(kind, &body)));
    }
}

#[test]
fn the_helper_answers_capabilities_list_and_fetch_as_git_asks() {
    let mut repo = drive();
    let head = repo.head().unwrap().to_hex();
    let script = format!(
        "capabilities\noption object-format true\noption progress true\nlist\n\
         fetch {head} refs/heads/main\n\n"
    );
    let mut output = Vec::new();
    let mut packs: Vec<Vec<u8>> = Vec::new();
    serve(&mut repo, Cursor::new(script), &mut output, &mut |pack: &[u8]| {
        packs.push(pack.to_vec());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        format!(
            "fetch\noption\nobject-format\n\nok\nunsupported\n:object-format sha256\n\
             {head} refs/heads/main\n@refs/heads/main HEAD\n\n\n"
        )
    );
    assert_eq!(packs.len(), 1);
    let objects = read_pack(&packs[0]);
    let commits: Vec<String> = objects
        .iter()
        .filter(|(code, _)| *code == Kind::Commit.code())
        .map(|(_, body)| ObjectId::of(Kind::Commit, body).to_hex())
        .collect();
    assert_eq!(commits.len(), 3);
    assert!(commits.contains(&head));
}

/// git 2.39 asks `option object-format` without a value (checked with a stand-in helper and
/// the real `git clone`); it gives up on anything but `ok`.
#[test]
fn the_helper_says_ok_to_the_object_format_option_git_sends_without_a_value() {
    let mut repo = drive();
    let mut output = Vec::new();
    serve(
        &mut repo,
        Cursor::new("option object-format\nlist\n\n"),
        &mut output,
        &mut |_: &[u8]| Ok(()),
    )
    .unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.starts_with("ok\n:object-format sha256\n"), "{text}");
}

/// The helper's whole run, as `git-remote-azlin` and `azcloud git-remote` make it: the
/// repository straight from its bucket (S3's or a folder's), packs read for a fetch only.
#[test]
fn the_helper_serves_a_repository_straight_from_its_bucket() {
    let bucket = MemoryBucket::new();
    let mut laptop =
        Repo::create(bucket.clone(), TestSealer::new([71; 32]), "laptop", "Laptop").unwrap();
    let id = laptop.write_blob(b"pointer\n");
    laptop
        .commit(
            &[Change::Put {
                path: "a.txt".to_string(),
                id,
            }],
            "add a.txt\n",
            &mut keep_both,
        )
        .unwrap();
    let head = laptop.head().unwrap().to_hex();

    let script = format!("option object-format\nlist\nfetch {head} refs/heads/main\n\n");
    let mut output = Vec::new();
    let mut packs = 0;
    serve_bucket(
        bucket.clone(),
        TestSealer::new([71; 32]),
        Cursor::new(script),
        &mut output,
        &mut |pack: &[u8]| {
            assert_eq!(&pack[..4], b"PACK");
            packs += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        format!("ok\n:object-format sha256\n{head} refs/heads/main\n@refs/heads/main HEAD\n\n\n")
    );
    assert_eq!(packs, 1);

    // Another key opens nothing.
    assert!(serve_bucket(
        bucket,
        TestSealer::new([72; 32]),
        Cursor::new("list\n\n"),
        &mut Vec::new(),
        &mut |_: &[u8]| Ok(()),
    )
    .is_err());
}

#[test]
fn a_drive_key_file_holds_64_hex_digits() {
    let key = key_from_hex(&format!("{}\n", "0a".repeat(32))).unwrap();
    assert_eq!(key, [0x0a; 32]);
    assert_eq!(key_from_hex("0a0b"), None);
    assert_eq!(key_from_hex(&"zz".repeat(32)), None);
}
