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

/// The model of the guide's file-dialog example.
struct DataModel {
    folder: alloc::string::String,
}

// The guide's file-dialog example, as written there (hello-world/rust.md).
extern "C" fn on_choose_folder(data: RefAny, _: CallbackInfo) -> Update {
    FileDialog::open_directory("Choose a folder", None, data.clone(), on_pick_folder);
    Update::DoNothing
}

extern "C" fn on_pick_folder(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    Update::with(|| {
        let mut model = data.downcast_mut::<DataModel>()?;
        let path = FileOpenResult::downcast(result).into_option()?.path.into_option()?;
        model.folder = path.inner.as_str().to_string();
        Some(Update::RefreshDom)
    })
}

#[cfg(feature = "try-trait")]
mod nightly {
    use super::*;

    extern "C" fn on_pick_folder(mut data: RefAny, _: CallbackInfo, result: RefAny) -> Update {
        let mut model = data.downcast_mut::<DataModel>()?;
        let path = FileOpenResult::downcast(result)?.path?;
        model.folder = path.inner.as_str().to_string();
        Update::RefreshDom
    }

    /// An error inside a callback: logged, and the callback does nothing.
    extern "C" fn on_parse(_data: RefAny, _: CallbackInfo) -> Update {
        let _url = Url::parse("no scheme")?;
        Update::RefreshDom
    }

    pub fn reachable() {
        let _ = (on_pick_folder as extern "C" fn(_, _, _) -> _, on_parse as extern "C" fn(_, _) -> _);
    }
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
        nightly::reachable();
    }

    let _ = (on_choose_folder as extern "C" fn(_, _) -> _, on_pick_folder as extern "C" fn(_, _, _) -> _);
    println!("rust native optionals: ok");
}
