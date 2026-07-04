use crate::binary::read::{ReadBinary, ReadCtxt};
use crate::error::ParseError;
use crate::woff2::{PackedU16, TableDirectoryEntry, Woff2Font};

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Directory {
    #[allow(unused)]
    version: u32,
    entries: Vec<FontEntry>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct FontEntry {
    #[allow(unused)]
    flavor: u32,
    table_indices: Vec<u16>,
}

impl ReadBinary for FontEntry {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let num_tables = ctxt.read::<PackedU16>()?;
        let flavor = ctxt.read_u32be()?;
        let table_indices = (0..num_tables)
            .map(|_| ctxt.read::<PackedU16>())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(FontEntry {
            flavor,
            table_indices,
        })
    }
}

impl ReadBinary for Directory {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let ttc_version = ctxt.read_u32be()?;
        let num_fonts = ctxt.read::<PackedU16>()?;
        let entries = (0..num_fonts)
            .map(|_| ctxt.read::<FontEntry>())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Directory {
            version: ttc_version,
            entries,
        })
    }
}

impl Directory {
    pub fn fonts(&self) -> impl Iterator<Item = &FontEntry> + '_ {
        self.entries.iter()
    }

    pub fn get(&self, index: usize) -> Option<&FontEntry> {
        self.entries.get(index)
    }
}

impl FontEntry {
    pub fn table_entries<'a>(
        &'a self,
        file: &'a Woff2Font<'_>,
    ) -> impl Iterator<Item = &'a TableDirectoryEntry> + 'a {
        self.table_indices
            .iter()
            .flat_map(move |&index| file.table_directory.get(usize::from(index)))
    }
}
