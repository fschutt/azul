//! The life of the app, as the app hears of it: sent to the background, brought back, short of memory, about to end.
//!
//! The data model only. The queue and the handler live in `azul-layout` (`managers::lifecycle`), the shells raise the
//! events (`azul-dll`, `desktop/lifecycle`: Android's `onPause` / `onTrimMemory`, iOS's `applicationDidEnterBackground`
//! and `applicationDidReceiveMemoryWarning`), and the app learns of them in the handler it installs with
//! `CallbackInfo::set_lifecycle_handler`, which reads the event with `CallbackInfo::get_lifecycle_event`.
//!
//! A phone does not ask an app whether it may stop: after `Paused` the process may be ended without another word
//! (Android kills a background process whenever it needs the memory). What a person would lose - the document, the
//! drawing, the half-typed message - is written in the handler of `Paused`, not in `WillTerminate`.

/// An event of the app's life, as `CallbackInfo::get_lifecycle_event` reports it inside the lifecycle handler.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LifecycleEvent {
    /// The app left the foreground: another app is in front, the screen turned off, the phone's home key was pressed
    /// (Android `onPause`, iOS `applicationDidEnterBackground`). Save what a person would lose: the process may be ended
    /// without another event.
    Paused,
    /// The app is in the foreground again (Android `onResume`, iOS `applicationWillEnterForeground`).
    Resumed,
    /// The system is short of memory and asks the app to give some back (Android `onTrimMemory` / `onLowMemory`, iOS
    /// `applicationDidReceiveMemoryWarning`): drop what can be made again - caches, decoded pictures, tiles.
    LowMemory,
    /// The process is about to end (iOS `applicationWillTerminate`, a desktop's quit, Android's `onDestroy` of the last
    /// activity). The last chance to write, and not a promise: a phone often ends a process with no such event.
    WillTerminate,
}

impl_option!(
    LifecycleEvent,
    OptionLifecycleEvent,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
