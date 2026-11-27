//! `AVAudioSession` on iOS: the one place the engine says what it wants of the app's audio
//! session.
//!
//! Every iOS app has a DEFAULT audio session. Apple's overview: "It supports audio playback, but
//! disallows audio recording" - so an `AVAudioEngine` whose input node is tapped without a
//! record-capable category reports an input of zero Hz and the speech recognizer answers "there is
//! no microphone to listen with", every time. And "in iOS, setting the Ring/Silent switch to silent
//! mode silences any audio the app is playing; locking a device silences the app's audio" for the
//! default (ambient) category: an `AudioSink` that must be heard needs the playback category.
//!
//! Each user of audio calls [`activate`] with what it is about to do, before it builds its engine;
//! the category is the weakest one that allows it (record asks for playback too: the app that
//! listens usually speaks). macOS has no audio session: [`activate`] is a no-op there.
//! Interruptions (a call, Siri) and route changes are not handled here: the engines restart on
//! their own configuration-change notification, which the sinks and microphones own.

/// What the caller is about to do with audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionUse {
    /// Output that must be heard (a sink): not silenced by the Ring/Silent switch or the screen
    /// lock.
    Playback,
    /// Input (a recognizer's microphone), with output still possible.
    Record,
}

/// The `AVAudioSessionCategory...` constant's value for a use.
#[must_use]
pub const fn category_for(use_: SessionUse) -> &'static str {
    match use_ {
        SessionUse::Playback => "AVAudioSessionCategoryPlayback",
        SessionUse::Record => "AVAudioSessionCategoryPlayAndRecord",
    }
}

/// The `AVAudioSessionMode...` constant's value for a use: a recognizer wants the raw signal
/// (`Measurement` turns the system's automatic gain and processing off), playback the default.
#[must_use]
pub const fn mode_for(use_: SessionUse) -> &'static str {
    match use_ {
        SessionUse::Playback => "AVAudioSessionModeDefault",
        SessionUse::Record => "AVAudioSessionModeMeasurement",
    }
}

/// `AVAudioSessionCategoryOptions` for a use: recording through the built-in speaker as well as
/// a headset (`DefaultToSpeaker`) and Bluetooth hands-free (`AllowBluetooth`); playback has none.
#[must_use]
pub const fn options_for(use_: SessionUse) -> usize {
    match use_ {
        SessionUse::Playback => 0,
        SessionUse::Record => 0x4 | 0x8,
    }
}

/// An autoreleased `NSString` for a constant's value.
#[cfg(target_os = "ios")]
unsafe fn ns_string(text: &str) -> *mut objc::runtime::Object {
    use objc::{class, msg_send, sel, sel_impl};
    let c = std::ffi::CString::new(text).unwrap_or_default();
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

/// Set the category for `use_` and activate the session. The reason, when it fails.
#[cfg(target_os = "ios")]
pub fn activate(use_: SessionUse) -> Result<(), String> {
    use objc::{
        msg_send,
        runtime::{Class, Object},
        sel, sel_impl,
    };

    // SAFETY: plain Objective-C calls on the shared session; the error out-parameters are
    // pointers to locals that outlive the calls.
    unsafe {
        let Some(class) = Class::get("AVAudioSession") else {
            return Err("AVAudioSession is not loaded".to_string());
        };
        let session: *mut Object = msg_send![class, sharedInstance];
        if session.is_null() {
            return Err("the shared audio session is nil".to_string());
        }
        let category = ns_string(category_for(use_));
        let mode = ns_string(mode_for(use_));
        let options: usize = options_for(use_);
        let mut error: *mut Object = core::ptr::null_mut();
        // `NSError **`, as the `*mut c_void` objc 0.2 can encode.
        let error_out = &mut error as *mut *mut Object as *mut core::ffi::c_void;
        let set: bool = msg_send![
            session,
            setCategory: category
                   mode: mode
                options: options
                  error: error_out
        ];
        if !set {
            return Err(describe(error, "setCategory"));
        }
        let mut error: *mut Object = core::ptr::null_mut();
        let error_out = &mut error as *mut *mut Object as *mut core::ffi::c_void;
        let active: bool = msg_send![session, setActive: true error: error_out];
        if !active {
            return Err(describe(error, "setActive"));
        }
    }
    Ok(())
}

/// `NSError.localizedDescription` as text, or just the call's name.
#[cfg(target_os = "ios")]
unsafe fn describe(error: *mut objc::runtime::Object, call: &str) -> String {
    use objc::{msg_send, runtime::Object, sel, sel_impl};
    if error.is_null() {
        return format!("AVAudioSession {call} failed");
    }
    let text: *mut Object = msg_send![error, localizedDescription];
    let utf8: *const core::ffi::c_char = if text.is_null() {
        core::ptr::null()
    } else {
        msg_send![text, UTF8String]
    };
    if utf8.is_null() {
        return format!("AVAudioSession {call} failed");
    }
    format!(
        "AVAudioSession {call}: {}",
        core::ffi::CStr::from_ptr(utf8).to_string_lossy()
    )
}

/// macOS (and every other host) has no audio session to configure.
#[cfg(not(target_os = "ios"))]
pub fn activate(_use: SessionUse) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
#[path = "audio_session_test.rs"]
mod audio_session_test;
