//! `Url::open`, routed through the platform's opener where `azul_core` has none.
//!
//! `azul_core::url::Url::open` spawns `open` / `xdg-open` / `rundll32`; a phone has no such process. api.json's
//! `Url.open` calls this, so the generated C ABI (and the `azul::url::Url` facade over it) reaches the Android opener
//! (`desktop::extra::url_opener`: an activity is started through `AzulUrl.open`, `azul-settings:app` is the app's own
//! page in the system settings). Everywhere else this is `Url::open` as it was.

/// Opens `url` in the system's default handler; `true` once the opener started.
#[must_use]
pub fn open(url: &azul_core::url::Url) -> bool {
    // The Android opener lives in the desktop module (`az_desktop`: the builds that have a shell); an engine-only build has none.
    #[cfg(all(target_os = "android", az_desktop))]
    {
        crate::desktop::extra::url_opener::open(url.href.as_str())
    }
    #[cfg(not(all(target_os = "android", az_desktop)))]
    {
        url.open()
    }
}
