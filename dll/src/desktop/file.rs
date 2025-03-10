use alloc::sync::Arc;
use core::fmt;
use std::{
    fs,
    io::{Read, Write},
    sync::Mutex,
};

use azul_css::{AzString, U8Vec};

#[repr(C)]
pub struct File {
    pub ptr: Box<Arc<Mutex<fs::File>>>,
    pub path: AzString,
    pub run_destructor: bool,
}

impl Clone for File {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            path: self.path.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.path.as_str())
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.path.as_str())
    }
}

impl PartialEq for File {
    fn eq(&self, other: &Self) -> bool {
        self.path.as_str().eq(other.path.as_str())
    }
}

impl Eq for File {}

impl PartialOrd for File {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.path.as_str().partial_cmp(other.path.as_str())
    }
}

azul_css::impl_option!(File, OptionFile, copy = false, [Clone, Debug]);

impl File {
    fn new(f: fs::File, path: AzString) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(f))),
            path,
            run_destructor: true,
        }
    }
    pub fn open(path: &str) -> Option<Self> {
        Some(Self::new(
            fs::File::open(path).ok()?,
            path.to_string().into(),
        ))
    }
    pub fn create(path: &str) -> Option<Self> {
        Some(Self::new(
            fs::File::create(path).ok()?,
            path.to_string().into(),
        ))
    }
    pub fn read_to_string(&mut self) -> Option<AzString> {
        let file_string = std::fs::read_to_string(self.path.as_str()).ok()?;
        Some(file_string.into())
    }
    pub fn read_to_bytes(&mut self) -> Option<U8Vec> {
        let file_bytes = std::fs::read(self.path.as_str()).ok()?;
        Some(file_bytes.into())
    }
    pub fn write_string(&mut self, string: &str) -> Option<()> {
        self.write_bytes(string.as_bytes())
    }
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Option<()> {
        let mut lock = self.ptr.lock().ok()?;
        lock.write_all(bytes).ok()?;
        lock.sync_all().ok()?;
        Some(())
    }
    pub fn close(self) {}
}
