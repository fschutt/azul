//! Utilities for obtaining a name from a fonts `name` table.

use crate::binary::read::ReadScope;
use crate::error::ParseError;
use crate::tables::NameTable;
use encoding_rs::{DecoderResult, MACINTOSH, UTF_16BE};
use std::ffi::CString;

pub fn fontcode_get_name(
    name_table_data: &[u8],
    name_id: u16,
) -> Result<Option<CString>, ParseError> {
    let name_table = ReadScope::new(name_table_data).read::<NameTable<'_>>()?;
    let mut best = 0;
    let mut result = None;
    for name_record in &name_table.name_records {
        if name_record.name_id == name_id {
            let offset = usize::from(name_record.offset);
            let length = usize::from(name_record.length);
            let name_data = name_table
                .string_storage
                .offset_length(offset, length)?
                .data();
            if let Some((score, encoding)) = score_encoding(
                name_record.platform_id,
                name_record.encoding_id,
                name_record.language_id,
            ) {
                if best < score {
                    if let Some(name) = decode_name(encoding, name_data) {
                        result = Some(name);
                        best = score;
                    }
                }
            }
        }
    }
    Ok(result)
}

enum NameEncoding {
    Utf16Be,
    AppleRoman,
}

fn score_encoding(
    platform_id: u16,
    encoding_id: u16,
    language_id: u16,
) -> Option<(usize, NameEncoding)> {
    match (platform_id, encoding_id, language_id) {
        // Windows; Unicode full repertoire
        (3, 10, _) => Some((1000, NameEncoding::Utf16Be)),

        // Unicode; Unicode full repertoire
        (0, 6, 0) => Some((900, NameEncoding::Utf16Be)),

        // Unicode; Unicode 2.0 and onwards semantics, Unicode full repertoire
        (0, 4, 0) => Some((800, NameEncoding::Utf16Be)),

        // Windows; Unicode BMP
        (3, 1, 0x409) => Some((750, NameEncoding::Utf16Be)),
        (3, 1, lang) if lang != 0x409 => Some((700, NameEncoding::Utf16Be)),

        // Unicode; Unicode 2.0 and onwards semantics, Unicode BMP only
        (0, 3, 0) => Some((600, NameEncoding::Utf16Be)),

        // Unicode; ISO/IEC 10646 semantics
        (0, 2, 0) => Some((500, NameEncoding::Utf16Be)),

        // Unicode; Unicode 1.1 semantics
        (0, 1, 0) => Some((400, NameEncoding::Utf16Be)),

        // Unicode; Unicode 1.0 semantics
        (0, 0, 0) => Some((300, NameEncoding::Utf16Be)),

        // Windows, Symbol
        (3, 0, _) => Some((200, NameEncoding::Utf16Be)),

        // Apple Roman
        (1, 0, 0) => Some((150, NameEncoding::AppleRoman)),
        (1, 0, lang) if lang != 0 => Some((100, NameEncoding::AppleRoman)),
        _ => None,
    }
}

fn decode_name(encoding: NameEncoding, data: &[u8]) -> Option<CString> {
    let mut decoder = match encoding {
        NameEncoding::Utf16Be => UTF_16BE.new_decoder(),
        NameEncoding::AppleRoman => MACINTOSH.new_decoder(),
    };
    if let Some(size) = decoder.max_utf8_buffer_length(data.len()) {
        let mut s = String::with_capacity(size);
        let (res, _read) = decoder.decode_to_string_without_replacement(data, &mut s, true);
        match res {
            DecoderResult::InputEmpty => CString::new(s).ok(),
            DecoderResult::OutputFull => None, // should not happen
            DecoderResult::Malformed(_, _) => None,
        }
    } else {
        None
    }
}
