//! Rust: the FFI `Option*` / `Result*` types convert to std ones in both
//! directions, and a callback body uses `?` through `Update::with` on stable
//! (and directly with the nightly `try-trait` feature).
//!
//! Built and run by scripts/native_optionals_smoke.sh, which compiles the
//! generated link-dynamic binding into this one file the way azul-dll does.
#![cfg_attr(feature = "try-trait", feature(try_trait_v2))]
#![allow(dead_code)]

extern crate alloc;

mod __ffi_external {
    include!(concat!(env!("AZ_CODEGEN_DIR"), "/dll_api_external.rs"));
}
pub use __ffi_external::__dll_api_inner::dll;
pub mod ffi {
    pub use crate::__ffi_external::__dll_api_inner::*;
}
include!(concat!(env!("AZ_CODEGEN_DIR"), "/reexports.rs"));

use crate::{
    callbacks::{CallbackInfo, RefAny, Update},
    dialog::{FileDialog, FileOpenResult},
    error::UrlParseError,
    option::OptionString,
    str::String as AzString,
    url::Url,
    window::IconVariants,
};

/// The resume callback of the guide's file-dialog example, written with `?`.
extern "C" fn on_pick(_data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    Update::with(|| {
        let picked = FileOpenResult::downcast(result).into_option()?;
        let path = picked.path.into_option()?;
        println!("picked {}", path.inner.as_str());
        Some(Update::RefreshDom)
    })
}

/// The click handler: `None` where the API asks for an `OptionString`.
fn open_picker(data: RefAny) {
    let _ = FileDialog::open_directory("Pick a folder", None, data, on_pick);
}

#[cfg(feature = "try-trait")]
extern "C" fn on_pick_nightly(_data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let picked = FileOpenResult::downcast(result)?;
    let path = picked.path?;
    let _url = Url::parse(path.inner.clone())?;
    Update::RefreshDom
}

fn main() {
    // A field: None in, None out; Some in, Some out.
    let mut icons = IconVariants::default();
    assert_eq!(Option::<AzString>::from(icons.light.clone()), None);
    icons.light = Some(AzString::from("light.png")).into();
    icons.dark = AzString::from("dark.png").into();
    let light: Option<AzString> = icons.light.clone().into();
    assert_eq!(light.as_deref(), Some("light.png"));
    assert_eq!(icons.dark.clone().into_option().as_deref(), Some("dark.png"));
    icons.light = None.into();
    assert!(icons.light.is_none());

    // A result: Ok in, Ok out; Err in, Err out.
    let ok: Result<Url, UrlParseError> = Url::parse("https://azul.rs/guide").into();
    assert!(ok.is_ok());
    let err: Result<Url, UrlParseError> = Url::parse("no scheme").into();
    assert!(err.is_err());

    // `?` inside a callback body on stable.
    let missing = Update::with(|| {
        let s: AzString = OptionString::None.into_option()?;
        let _ = s;
        Some(Update::RefreshDom)
    });
    assert_eq!(missing, Update::DoNothing);
    let present = Update::with(|| {
        let s: AzString = OptionString::Some("x".into()).into_option()?;
        assert_eq!(s.as_str(), "x");
        Some(Update::RefreshDom)
    });
    assert_eq!(present, Update::RefreshDom);
    let failed = Update::with_result(|| {
        let _url: Url = Url::parse("no scheme").into_result()?;
        Ok::<_, UrlParseError>(Update::RefreshDom)
    });
    assert_eq!(failed, Update::DoNothing);

    // The nightly `?` straight on the FFI types.
    #[cfg(feature = "try-trait")]
    {
        fn first_light(icons: IconVariants) -> Option<AzString> {
            let light = icons.light?;
            Some(light)
        }
        assert_eq!(first_light(IconVariants::default()), None);
        fn parsed() -> Result<Url, UrlParseError> {
            let url = Url::parse("https://azul.rs")?;
            Ok(url)
        }
        assert!(parsed().is_ok());
        let _ = on_pick_nightly;
    }

    let _ = open_picker;
    println!("rust native optionals: ok");
}
