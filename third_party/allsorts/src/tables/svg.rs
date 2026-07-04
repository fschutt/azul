#![deny(missing_docs)]

//! `SVG` table parsing.
//!
//! <https://docs.microsoft.com/en-us/typography/opentype/spec/SVG>

use std::io::Read;

use flate2::read::GzDecoder;

use crate::binary::read::{
    ReadArray, ReadBinary, ReadBinaryDep, ReadCtxt, ReadFixedSizeDep, ReadScope,
};
use crate::bitmap::{
    Bitmap, BitmapGlyph, EncapsulatedBitmap, EncapsulatedFormat, Metrics, OriginOffset,
};
use crate::error::ParseError;
use crate::size;

const GZIP_HEADER: &[u8] = &[0x1F, 0x8B, 0x08];

/// Holds the records from the `SVG` table.
pub struct SvgTable<'a> {
    /// The version of the table. Only version `0` is supported.
    pub version: u16,
    /// The SVG document records.
    ///
    /// **Example:**
    ///
    /// ```ignore
    /// for record in svg.document_records.iter_res() {
    ///     let record = record?;
    ///     // Use record here
    /// }
    /// ```
    pub document_records: ReadArray<'a, SVGDocumentRecord<'a>>,
}

/// One SVG record holding a glyph range and `SVGDocumentRecord`.
pub struct SVGDocumentRecord<'a> {
    /// The starting glyph id.
    pub start_glyph_id: u16,
    /// The end glyph id.
    ///
    /// Can be the same as `start_glyph_id`.
    pub end_glyph_id: u16,
    /// The SVG document data. Possibly compressed.
    ///
    /// If the data is compressed it will begin with 0x1F, 0x8B, 0x08, which is a gzip member
    /// header indicating "deflate" as the compression method. See section 2.3.1 of
    /// <https://www.ietf.org/rfc/rfc1952.txt>
    pub svg_document: &'a [u8],
}

impl<'a> SvgTable<'a> {
    /// Locate the SVG record for the supplied `glyph_id`.
    pub fn lookup_glyph(&self, glyph_id: u16) -> Result<Option<SVGDocumentRecord<'a>>, ParseError> {
        for record in self.document_records.iter_res() {
            let record = record?;
            if glyph_id >= record.start_glyph_id && glyph_id <= record.end_glyph_id {
                return Ok(Some(record));
            }
        }
        Ok(None)
    }
}

impl ReadBinary for SvgTable<'_> {
    type HostType<'a> = SvgTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let version = ctxt.read_u16be()?;
        ctxt.check(version == 0)?;
        let document_records_offset = usize::try_from(ctxt.read_u32be()?)?;

        let records_scope = scope.offset(document_records_offset);
        let mut records_ctxt = records_scope.ctxt();
        let num_records = records_ctxt.read_u16be().map(usize::from)?;
        let document_records = records_ctxt.read_array_dep(num_records, records_scope)?;

        Ok(SvgTable {
            version,
            document_records,
        })
    }
}

impl ReadBinaryDep for SVGDocumentRecord<'_> {
    type Args<'a> = ReadScope<'a>;
    type HostType<'a> = SVGDocumentRecord<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        scope: ReadScope<'a>,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let start_glyph_id = ctxt.read_u16be()?;
        let end_glyph_id = ctxt.read_u16be()?;
        let svg_doc_offset = usize::try_from(ctxt.read_u32be()?)?;
        let svg_doc_length = usize::try_from(ctxt.read_u32be()?)?;
        let svg_data = scope.offset_length(svg_doc_offset, svg_doc_length)?;
        let svg_document = svg_data.data();

        Ok(SVGDocumentRecord {
            start_glyph_id,
            end_glyph_id,
            svg_document,
        })
    }
}
impl ReadFixedSizeDep for SVGDocumentRecord<'_> {
    fn size(_: Self::Args<'_>) -> usize {
        // uint16   startGlyphID
        // uint16   endGlyphID
        // Offset32 svgDocOffset
        // uint32   svgDocLength
        // — https://docs.microsoft.com/en-us/typography/opentype/spec/svg#svg-document-list
        (2 * size::U16) + (2 * size::U32)
    }
}

impl<'a> TryFrom<(&SVGDocumentRecord<'a>, u16)> for BitmapGlyph {
    type Error = ParseError;

    fn try_from(
        (svg_record, bitmap_id): (&SVGDocumentRecord<'a>, u16),
    ) -> Result<Self, ParseError> {
        // If the document is compressed then inflate it. &[0x1F, 0x8B, 0x08] is a gzip member
        // header indicating "deflate" as the compression method. See section 2.3.1 of
        // https://www.ietf.org/rfc/rfc1952.txt
        let data = if svg_record.svg_document.starts_with(GZIP_HEADER) {
            let mut gz = GzDecoder::new(svg_record.svg_document);
            let mut uncompressed = Vec::with_capacity(svg_record.svg_document.len());
            gz.read_to_end(&mut uncompressed)
                .map_err(|_err| ParseError::CompressionError)?;
            uncompressed.into_boxed_slice()
        } else {
            Box::from(svg_record.svg_document)
        };

        let encapsulated = EncapsulatedBitmap {
            format: EncapsulatedFormat::Svg,
            data,
        };
        Ok(BitmapGlyph {
            bitmap: Bitmap::Encapsulated(encapsulated),
            bitmap_id,
            metrics: Metrics::HmtxVmtx(OriginOffset { x: 0, y: 0 }),
            ppem_x: None,
            ppem_y: None,
            should_flip_hori: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_data::FontData;
    use crate::tables::FontTableProvider;
    use crate::tag;
    use crate::tests::read_fixture;

    #[test]
    fn test_read_svg() {
        let buffer = read_fixture("tests/fonts/opentype/TwitterColorEmoji-SVGinOT.ttf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope
            .read::<FontData<'_>>()
            .expect("unable to parse font file");
        let table_provider = font_file
            .table_provider(0)
            .expect("unable to create font provider");
        let svg_data = table_provider
            .read_table_data(tag::SVG)
            .expect("unable to read SVG table data");
        let svg = ReadScope::new(&svg_data).read::<SvgTable<'_>>().unwrap();

        let records = svg
            .document_records
            .iter_res()
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(records.len(), 3075);

        let record = &records[0];
        assert_eq!(record.start_glyph_id, 5);
        assert_eq!(record.end_glyph_id, 5);
        assert_eq!(record.svg_document.len(), 751);
        let doc = std::str::from_utf8(record.svg_document).unwrap();
        assert_eq!(&doc[0..43], "<?xml version='1.0' encoding='UTF-8'?>\n<svg");
    }

    #[test]
    fn test_read_gzipped_svg() {
        let buffer = read_fixture("tests/fonts/svg/gzipped.ttf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope
            .read::<FontData<'_>>()
            .expect("unable to parse font file");
        let table_provider = font_file
            .table_provider(0)
            .expect("unable to create font provider");
        let svg_data = table_provider
            .read_table_data(tag::SVG)
            .expect("unable to read SVG table data");
        let svg = ReadScope::new(&svg_data).read::<SvgTable<'_>>().unwrap();
        let record = svg
            .document_records
            .iter_res()
            .into_iter()
            .nth(0)
            .unwrap()
            .unwrap();

        // Ensure the document is actually compressed
        assert!(record.svg_document.starts_with(GZIP_HEADER));
        // Now test decompression
        match BitmapGlyph::try_from((&record, 0 /* Arbitrary ID */)) {
            Ok(BitmapGlyph {
                bitmap: Bitmap::Encapsulated(EncapsulatedBitmap { data, .. }),
                ..
            }) => {
                let doc = std::str::from_utf8(&data).unwrap();
                assert_eq!(&doc[0..42], r#"<?xml version="1.0" encoding="UTF-8"?><svg"#);
            }
            _ => panic!("did not get expected result"),
        }
    }
}
