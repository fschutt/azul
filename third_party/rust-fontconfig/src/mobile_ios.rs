//! iOS-only font discovery via CoreText.
//!
//! On iOS, system fonts (`Helvetica.ttc`, `SFNS.ttc`, `PingFang.ttc`, …) live
//! under `/System/Library/Fonts/{,Core,AssetsV2}/` and the per-app CoreText
//! cache. The app sandbox denies `read_dir(2)` on those paths even when the
//! files are world-readable, so a plain directory scan returns nothing — but the
//! individual files *are* openable once you know the path.
//!
//! We get those paths from CoreText. Earlier versions used
//! `CTFontManagerCopyAvailableFontURLs`, but that symbol is **macOS-only** — it
//! is not exported by the iOS CoreText framework, so an iOS build failed to link
//! ("Undefined symbols: _CTFontManagerCopyAvailableFontURLs"). Instead we build
//! the available-fonts collection and read each descriptor's `kCTFontURLAttribute`:
//!
//!   CTFontCollectionCreateFromAvailableFonts(NULL)        // iOS 7+
//!     -> CTFontCollectionCreateMatchingFontDescriptors()  // iOS 7+
//!       -> CTFontDescriptorCopyAttribute(d, kCTFontURLAttribute)  // iOS 3.2+
//!         -> CFURLGetFileSystemRepresentation()           // iOS 2.0+
//!
//! All of these are available from iOS 7 (the URL attribute since 3.2), so this
//! links and runs on every iOS version azul targets. The resulting `PathBuf`s
//! feed the same `FcParseFont` path the desktop arms use. Many descriptors share
//! one `.ttc`, so the paths are de-duplicated before returning.

use alloc::vec::Vec;
use core::ffi::c_void;
use std::collections::BTreeSet;
use std::os::raw::{c_long, c_uchar};
use std::path::PathBuf;

#[repr(C)]
pub(crate) struct __CFArray(c_void);
#[repr(C)]
pub(crate) struct __CFURL(c_void);
#[repr(C)]
pub(crate) struct __CFString(c_void);
#[repr(C)]
pub(crate) struct __CFDictionary(c_void);
#[repr(C)]
pub(crate) struct __CTFontCollection(c_void);
#[repr(C)]
pub(crate) struct __CTFontDescriptor(c_void);

type CFArrayRef = *const __CFArray;
type CFURLRef = *const __CFURL;
type CFStringRef = *const __CFString;
type CFDictionaryRef = *const __CFDictionary;
type CTFontCollectionRef = *const __CTFontCollection;
type CTFontDescriptorRef = *const __CTFontDescriptor;
type CFIndex = c_long;

#[link(name = "CoreText", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    // kCTFontURLAttribute: the file URL of a font descriptor (iOS 3.2+).
    static kCTFontURLAttribute: CFStringRef;

    // Available-fonts collection + its descriptors (iOS 7+).
    fn CTFontCollectionCreateFromAvailableFonts(options: CFDictionaryRef) -> CTFontCollectionRef;
    fn CTFontCollectionCreateMatchingFontDescriptors(
        collection: CTFontCollectionRef,
    ) -> CFArrayRef;
    // CTFontDescriptorCopyAttribute returns a +1 (owned) CFType (iOS 3.2+).
    fn CTFontDescriptorCopyAttribute(
        descriptor: CTFontDescriptorRef,
        attribute: CFStringRef,
    ) -> *const c_void;

    fn CFArrayGetCount(theArray: CFArrayRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(theArray: CFArrayRef, idx: CFIndex) -> *const c_void;
    fn CFURLGetFileSystemRepresentation(
        url: CFURLRef,
        resolve_against_base: bool,
        buffer: *mut c_uchar,
        max_buf_len: CFIndex,
    ) -> bool;
    fn CFRelease(cf: *const c_void);
}

/// Enumerate every available font's on-disk path via CoreText. Returns an empty
/// vec if CoreText reports no fonts (it never should on a real device). Uses only
/// iOS-7-and-newer symbols so it links on every supported iOS version.
pub(crate) fn copy_available_font_urls() -> Vec<PathBuf> {
    // Deduplicate: a `.ttc` exposes many descriptors (one per face) that all map
    // to the same file URL; we only want to parse each file once.
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();

    unsafe {
        let collection = CTFontCollectionCreateFromAvailableFonts(core::ptr::null());
        if collection.is_null() {
            return Vec::new();
        }
        let descriptors = CTFontCollectionCreateMatchingFontDescriptors(collection);
        CFRelease(collection as *const c_void);
        if descriptors.is_null() {
            return Vec::new();
        }

        let count = CFArrayGetCount(descriptors);
        // PATH_MAX on Darwin is 1024; allow extra room for symlinked cache trees.
        let mut buf = [0u8; 4096];

        for i in 0..count {
            let desc = CFArrayGetValueAtIndex(descriptors, i) as CTFontDescriptorRef;
            if desc.is_null() {
                continue;
            }
            // Owned (+1) CFURLRef — must CFRelease.
            let url = CTFontDescriptorCopyAttribute(desc, kCTFontURLAttribute) as CFURLRef;
            if url.is_null() {
                continue; // in-memory / data fonts have no file URL
            }
            let ok = CFURLGetFileSystemRepresentation(
                url,
                true,
                buf.as_mut_ptr(),
                buf.len() as CFIndex,
            );
            if ok {
                let nul_idx = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                if nul_idx != 0 {
                    if let Ok(s) = core::str::from_utf8(&buf[..nul_idx]) {
                        seen.insert(PathBuf::from(s));
                    }
                }
            }
            CFRelease(url as *const c_void);
        }

        CFRelease(descriptors as *const c_void);
    }

    seen.into_iter().collect()
}
