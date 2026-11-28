//! The share sheet of the platform: the backend `CallbackInfo::share` calls.
//!
//! The data model is `azul_core::share`, the request check, the limits and the slot are
//! `azul_layout::managers::share` (and the staging of the copies `managers::share_area`). This module is the OS plumbing and
//! the one function that installs it into the App's services at the start:
//!
//! | | Android | iOS | macOS / Linux / Windows |
//! |---|---|---|---|
//! | the sheet | `Intent.ACTION_SEND` / `ACTION_SEND_MULTIPLE` through `Intent.createChooser`; "open with" is `ACTION_VIEW` through a chooser | `UIActivityViewController`; "open with" is `UIDocumentInteractionController`'s open-in menu | none: `Unsupported(reason)` |
//! | the files | copies served by the app's `AzulShareProvider` (`content://<package>.azul.share/<token>/<name>`), granted read-only to the chosen app | copies in the app's temporary folder, handed over as file URLs | - |
//! | called on | the thread the callbacks run on (the loop thread: `startActivity` may be called from any thread) | the main thread (UIKit) | - |
//!
//! A desktop has no system share sheet this engine shows: the app is told so (`Unsupported`, with the reason, and
//! `CallbackInfo::can_share` is `false`), never a silent nothing. The macOS `NSSharingServicePicker` and the Windows share
//! UI are the natural next backends (ledger WP-F).

use azul_core::share::ShareStatus;
use azul_layout::managers::{
    app_services::AppServices,
    share::{ShareBackend, SharePlan},
};

#[cfg(all(target_os = "android", feature = "jni"))]
mod android;
#[cfg(target_os = "ios")]
mod apple;

/// Install this platform's share backend into `services` (the App's), once, at the start (`App::run`).
pub fn install(services: &AppServices) {
    services.share.install(Some(ShareBackend {
        share: platform_share,
        unavailable: platform_unavailable,
    }));
}

/// Why this platform has no share sheet, or `None` when it has one.
fn platform_unavailable() -> Option<String> {
    #[cfg(all(target_os = "android", feature = "jni"))]
    {
        None
    }
    #[cfg(all(target_os = "android", not(feature = "jni")))]
    {
        Some("this Android build has no JNI bridge (the `jni` feature), so it cannot start the chooser".to_string())
    }
    #[cfg(target_os = "ios")]
    {
        None
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        Some(
            "this desktop has no system share sheet: reveal the file in the file manager, or copy the link, instead"
                .to_string(),
        )
    }
}

/// Show the platform's sheet for a checked plan.
#[allow(unused_variables)] // a desktop target has no use for the plan
fn platform_share(plan: &SharePlan) -> ShareStatus {
    #[cfg(all(target_os = "android", feature = "jni"))]
    {
        android::share(plan)
    }
    #[cfg(target_os = "ios")]
    {
        apple::share(plan)
    }
    #[cfg(not(any(all(target_os = "android", feature = "jni"), target_os = "ios")))]
    {
        // `unavailable` answers first, so this is only reached by a caller that skipped it
        ShareStatus::Unsupported(
            platform_unavailable()
                .unwrap_or_else(|| "no share sheet on this platform".to_string())
                .into(),
        )
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
