use std::fs;
use azul_css::{U8Vec, AzString};
use std::io::{Read, Write};

#[repr(C)]
pub struct File {
    ptr: Box<fs::File>,
}

impl_option!(File, OptionFile, copy = false, clone = false,[]);

impl File {
    fn new(f: fs::File) -> Self { Self { ptr: Box::new(f) } }
    pub fn open(path: &str) -> Option<Self> {
        Some(Self::new(fs::File::open(path).ok()?))
    }
    pub fn create(path: &str) -> Option<Self> {
        Some(Self::new(fs::File::create(path).ok()?))
    }
    pub fn read_to_string(&mut self) -> Option<AzString> {
        let mut contents = String::new();
        self.ptr.read_to_string(&mut contents).ok()?;
        Some(contents.into())
    }
    pub fn read_to_bytes(&mut self) -> Option<U8Vec> {
        let mut contents = Vec::new();
        self.ptr.read(&mut contents).ok()?;
        Some(contents.into())
    }
    pub fn write_string(&mut self, string: &str) -> Option<()> {
        self.write_bytes(string.as_bytes())
    }
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Option<()> {
        self.ptr.write_all(bytes).ok()?;
        self.ptr.sync_all().ok()?;
        Some(())
    }
    pub fn close(self) { }
}