#![deny(missing_docs)]

//! Checksum calculation routines.

use std::num::Wrapping;

use crate::binary::read::ReadScope;
use crate::binary::U32Be;
use crate::error::ParseError;

/// Calculate a checksum of `data` according to the OpenType table checksum algorithm
///
/// <https://docs.microsoft.com/en-us/typography/opentype/spec/otff#calculating-checksums>
pub fn table_checksum(data: &[u8]) -> Result<Wrapping<u32>, ParseError> {
    assert_eq!(data.len() % 4, 0, "data end is not 32-bit aligned");

    let mut ctxt = ReadScope::new(data).ctxt();
    let array = ctxt.read_array::<U32Be>(data.len() / 4)?;
    Ok(array.iter().map(Wrapping).sum())
}

#[cfg(test)]
mod tests {
    use super::Wrapping;

    #[test]
    fn test_table_checksum() {
        let data = [0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4];

        assert_eq!(super::table_checksum(&data).unwrap(), Wrapping(10));
    }

    #[test]
    fn test_table_checksum_overflow() {
        let data = [0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 2];

        assert_eq!(super::table_checksum(&data).unwrap(), Wrapping(1));
    }
}
