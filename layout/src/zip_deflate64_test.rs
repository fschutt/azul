//! An entry that 7-Zip wrote with Deflate64 (method 9): read by a build that asks for `zip-deflate64`,
//! refused with an error (never a panic) by one that does not.

use super::*;

/// CRC-32 (IEEE 802.3) of `data`, bit by bit: the one entry of the test archive needs its checksum.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A ZIP archive of ONE entry `name`, compressed with `method` into `stream` (`content` is what the
/// entry holds once it is extracted).
fn archive(name: &str, content: &[u8], method: u16, stream: &[u8]) -> Vec<u8> {
    let crc = crc32(content);
    let word = |out: &mut Vec<u8>, value: u32| out.extend_from_slice(&value.to_le_bytes());
    let half = |out: &mut Vec<u8>, value: u16| out.extend_from_slice(&value.to_le_bytes());

    let mut out = Vec::new();
    // the local file header
    word(&mut out, 0x0403_4b50);
    half(&mut out, 21); // version needed to extract: 2.1
    half(&mut out, 0); // flags
    half(&mut out, method);
    half(&mut out, 0); // time
    half(&mut out, 0x21); // date: 1980-01-01
    word(&mut out, crc);
    word(&mut out, stream.len() as u32);
    word(&mut out, content.len() as u32);
    half(&mut out, name.len() as u16);
    half(&mut out, 0); // extra
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(stream);
    // the central directory
    let central_at = out.len();
    word(&mut out, 0x0201_4b50);
    half(&mut out, 0x031e); // made by: Unix, 3.0
    half(&mut out, 21);
    half(&mut out, 0);
    half(&mut out, method);
    half(&mut out, 0);
    half(&mut out, 0x21);
    word(&mut out, crc);
    word(&mut out, stream.len() as u32);
    word(&mut out, content.len() as u32);
    half(&mut out, name.len() as u16);
    half(&mut out, 0); // extra
    half(&mut out, 0); // comment
    half(&mut out, 0); // disk
    half(&mut out, 0); // internal attributes
    word(&mut out, 0o100_644 << 16); // external attributes: a regular file, rw-r--r--
    word(&mut out, 0); // offset of the local header
    out.extend_from_slice(name.as_bytes());
    let central_len = out.len() - central_at;
    // the end of the central directory
    word(&mut out, 0x0605_4b50);
    half(&mut out, 0);
    half(&mut out, 0);
    half(&mut out, 1);
    half(&mut out, 1);
    word(&mut out, central_len as u32);
    word(&mut out, central_at as u32);
    half(&mut out, 0);
    out
}

/// One stored block: BFINAL 1, BTYPE 00, LEN, NLEN and the bytes, which Deflate and Deflate64 read
/// alike (what differs is the window and the length codes of the compressed blocks, so this entry
/// needs no compressor).
fn stored_block(content: &[u8]) -> Vec<u8> {
    let mut stream = vec![0x01];
    stream.extend_from_slice(&(content.len() as u16).to_le_bytes());
    stream.extend_from_slice(&(!(content.len() as u16)).to_le_bytes());
    stream.extend_from_slice(content);
    stream
}

#[test]
fn an_entry_written_with_deflate64_is_read_by_a_build_that_asks_for_it_and_refused_by_one_that_does_not(
) {
    let content = b"an entry that 7-Zip wrote with Deflate64";
    let bytes = archive("note.txt", content, 9, &stored_block(content));
    let result = ZipFile::from_bytes(&bytes, &ZipReadConfig::default());
    #[cfg(feature = "zip-deflate64")]
    {
        let zip = result.expect("a build with zip-deflate64 reads a Deflate64 entry");
        let entry = zip.get("note.txt").expect("the entry is in the archive");
        assert_eq!(entry.data.as_slice(), content.as_slice());
    }
    #[cfg(not(feature = "zip-deflate64"))]
    assert!(
        result.is_err(),
        "no zip-deflate64 feature: method 9 is refused"
    );
}

#[test]
fn the_archive_of_the_test_is_a_well_formed_one_that_every_build_reads_when_the_entry_is_stored() {
    // the same builder with method 0: the entry's refusal above is about the method, not the archive
    let content = b"stored";
    let bytes = archive("plain.txt", content, 0, content);
    let zip = ZipFile::from_bytes(&bytes, &ZipReadConfig::default())
        .expect("a stored entry is read by every build");
    assert_eq!(
        zip.get("plain.txt").expect("the entry").data.as_slice(),
        content.as_slice()
    );
}
