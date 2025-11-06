use std::time::Duration;
use x11_clipboard::Clipboard as SystemClipboard;
use {clipboard_metadata::ClipboardContentType, errors::ClipboardError, Clipboard};

pub struct X11Clipboard {
    inner: SystemClipboard,
}

impl Clipboard for X11Clipboard {
    type Output = Self;

    fn new() -> Result<Self::Output, ClipboardError> {
        Ok(X11Clipboard {
            inner: SystemClipboard::new()?,
        })
    }

    /// # **WARNING**: Unimplemented, use `get_string_contents`
    fn get_contents(&self) -> Result<(Vec<u8>, ClipboardContentType), ClipboardError> {
        Err(ClipboardError::Unimplemented)
    }

    fn get_string_contents(&self) -> Result<String, ClipboardError> {
        Ok(String::from_utf8(self.inner.load(
            self.inner.getter.atoms.clipboard,
            self.inner.getter.atoms.utf8_string,
            self.inner.getter.atoms.property,
            Duration::from_secs(3),
        )?)?)
    }

    /// # **WARNING**: Unimplemented, use `set_string_contents`
    fn set_contents(
        &self,
        _contents: Vec<u8>,
        _: ClipboardContentType,
    ) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unimplemented)
    }

    fn set_string_contents(&self, contents: String) -> Result<(), ClipboardError> {
        Ok(self.inner.store(
            self.inner.setter.atoms.clipboard,
            self.inner.setter.atoms.utf8_string,
            contents,
        )?)
    }
}
