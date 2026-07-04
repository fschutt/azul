//! Top-level font file representation.

use std::borrow::Cow;

use crate::binary::read::{ReadBinary, ReadCtxt};
use crate::error::{ParseError, ReadWriteError};
use crate::tables::{
    FontTableProvider, OpenTypeFont, SfntVersion, CFF_MAGIC, TRUE_MAGIC, TTCF_MAGIC, TTF_MAGIC,
};
use crate::woff::{self, WoffFont};
use crate::woff2::{self, Woff2Font};

/// Type that can represent any of the supported font formats.
#[derive(Clone)]
pub enum FontData<'a> {
    OpenType(OpenTypeFont<'a>),
    Woff(WoffFont<'a>),
    Woff2(Woff2Font<'a>),
}

/// Generic implementation of the `FontTableProvider` trait
pub struct DynamicFontTableProvider<'a> {
    sfnt_version: u32,
    provider: Box<dyn FontTableProvider + Send + Sync + 'a>,
}

impl ReadBinary for FontData<'_> {
    type HostType<'a> = FontData<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let mut peek = ctxt.clone();
        let magic = peek.read_u32be()?;
        match magic {
            TTF_MAGIC | TRUE_MAGIC | CFF_MAGIC => Ok(FontData::OpenType(OpenTypeFont::read(ctxt)?)),
            TTCF_MAGIC => Ok(FontData::OpenType(OpenTypeFont::read(ctxt)?)),
            woff::MAGIC => Ok(FontData::Woff(WoffFont::read(ctxt)?)),
            woff2::MAGIC => Ok(FontData::Woff2(Woff2Font::read(ctxt)?)),
            _ => Err(ParseError::BadVersion),
        }
    }
}

impl FontTableProvider for DynamicFontTableProvider<'_> {
    fn table_data(&self, tag: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError> {
        self.provider.table_data(tag)
    }

    fn has_table(&self, tag: u32) -> bool {
        self.provider.has_table(tag)
    }

    fn table_tags(&self) -> Option<Vec<u32>> {
        self.provider.table_tags()
    }
}

impl SfntVersion for DynamicFontTableProvider<'_> {
    fn sfnt_version(&self) -> u32 {
        self.sfnt_version
    }
}

impl<'a> FontData<'a> {
    /// Obtain an implementation of `FontTableProvider` for this font.
    pub fn table_provider(
        &self,
        index: usize,
    ) -> Result<DynamicFontTableProvider<'a>, ReadWriteError> {
        match self {
            FontData::OpenType(font) => {
                let provider = font.table_provider(index)?;
                Ok(DynamicFontTableProvider {
                    sfnt_version: provider.sfnt_version(),
                    provider: Box::new(provider),
                })
            }
            FontData::Woff(font) => {
                // This clone is relatively cheap as WoffFile is mostly holding borrowed data
                Ok(DynamicFontTableProvider {
                    sfnt_version: font.sfnt_version(),
                    provider: Box::new(font.clone()),
                })
            }
            FontData::Woff2(font) => {
                let provider = font.table_provider(index)?;
                Ok(DynamicFontTableProvider {
                    sfnt_version: provider.sfnt_version(),
                    provider: Box::new(provider),
                })
            }
        }
    }
}
