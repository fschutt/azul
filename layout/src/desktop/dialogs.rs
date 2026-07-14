//! Native OS dialog wrappers (message boxes, file open/save, color picker).
//!
//! Desktop targets back this with the `tfd` (tiny-file-dialogs) crate; on
//! Android / iOS every method is a no-op that returns the "cancelled / safe
//! default" answer (there is no equivalent of `tfd` on those platforms from
//! a pure-Rust crate, and `tfd 0.1.0` does not cross-compile for them
//! anyway). The public type surface is identical on every target so
//! consumer code keeps compiling.

use azul_css::{
    corety::OptionString,
    impl_option, impl_option_inner,
    props::basic::color::{ColorU, OptionColorU},
    AzString, OptionStringVec, StringVec,
};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tfd::{DefaultColorValue, MessageBoxIcon};

/// Static-method namespace for `tfd`-backed message-box dialogs.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder field
pub struct MsgBox {
    pub _reserved: u8,
}

/// Static-method namespace for `tfd`-backed file dialogs.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder field
pub struct FileDialog {
    pub _reserved: u8,
}

/// Static-method namespace for the `tfd`-backed color picker.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder field
pub struct ColorPickerDialog {
    pub _reserved: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum OkCancel {
    Ok,
    Cancel,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<tfd::OkCancel> for OkCancel {
    #[inline]
    fn from(e: tfd::OkCancel) -> Self {
        match e {
            tfd::OkCancel::Ok => Self::Ok,
            tfd::OkCancel::Cancel => Self::Cancel,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<OkCancel> for tfd::OkCancel {
    #[inline]
    fn from(e: OkCancel) -> Self {
        match e {
            OkCancel::Ok => Self::Ok,
            OkCancel::Cancel => Self::Cancel,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum YesNo {
    Yes,
    No,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<YesNo> for tfd::YesNo {
    #[inline]
    fn from(e: YesNo) -> Self {
        match e {
            YesNo::Yes => Self::Yes,
            YesNo::No => Self::No,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<tfd::YesNo> for YesNo {
    #[inline]
    fn from(e: tfd::YesNo) -> Self {
        match e {
            tfd::YesNo::Yes => Self::Yes,
            tfd::YesNo::No => Self::No,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(C)]
pub enum MsgBoxIcon {
    Info,
    Warning,
    Error,
    Question,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<MsgBoxIcon> for MessageBoxIcon {
    #[inline]
    fn from(e: MsgBoxIcon) -> Self {
        match e {
            MsgBoxIcon::Info => Self::Info,
            MsgBoxIcon::Warning => Self::Warning,
            MsgBoxIcon::Error => Self::Error,
            MsgBoxIcon::Question => Self::Question,
        }
    }
}

impl Default for MsgBox {
    fn default() -> Self {
        Self::new()
    }
}

impl MsgBox {
    /// Returns a zero-initialised namespace handle. The struct itself carries
    /// no state — instances exist only so the FFI layer can hang static
    /// methods off the type.
    #[must_use] pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// "Ok" message box — title, message, icon. Quotes are stripped from the
    /// message to work around `tfd` misinterpreting them as shell metacharacters
    /// on some platforms.
    // owned C-ABI dialog types (AzString/MsgBoxIcon) are passed by value per the azul FFI
    // / api.json convention; taking them by reference would break the exported signature.
    #[allow(clippy::needless_pass_by_value)]
    pub fn ok(title: AzString, message: AzString, icon: MsgBoxIcon) {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut msg = message.as_str().to_string();
            msg = msg.replace('\"', "");
            msg = msg.replace('\'', "");
            tfd::MessageBox::new(title.as_str(), &msg)
                .with_icon(icon.into())
                .run_modal();
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, message, icon);
        }
    }

    /// "Ok / Cancel" message box — title, message, icon, default button.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use] pub fn ok_cancel(
        title: AzString,
        message: AzString,
        icon: MsgBoxIcon,
        default: OkCancel,
    ) -> OkCancel {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            tfd::MessageBox::new(title.as_str(), message.as_str())
                .with_icon(icon.into())
                .run_modal_ok_cancel(default.into())
                .into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, message, icon);
            default
        }
    }

    /// "Yes / No" message box — title, message, icon, default button.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use] pub fn yes_no(
        title: AzString,
        message: AzString,
        icon: MsgBoxIcon,
        default: YesNo,
    ) -> YesNo {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            tfd::MessageBox::new(title.as_str(), message.as_str())
                .with_icon(icon.into())
                .run_modal_yes_no(default.into())
                .into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, message, icon);
            default
        }
    }

    /// Convenience: "Ok" message box with the title "Info" and an info icon.
    pub fn info(content: AzString) {
        Self::ok(AzString::from("Info"), content, MsgBoxIcon::Info);
    }
}

impl Default for ColorPickerDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorPickerDialog {
    /// Returns a zero-initialised namespace handle. Static-only — the struct
    /// is just a hook for the FFI layer.
    #[must_use] pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Opens the default color picker dialog. Returns `None` if cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use] pub fn open(title: AzString, default_value: OptionColorU) -> OptionColorU {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let rgb = default_value
                .into_option()
                .map_or([0, 0, 0], |c| [c.r, c.g, c.b]);
            let default_color = DefaultColorValue::RGB(rgb);
            let result = tfd::ColorChooser::new(title.as_str())
                .with_default_color(default_color)
                .run_modal();
            match result {
                Some(r) => OptionColorU::Some(ColorU {
                    r: r.1[0],
                    g: r.1[1],
                    b: r.1[2],
                    a: ColorU::ALPHA_OPAQUE,
                }),
                None => OptionColorU::None,
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = title;
            default_value
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct FileTypeList {
    pub document_types: StringVec,
    pub document_descriptor: AzString,
}

impl_option!(
    FileTypeList,
    OptionFileTypeList,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd]
);

/// Apply a [`FileTypeList`] filter to a `tfd::FileDialog`.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
// consumes the FileTypeList forwarded from the by-value FFI file-dialog API.
#[allow(clippy::needless_pass_by_value)]
fn apply_filter(mut dialog: tfd::FileDialog, filter: FileTypeList) -> tfd::FileDialog {
    let v = filter.document_types.clone().into_library_owned_vec();
    let patterns: Vec<&str> = v.iter().map(AzString::as_str).collect();
    dialog = dialog.with_filter(&patterns, filter.document_descriptor.as_str());
    dialog
}

impl Default for FileDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl FileDialog {
    /// Returns a zero-initialised namespace handle. Static-only — the struct
    /// is just a hook for the FFI layer.
    #[must_use] pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Open a single file. Returns `None` if the user cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_file(
        title: AzString,
        default_path: OptionString,
        filter_list: OptionFileTypeList,
    ) -> OptionString {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            if let Some(filter) = filter_list.into_option() {
                dialog = apply_filter(dialog, filter);
            }
            dialog.open_file().map(AzString::from).into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path, filter_list);
            OptionString::None
        }
    }

    /// Open a directory. Returns `None` if the user cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_directory(title: AzString, default_path: OptionString) -> OptionString {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            dialog.select_folder().map(AzString::from).into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path);
            OptionString::None
        }
    }

    /// Open multiple files. Returns `None` if the user cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_multiple_files(
        title: AzString,
        default_path: OptionString,
        filter_list: OptionFileTypeList,
    ) -> OptionStringVec {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog =
                tfd::FileDialog::new(title.as_str()).with_multiple_selection(true);
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            if let Some(filter) = filter_list.into_option() {
                dialog = apply_filter(dialog, filter);
            }
            dialog.open_files().map(StringVec::from).into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path, filter_list);
            OptionStringVec::None
        }
    }

    /// Save file dialog. Returns `None` if the user cancelled.
    // owned C-ABI dialog types passed by value per the azul FFI / api.json convention.
    #[allow(clippy::needless_pass_by_value)]
    pub fn save_file(title: AzString, default_path: OptionString) -> OptionString {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let mut dialog = tfd::FileDialog::new(title.as_str());
            if let Some(path) = default_path.as_option() {
                dialog = dialog.with_path(path.as_str());
            }
            dialog.save_file().map(AzString::from).into()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (title, default_path);
            OptionString::None
        }
    }
}

/// Convenience shim: show a default "Info" message box.
pub fn msg_box(content: &str) {
    MsgBox::info(AzString::from(content));
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // Every dialog entry point in this file (`MsgBox::ok`, `FileDialog::open_file`,
    // `ColorPickerDialog::open`, `msg_box`, ...) ends in a `run_modal()` /
    // `open_file()` call that blocks on a native modal window. Calling one from a
    // test would hang the test binary forever (or shell out to zenity/kdialog on
    // a headless box), so they are NEVER invoked here. Instead they are covered by
    // a signature guard (below) that type-checks the FFI surface without running
    // it, and by the android/iOS no-op contract tests, which exercise the branch
    // that genuinely returns without showing a dialog.
    //
    // What IS exercised for real: the three const namespace constructors, the
    // `tfd` enum conversions, and `apply_filter` — a pure builder that never
    // opens anything.

    fn s(value: &str) -> AzString {
        AzString::from(value.to_string())
    }

    fn file_type_list(patterns: &[&str], descriptor: &str) -> FileTypeList {
        FileTypeList {
            document_types: StringVec::from_vec(patterns.iter().map(|p| s(p)).collect()),
            document_descriptor: s(descriptor),
        }
    }

    // ---------------------------------------------------------------------
    // Constructors: MsgBox::new / FileDialog::new / ColorPickerDialog::new
    // ---------------------------------------------------------------------

    #[test]
    fn namespace_handles_are_zero_initialised() {
        assert_eq!(MsgBox::new()._reserved, 0);
        assert_eq!(FileDialog::new()._reserved, 0);
        assert_eq!(ColorPickerDialog::new()._reserved, 0);
    }

    #[test]
    fn namespace_handles_are_const_evaluable() {
        // `new()` is `const fn`; if it ever stops being usable in a const context
        // the FFI/api.json static-namespace contract breaks. This fails to compile
        // rather than fails at runtime, which is the point.
        const MSG_BOX: MsgBox = MsgBox::new();
        const FILE_DIALOG: FileDialog = FileDialog::new();
        const COLOR_PICKER: ColorPickerDialog = ColorPickerDialog::new();

        assert_eq!(MSG_BOX._reserved, 0);
        assert_eq!(FILE_DIALOG._reserved, 0);
        assert_eq!(COLOR_PICKER._reserved, 0);
    }

    #[test]
    fn namespace_handles_default_matches_new() {
        assert_eq!(MsgBox::default(), MsgBox::new());
        assert_eq!(FileDialog::default(), FileDialog::new());
        assert_eq!(ColorPickerDialog::default(), ColorPickerDialog::new());
    }

    #[test]
    fn namespace_handles_are_stateless_single_byte_shims() {
        // These types are `#[repr(C)]` placeholders that the FFI layer hangs static
        // methods off. A field creeping in would silently change the C ABI.
        assert_eq!(core::mem::size_of::<MsgBox>(), 1);
        assert_eq!(core::mem::size_of::<FileDialog>(), 1);
        assert_eq!(core::mem::size_of::<ColorPickerDialog>(), 1);
        assert_eq!(core::mem::align_of::<MsgBox>(), 1);
        assert_eq!(core::mem::align_of::<FileDialog>(), 1);
        assert_eq!(core::mem::align_of::<ColorPickerDialog>(), 1);
    }

    #[test]
    fn namespace_handles_are_copy_and_hash_consistently() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        fn hash_of<T: Hash>(value: &T) -> u64 {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        }

        let original = MsgBox::new();
        let copied = original; // Copy, not a move
        assert_eq!(original, copied);
        assert_eq!(hash_of(&original), hash_of(&copied));
        assert_eq!(hash_of(&MsgBox::new()), hash_of(&MsgBox::new()));
        assert_eq!(hash_of(&FileDialog::new()), hash_of(&FileDialog::new()));
        assert_eq!(
            hash_of(&ColorPickerDialog::new()),
            hash_of(&ColorPickerDialog::new())
        );
    }

    // ---------------------------------------------------------------------
    // Enum conversions to/from `tfd` (round-trip: encode == decode)
    // ---------------------------------------------------------------------

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn ok_cancel_round_trips_through_tfd() {
        for variant in [OkCancel::Ok, OkCancel::Cancel] {
            let encoded: tfd::OkCancel = variant.into();
            let decoded: OkCancel = encoded.into();
            assert_eq!(decoded, variant, "round-trip lost {variant:?}");
        }

        // ... and the other direction, exhaustively.
        assert_eq!(OkCancel::from(tfd::OkCancel::Ok), OkCancel::Ok);
        assert_eq!(OkCancel::from(tfd::OkCancel::Cancel), OkCancel::Cancel);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn yes_no_round_trips_through_tfd() {
        for variant in [YesNo::Yes, YesNo::No] {
            let encoded: tfd::YesNo = variant.into();
            let decoded: YesNo = encoded.into();
            assert_eq!(decoded, variant, "round-trip lost {variant:?}");
        }

        assert_eq!(YesNo::from(tfd::YesNo::Yes), YesNo::Yes);
        assert_eq!(YesNo::from(tfd::YesNo::No), YesNo::No);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn answer_enums_must_be_converted_by_variant_never_by_discriminant() {
        // azul declares `OkCancel { Ok, Cancel }` (Ok = 0) but tfd declares
        // `OkCancel { Cancel = 0, Ok = 1 }` — the discriminants are INVERTED.
        // Same story for YesNo. So a `transmute` or an `as`-cast in place of the
        // `From` impls would silently turn "Ok" into "Cancel", i.e. hand the caller
        // the exact opposite of what the user clicked. This test pins the mismatch
        // so nobody "optimises" the match arms into a cast.
        assert_eq!(OkCancel::Ok as u8, 0);
        assert_eq!(OkCancel::Cancel as u8, 1);
        assert_eq!(tfd::OkCancel::Ok as u8, 1);
        assert_eq!(tfd::OkCancel::Cancel as u8, 0);

        assert_eq!(YesNo::Yes as u8, 0);
        assert_eq!(YesNo::No as u8, 1);
        assert_eq!(tfd::YesNo::Yes as u8, 1);
        assert_eq!(tfd::YesNo::No as u8, 0);

        // The conversions must follow the variant, not the number.
        assert_eq!(tfd::OkCancel::from(OkCancel::Ok), tfd::OkCancel::Ok);
        assert_eq!(tfd::YesNo::from(YesNo::Yes), tfd::YesNo::Yes);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn msg_box_icon_maps_to_the_matching_tfd_icon() {
        let mapping = [
            (MsgBoxIcon::Info, MessageBoxIcon::Info),
            (MsgBoxIcon::Warning, MessageBoxIcon::Warning),
            (MsgBoxIcon::Error, MessageBoxIcon::Error),
            (MsgBoxIcon::Question, MessageBoxIcon::Question),
        ];
        for (ours, theirs) in mapping {
            assert_eq!(MessageBoxIcon::from(ours), theirs, "wrong icon for {ours:?}");
        }

        // Injective: four distinct inputs must not collapse onto three icons.
        let encoded: Vec<MessageBoxIcon> = mapping
            .iter()
            .map(|(ours, _)| MessageBoxIcon::from(*ours))
            .collect();
        for (i, a) in encoded.iter().enumerate() {
            for b in encoded.iter().skip(i + 1) {
                assert_ne!(a, b, "two MsgBoxIcon variants map to the same tfd icon");
            }
        }
    }

    // ---------------------------------------------------------------------
    // apply_filter — the only non-modal logic in this file
    // ---------------------------------------------------------------------

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_with_no_patterns_does_not_panic() {
        let dialog = apply_filter(tfd::FileDialog::new("title"), file_type_list(&[], ""));
        assert!(dialog.filter_patterns().is_empty());
        assert_eq!(dialog.filter_description(), "");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_with_a_default_constructed_string_vec_does_not_panic() {
        // `StringVec::new()` is the empty/possibly-null-pointer case that
        // `into_library_owned_vec` has to survive.
        let filter = FileTypeList {
            document_types: StringVec::new(),
            document_descriptor: s("no types"),
        };
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);
        assert!(dialog.filter_patterns().is_empty());
        assert_eq!(dialog.filter_description(), "no types");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_preserves_patterns_verbatim_and_in_order() {
        let filter = file_type_list(&["*.png", "*.jpg", "*.png", ""], "Images");
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        // Duplicates and the empty pattern survive: the filter is a pass-through,
        // not a set.
        assert_eq!(dialog.filter_patterns(), &["*.png", "*.jpg", "*.png", ""]);
        assert_eq!(dialog.filter_description(), "Images");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_preserves_unicode_patterns() {
        let patterns = [
            "*.图片",          // CJK
            "*.🎨",            // astral-plane emoji
            "*.مِلَف",           // RTL with combining marks
            "*.e\u{0301}xt",   // decomposed é — must not be normalised away
            "*.\u{200B}zwsp",  // zero-width space
        ];
        let filter = file_type_list(&patterns, "Ünïcödé — файлы 🎨");
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        assert_eq!(dialog.filter_patterns(), &patterns);
        assert_eq!(dialog.filter_description(), "Ünïcödé — файлы 🎨");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_does_not_truncate_at_interior_nul_bytes() {
        // A NUL is a legal Rust `str` byte but terminates a C string. `apply_filter`
        // is pure Rust, so it must hand the bytes on intact rather than silently
        // cutting the pattern short (a truncation here would turn "*.png\0evil" into
        // a filter the caller never asked for).
        let filter = file_type_list(&["*.pn\0g", "\0", "a\u{1}b\u{7f}"], "desc\0ription");
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        assert_eq!(dialog.filter_patterns(), &["*.pn\0g", "\0", "a\u{1}b\u{7f}"]);
        assert_eq!(dialog.filter_description(), "desc\0ription");
        assert_eq!(dialog.filter_patterns()[0].len(), 6); // bytes kept, not cut at the NUL
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_passes_shell_metacharacters_through_unchanged() {
        // Documents the ACTUAL behaviour: unlike `MsgBox::ok` (which strips quotes
        // before handing the string to tfd), `apply_filter` sanitises nothing. If a
        // sanitisation step is ever added, this test should be updated deliberately
        // — it must not change by accident.
        let hostile = ["\"", "'", "$(id)", "`id`", "a;b", "x\ny", "--", "*"];
        let filter = file_type_list(&hostile, "\"quoted\" $(id)");
        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        assert_eq!(dialog.filter_patterns(), &hostile);
        assert_eq!(dialog.filter_description(), "\"quoted\" $(id)");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_survives_a_huge_filter_list() {
        let patterns: Vec<String> = (0..2000).map(|i| format!("*.ext{i}")).collect();
        let descriptor = "d".repeat(64 * 1024);
        let filter = FileTypeList {
            document_types: StringVec::from_vec(
                patterns.iter().map(|p| s(p)).collect::<Vec<AzString>>(),
            ),
            document_descriptor: s(&descriptor),
        };

        let dialog = apply_filter(tfd::FileDialog::new("title"), filter);

        assert_eq!(dialog.filter_patterns().len(), 2000);
        assert_eq!(dialog.filter_patterns()[0], "*.ext0");
        assert_eq!(dialog.filter_patterns()[1999], "*.ext1999");
        assert_eq!(dialog.filter_description().len(), 64 * 1024);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_overwrites_rather_than_appends() {
        // tfd's `with_filter` assigns, so applying twice is last-write-wins. Worth
        // pinning: an `open_file` caller that expects the two lists to merge would
        // silently lose the first set of extensions.
        let dialog = tfd::FileDialog::new("title");
        let dialog = apply_filter(dialog, file_type_list(&["*.png"], "Images"));
        let dialog = apply_filter(dialog, file_type_list(&["*.txt"], "Text"));

        assert_eq!(dialog.filter_patterns(), &["*.txt"]);
        assert_eq!(dialog.filter_description(), "Text");
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn apply_filter_leaves_the_rest_of_the_dialog_alone() {
        // `open_multiple_files` sets the path + multi-select BEFORE calling
        // apply_filter; the filter must not clobber either.
        let dialog = tfd::FileDialog::new("title")
            .with_path("/tmp/somewhere")
            .with_multiple_selection(true);
        let dialog = apply_filter(dialog, file_type_list(&["*.png"], "Images"));

        assert_eq!(dialog.path(), "/tmp/somewhere");
        assert!(dialog.multiple_selection());
        assert_eq!(dialog.filter_patterns(), &["*.png"]);
    }

    // ---------------------------------------------------------------------
    // FileTypeList / OptionFileTypeList container invariants
    // ---------------------------------------------------------------------

    #[test]
    fn string_vec_round_trips_through_into_library_owned_vec() {
        // This is the exact conversion `apply_filter` performs internally.
        let original: Vec<AzString> = vec![s("*.png"), s(""), s("*.🎨"), s("a\0b")];
        let round_tripped = StringVec::from_vec(original.clone()).into_library_owned_vec();
        assert_eq!(round_tripped, original);

        // ... and the empty case, which takes the null/zero-length branch.
        let empty = StringVec::from_vec(Vec::<AzString>::new()).into_library_owned_vec();
        assert!(empty.is_empty());
    }

    #[test]
    fn file_type_list_clone_is_equal_and_orders_reflexively() {
        use std::cmp::Ordering;

        let filter = file_type_list(&["*.png", "*.jpg"], "Images");
        let cloned = filter.clone();

        assert_eq!(cloned, filter);
        assert_eq!(filter.partial_cmp(&filter), Some(Ordering::Equal));
        assert_eq!(cloned.document_types.len(), 2);
        assert_eq!(cloned.document_descriptor.as_str(), "Images");
    }

    #[test]
    fn file_type_list_ordering_follows_the_descriptor_when_types_match() {
        use std::cmp::Ordering;

        let a = file_type_list(&["*.png"], "aaa");
        let b = file_type_list(&["*.png"], "bbb");
        assert_eq!(a.partial_cmp(&b), Some(Ordering::Less));
        assert_eq!(b.partial_cmp(&a), Some(Ordering::Greater));
        assert_ne!(a, b);
    }

    #[test]
    fn option_file_type_list_round_trips() {
        let filter = file_type_list(&["*.png"], "Images");

        let some = OptionFileTypeList::Some(filter.clone());
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(some.as_option(), Some(&filter));
        assert_eq!(some.clone().into_option(), Some(filter));

        let none = OptionFileTypeList::None;
        assert!(none.is_none());
        assert_eq!(none.as_option(), None);
        assert_eq!(OptionFileTypeList::default(), OptionFileTypeList::None);
    }

    // ---------------------------------------------------------------------
    // Modal entry points: signature guard only — calling these would block
    // ---------------------------------------------------------------------

    #[test]
    fn modal_entry_points_keep_their_ffi_signatures() {
        // Coercing to a fn pointer type-checks every exported signature WITHOUT
        // invoking it. api.json / the C bindings are generated from these exact
        // shapes, so an argument reorder or a changed return type must not slip
        // through unnoticed just because no test can safely call them.
        let _ok: fn(AzString, AzString, MsgBoxIcon) = MsgBox::ok;
        let _ok_cancel: fn(AzString, AzString, MsgBoxIcon, OkCancel) -> OkCancel = MsgBox::ok_cancel;
        let _yes_no: fn(AzString, AzString, MsgBoxIcon, YesNo) -> YesNo = MsgBox::yes_no;
        let _info: fn(AzString) = MsgBox::info;
        let _color: fn(AzString, OptionColorU) -> OptionColorU = ColorPickerDialog::open;
        let _open_file: fn(AzString, OptionString, OptionFileTypeList) -> OptionString =
            FileDialog::open_file;
        let _open_dir: fn(AzString, OptionString) -> OptionString = FileDialog::open_directory;
        let _open_many: fn(AzString, OptionString, OptionFileTypeList) -> OptionStringVec =
            FileDialog::open_multiple_files;
        let _save_file: fn(AzString, OptionString) -> OptionString = FileDialog::save_file;
        let _msg_box: fn(&str) = msg_box;
    }

    // ---------------------------------------------------------------------
    // android / iOS: the no-op branch is the one that CAN be executed safely
    // ---------------------------------------------------------------------

    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_message_boxes_are_silent_no_ops() {
        MsgBox::ok(s("title"), s("message"), MsgBoxIcon::Error);
        MsgBox::info(s(""));
        msg_box("");
        msg_box("\0\u{1}🎨");
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_answer_dialogs_echo_the_default_back() {
        for default in [OkCancel::Ok, OkCancel::Cancel] {
            let answer = MsgBox::ok_cancel(s("t"), s("m"), MsgBoxIcon::Question, default);
            assert_eq!(answer, default);
        }
        for default in [YesNo::Yes, YesNo::No] {
            let answer = MsgBox::yes_no(s("t"), s("m"), MsgBoxIcon::Question, default);
            assert_eq!(answer, default);
        }
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_color_picker_echoes_the_default_back() {
        let default = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        };
        let picked = ColorPickerDialog::open(s("t"), OptionColorU::Some(default));
        match picked.as_option() {
            Some(c) => {
                assert_eq!((c.r, c.g, c.b), (1, 2, 3));
                // NB: the mobile stub keeps the caller's alpha, while the desktop
                // path forces ColorU::ALPHA_OPAQUE. Pinned deliberately.
                assert_eq!(c.a, 4);
            }
            None => panic!("mobile stub must return the default it was given"),
        }
        assert!(ColorPickerDialog::open(s("t"), OptionColorU::None).is_none());
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    #[test]
    fn mobile_file_dialogs_report_cancellation() {
        assert!(FileDialog::open_file(s("t"), OptionString::None, OptionFileTypeList::None).is_none());
        assert!(FileDialog::open_directory(s("t"), OptionString::None).is_none());
        assert!(FileDialog::save_file(s("t"), OptionString::Some(s("/tmp"))).is_none());
        assert!(
            FileDialog::open_multiple_files(s("t"), OptionString::None, OptionFileTypeList::None)
                .is_none()
        );
    }
}
