//! Reading of the WOFF font format.

use flate2::bufread::ZlibDecoder;

use crate::binary::read::{ReadArray, ReadBinary, ReadBuf, ReadCtxt, ReadFrom, ReadScope};
use crate::binary::U32Be;
use crate::error::ParseError;
use crate::tables::{FontTableProvider, SfntVersion};

use std::borrow::Cow;
use std::io::Read;

/// The magic number identifying a WOFF file: 'wOFF'
pub const MAGIC: u32 = 0x774F4646;

#[derive(Clone)]
pub struct WoffFont<'a> {
    pub scope: ReadScope<'a>,
    pub woff_header: WoffHeader,
    pub table_directory: ReadArray<'a, TableDirectoryEntry>,
}

#[derive(Clone, Debug)]
pub struct WoffHeader {
    pub flavor: u32,
    pub length: u32,
    pub num_tables: u16,
    pub total_sfnt_size: u32,
    pub _major_version: u16,
    pub _minor_version: u16,
    pub meta_offset: u32,
    pub meta_length: u32,
    pub meta_orig_length: u32,
    pub priv_offset: u32,
    pub priv_length: u32,
}

#[derive(Debug, Clone)]
pub struct TableDirectoryEntry {
    pub tag: u32,
    pub offset: u32,
    pub comp_length: u32,
    pub orig_length: u32,
    pub orig_checksum: u32,
}

impl WoffFont<'_> {
    /// The "sfnt version" of the input font
    pub fn flavor(&self) -> u32 {
        self.woff_header.flavor
    }

    /// Decompress and return the extended metadata XML if present
    pub fn extended_metadata(&self) -> Result<Option<String>, ParseError> {
        let offset = usize::try_from(self.woff_header.meta_offset)?;
        let length = usize::try_from(self.woff_header.meta_length)?;
        if offset == 0 || length == 0 {
            return Ok(None);
        }

        let compressed_metadata = self.scope.offset_length(offset, length)?;
        let mut z = ZlibDecoder::new(compressed_metadata.data());
        let mut metadata = String::new();
        z.read_to_string(&mut metadata)
            .map_err(|_err| ParseError::CompressionError)?;

        Ok(Some(metadata))
    }

    /// Find the table directory entry for the given `tag`
    pub fn find_table_directory_entry(&self, tag: u32) -> Option<TableDirectoryEntry> {
        self.table_directory
            .iter()
            .find(|table_entry| table_entry.tag == tag)
    }
}

impl ReadBinary for WoffFont<'_> {
    type HostType<'a> = WoffFont<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let scope = ctxt.scope();
        let woff_header = ctxt.read::<WoffHeader>()?;
        let table_directory =
            ctxt.read_array::<TableDirectoryEntry>(usize::from(woff_header.num_tables))?;
        Ok(WoffFont {
            scope,
            woff_header,
            table_directory,
        })
    }
}

impl FontTableProvider for WoffFont<'_> {
    fn table_data(&self, tag: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError> {
        self.find_table_directory_entry(tag)
            .map(|table_entry| {
                table_entry
                    .read_table(&self.scope)
                    .map(|table| table.into_data())
            })
            .transpose()
    }

    fn has_table(&self, tag: u32) -> bool {
        self.find_table_directory_entry(tag).is_some()
    }

    fn table_tags(&self) -> Option<Vec<u32>> {
        Some(self.table_directory.iter().map(|entry| entry.tag).collect())
    }
}

impl SfntVersion for WoffFont<'_> {
    fn sfnt_version(&self) -> u32 {
        self.flavor()
    }
}

impl ReadBinary for WoffHeader {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let signature = ctxt.read_u32be()?;
        match signature {
            MAGIC => {
                let flavor = ctxt.read_u32be()?;
                let length = ctxt.read_u32be()?;
                let num_tables = ctxt.read_u16be()?;
                let reserved = ctxt.read_u16be()?;
                // The header includes a reserved field; this MUST be set to zero. If this field is
                // non-zero, a conforming user agent MUST reject the file as invalid.
                ctxt.check(reserved == 0)?;
                let total_sfnt_size = ctxt.read_u32be()?;
                // The WOFF majorVersion and minorVersion fields specify the version number for a
                // given WOFF file, which can be based on the version number of the input font but
                // is not required to be. These fields have no effect on font loading or usage
                // behavior in user agents.
                let _major_version = ctxt.read_u16be()?;
                let _minor_version = ctxt.read_u16be()?;
                let meta_offset = ctxt.read_u32be()?;
                let meta_length = ctxt.read_u32be()?;
                let meta_orig_length = ctxt.read_u32be()?;
                let priv_offset = ctxt.read_u32be()?;
                let priv_length = ctxt.read_u32be()?;

                Ok(WoffHeader {
                    flavor,
                    length,
                    num_tables,
                    total_sfnt_size,
                    _major_version,
                    _minor_version,
                    meta_offset,
                    meta_length,
                    meta_orig_length,
                    priv_offset,
                    priv_length,
                })
            }
            _ => Err(ParseError::BadVersion),
        }
    }
}

impl ReadFrom for TableDirectoryEntry {
    type ReadType = ((U32Be, U32Be, U32Be), (U32Be, U32Be));
    fn read_from(
        ((tag, offset, comp_length), (orig_length, orig_checksum)): ((u32, u32, u32), (u32, u32)),
    ) -> Self {
        TableDirectoryEntry {
            tag,
            offset,
            comp_length,
            orig_length,
            orig_checksum,
        }
    }
}

impl TableDirectoryEntry {
    fn is_compressed(&self) -> bool {
        self.comp_length != self.orig_length
    }

    /// Read and uncompress the contents of a table entry
    pub fn read_table<'a>(&self, scope: &ReadScope<'a>) -> Result<ReadBuf<'a>, ParseError> {
        let offset = usize::try_from(self.offset)?;
        let length = usize::try_from(self.comp_length)?;
        let table_data = scope.offset_length(offset, length)?;

        if self.is_compressed() {
            let mut z = ZlibDecoder::new(table_data.data());
            let mut uncompressed = Vec::new();
            z.read_to_end(&mut uncompressed)
                .map_err(|_err| ParseError::CompressionError)?;

            Ok(ReadBuf::from(uncompressed))
        } else {
            Ok(ReadBuf::from(table_data.data()))
        }
    }
}
