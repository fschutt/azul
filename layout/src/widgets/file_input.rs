//! File input button, same as `Button`, but triggers a
//! user-supplied path-change callback when clicked

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::Dom,
    refany::RefAny,
    resources::OptionImageRef,
};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::CssPropertyWithConditionsVec,
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::button::{Button, ButtonOnClick, ButtonOnClickCallback},
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FileInput {
    /// State of the file input
    pub file_input_state: FileInputStateWrapper,
    /// Default text to display when no file has been selected
    /// (default = "Select File...")
    pub default_text: AzString,

    /// Optional image that is displayed next to the label
    pub image: OptionImageRef,
    /// Style for this button container
    pub container_style: CssPropertyWithConditionsVec,
    /// Style of the label
    pub label_style: CssPropertyWithConditionsVec,
    /// Style of the image
    pub image_style: CssPropertyWithConditionsVec,
}

impl Default for FileInput {
    fn default() -> Self {
        let default_button = Button::create(AzString::from_const_str(""));
        Self {
            file_input_state: FileInputStateWrapper::default(),
            default_text: "Select File...".into(),
            image: None.into(),
            container_style: default_button.container_style,
            label_style: default_button.label_style,
            image_style: default_button.image_style,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FileInputStateWrapper {
    pub inner: FileInputState,
    pub on_path_change: OptionFileInputOnPathChange,
    /// Title displayed in the file selection dialog
    pub file_dialog_title: AzString,
    /// Default directory of file input
    pub default_dir: OptionString,
}

impl Default for FileInputStateWrapper {
    fn default() -> Self {
        Self {
            inner: FileInputState::default(),
            on_path_change: None.into(),
            file_dialog_title: "Select File".into(),
            default_dir: None.into(),
        }
    }
}

/// Current state of the file input (selected path)
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FileInputState {
    pub path: OptionString,
}

impl Default for FileInputState {
    fn default() -> Self {
        Self { path: None.into() }
    }
}

/// Callback type invoked when the file input path changes
pub type FileInputOnPathChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, FileInputState) -> Update;

impl_widget_callback!(
    FileInputOnPathChange,
    OptionFileInputOnPathChange,
    FileInputOnPathChangeCallback,
    FileInputOnPathChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        FileInputOnPathChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: FILE_INPUT_ON_PATH_CHANGE_INVOKER,
    invoker_ty:     AzFileInputOnPathChangeCallbackInvoker,
    thunk_fn:       az_file_input_on_path_change_callback_thunk,
    setter_fn:      AzApp_setFileInputOnPathChangeCallbackInvoker,
    from_handle_fn: AzFileInputOnPathChangeCallback_createFromHostHandle,
    extra_args:     [ state: FileInputState ],
}

impl FileInput {
    #[must_use]
    pub fn create(path: OptionString) -> Self {
        Self {
            file_input_state: FileInputStateWrapper {
                inner: FileInputState { path },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(None.into());
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_default_text(&mut self, default_text: AzString) {
        self.default_text = default_text;
    }

    #[inline]
    #[must_use]
    pub fn with_default_text(mut self, default_text: AzString) -> Self {
        self.set_default_text(default_text);
        self
    }

    #[inline]
    pub fn set_on_path_change<I: Into<FileInputOnPathChangeCallback>>(
        &mut self,
        refany: RefAny,
        callback: I,
    ) {
        self.file_input_state.on_path_change = Some(FileInputOnPathChange {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    #[inline]
    #[must_use]
    pub fn with_on_path_change<I: Into<FileInputOnPathChangeCallback>>(
        mut self,
        refany: RefAny,
        callback: I,
    ) -> Self {
        self.set_on_path_change(refany, callback);
        self
    }

    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        // either show the default text or the file name
        // including the extension as the button label
        let button_label = match self.file_input_state.inner.path.as_ref() {
            Some(path) => std::path::Path::new(path.as_str())
                .file_name()
                .map_or_else(
                    || self.default_text.as_str().to_string(),
                    |s| s.to_string_lossy().to_string(),
                )
                .into(),
            None => self.default_text.clone(),
        };

        Button {
            label: button_label,
            image: self.image,
            icon: AzString::from_const_str(""),
            trailing_icon: AzString::from_const_str(""),
            button_type: crate::widgets::button::ButtonType::Default,
            container_style: self.container_style,
            label_style: self.label_style,
            image_style: self.image_style,
            icon_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
            trailing_icon_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
            on_click: Some(ButtonOnClick {
                refany: RefAny::new(self.file_input_state),
                callback: ButtonOnClickCallback {
                    cb: fileinput_on_click,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
            })
            .into(),
        }
        .dom()
    }
}

extern "C" fn fileinput_on_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut fileinputstatewrapper) = refany.downcast_mut::<FileInputStateWrapper>() else {
        return Update::DoNothing;
    };
    let fileinputstatewrapper = &mut *fileinputstatewrapper;

    // `tfd` is desktop-only (target-gated in Cargo.toml to not(android|ios)); the
    // `extra` feature does nothing on mobile, so gate the dialog block by the same
    // target cfg to avoid referencing the unlinked `tfd` crate on iOS/Android.
    #[cfg(all(feature = "extra", not(any(target_os = "android", target_os = "ios"))))]
    {
        let mut dialog = tfd::FileDialog::new(fileinputstatewrapper.file_dialog_title.as_str());
        if let Some(dir) = fileinputstatewrapper.default_dir.as_ref() {
            dialog = dialog.with_path(dir.as_str());
        }
        let Some(selected_path) = dialog.open_file() else {
            return Update::DoNothing;
        };
        fileinputstatewrapper.inner.path = Some(selected_path.into()).into();
    }

    let inner = fileinputstatewrapper.inner.clone();
    let mut result = match fileinputstatewrapper.on_path_change.as_mut() {
        Some(FileInputOnPathChange { refany, callback }) => {
            (callback.cb)(refany.clone(), info, inner)
        }
        None => Update::RefreshDom,
    };

    result.max_self(Update::RefreshDom);

    result
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::too_many_lines)] // table-driven cases; splitting them hides the case list
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{
            DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass, NodeId, NodeType, TabIndex,
        },
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::{ImageRef, RawImageFormat, RendererResources},
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::button::ButtonType,
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// The label a path-less file input renders, per the doc comment on
    /// `FileInput::default_text`.
    const DEFAULT_TEXT: &str = "Select File...";

    /// The title `FileInputStateWrapper::default` puts on the native dialog. Note it is
    /// *not* the same string as `DEFAULT_TEXT` (no ellipsis) — a "cleanup" that unified
    /// the two would silently retitle every file dialog.
    const DEFAULT_DIALOG_TITLE: &str = "Select File";

    /// Paths whose *last* component is a real file name, paired with the label
    /// `dom()` must render for them. Only shapes that mean the same thing on every
    /// platform live here (`/` is a separator on Windows too); the backslash- and
    /// double-slash-specific shapes are tested per-platform below.
    fn file_name_cases() -> Vec<(String, String)> {
        [
            ("/tmp/report.pdf", "report.pdf"),
            ("report.pdf", "report.pdf"),
            ("/tmp/dir/", "dir"),   // a trailing separator is not a component
            ("/tmp/dir///", "dir"), // ...and neither are three of them
            ("a/.", "a"),           // a trailing `.` normalizes away
            ("/a/b/c/d/e/f/g.txt", "g.txt"),
            (".hidden", ".hidden"), // a leading dot is part of the name
            ("...", "..."),         // only `.` and `..` are special
            ("..a", "..a"),
            ("/tmp/archive.tar.gz", "archive.tar.gz"),
            ("  ", "  "), // whitespace is a legal file name
            ("/tmp/a b.txt", "a b.txt"),
            ("/tmp/a\nb.txt", "a\nb.txt"), // control chars survive verbatim
            ("/tmp/a\tb.txt", "a\tb.txt"),
            ("a\0b", "a\0b"), // OsStr allows interior NULs; must not truncate
            ("/tmp/a\0b.txt", "a\0b.txt"),
            ("/tmp/日本語.txt", "日本語.txt"),
            ("/tmp/e\u{0301}.txt", "e\u{0301}.txt"), // decomposed é: no normalization
            (
                "/tmp/\u{1F469}\u{200D}\u{1F467}.png", // ZWJ emoji sequence
                "\u{1F469}\u{200D}\u{1F467}.png",
            ),
            ("/tmp/\u{202E}gpj.exe", "\u{202E}gpj.exe"), // RTL-override spoof, kept as-is
            ("/tmp/\u{FFFD}.bin", "\u{FFFD}.bin"),
            ("/Select File.../x", "x"), // a directory named like the default text
        ]
        .iter()
        .map(|(p, l)| ((*p).to_string(), (*l).to_string()))
        .collect()
    }

    /// Paths with *no* file-name component: `Path::file_name` returns `None` for each,
    /// so `dom()` must fall back to `default_text` rather than render an empty button.
    fn no_file_name_cases() -> Vec<String> {
        [
            "", "/", ".", "..", "./", "/.", "/..", "a/..", "a/b/../", "../..",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
    }

    /// Adversarial strings for the free-form text fields (`default_text`,
    /// `file_dialog_title`, `default_dir`): empty, whitespace-only, combining marks,
    /// ZWJ emoji, RTL, embedded NULs (`AzString` is length-based, so a NUL must not
    /// truncate), bidi overrides — plus one string far longer than any real label.
    fn adversarial_strings() -> Vec<String> {
        let mut v: Vec<String> = [
            "",
            " ",
            "Pick a file",
            "e\u{0301}",
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",
            "\0",
            "a\0b",
            "\u{FFFD}\u{202E}\u{200B}",
            "…\t\r\n",
            DEFAULT_TEXT,
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        v.push("x".repeat(100_000));
        v
    }

    fn opt(s: &str) -> OptionString {
        Some(AzString::from(s)).into()
    }

    /// A file input with every field set to something distinguishable, so that a
    /// mutator touching a field it should not is visible.
    ///
    /// Deliberately image-*less*: `ImageRef` equality is identity-based (a fresh `id`
    /// per constructor call), so two independently-built images never compare equal and
    /// an image here would break every `assert_eq!` between two fixtures.
    fn populated() -> FileInput {
        let mut fi = FileInput::create(opt("/tmp/original.txt"));
        fi.default_text = "custom default".into();
        fi.file_input_state.file_dialog_title = "custom title".into();
        fi.file_input_state.default_dir = opt("/tmp/custom-dir");
        fi
    }

    /// [`populated`] plus an image. Only for tests that compare a widget against its own
    /// `clone()` (clones share the image identity) or do not compare widgets at all.
    fn populated_with_image() -> FileInput {
        let mut fi = populated();
        fi.image = Some(ImageRef::null_image(
            3,
            5,
            RawImageFormat::RGBA8,
            b"file-input-probe".to_vec(),
        ))
        .into();
        fi
    }

    // ------------------------------------------------------------------
    // DOM probes
    // ------------------------------------------------------------------

    fn text_of(dom: &Dom) -> Option<&str> {
        match dom.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    /// The rendered button label. `Button::dom` always appends the label *last*
    /// (an optional image is pushed as the first child), block-formatted as
    /// `<p>` wrapping the text node.
    fn rendered_label(dom: &Dom) -> String {
        let children = dom.children.as_ref();
        let last = children.last().expect("the button has no label child");
        assert!(
            matches!(last.root.get_node_type(), NodeType::P),
            "the button label is not block-formatted (<p>)"
        );
        let inner = last.children.as_ref();
        assert_eq!(inner.len(), 1, "the label <p> wraps exactly one text node");
        text_of(&inner[0])
            .expect("the button label is not a text node")
            .to_string()
    }

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The recursive descendant count — `Dom::estimated_total_children` is a *cached*
    /// value that, if too small, makes `convert_dom_into_compact_dom` under-allocate
    /// its arenas and panic on out-of-bounds writes.
    fn count_descendants(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .map(|c| 1 + count_descendants(c))
            .sum()
    }

    /// The state wrapper the rendered DOM handed to its own click handler.
    fn registered_state(dom: &Dom) -> RefAny {
        let callbacks = dom.root.callbacks.as_ref();
        assert_eq!(
            callbacks.len(),
            1,
            "a file input must register exactly one callback",
        );
        callbacks[0].refany.clone()
    }

    fn state_of(refany: &RefAny) -> FileInputStateWrapper {
        let mut refany = refany.clone();
        let wrapper = refany
            .downcast_ref::<FileInputStateWrapper>()
            .expect("the widget state changed type");
        wrapper.clone()
    }

    // ------------------------------------------------------------------
    // Callback fixtures
    // ------------------------------------------------------------------

    /// A payload the path-change callback writes into. It arrives as the `refany`
    /// argument — a *shared* clone of what the test still holds — so the test reads
    /// back exactly what the widget passed, with no global state.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct PathLog {
        seen: Vec<Option<String>>,
        payload: u32,
    }

    fn log_refany() -> RefAny {
        RefAny::new(PathLog {
            seen: Vec::new(),
            payload: 0xDEAD_BEEF,
        })
    }

    fn read_log(probe: &RefAny) -> PathLog {
        let mut probe = probe.clone();
        let log = probe
            .downcast_ref::<PathLog>()
            .expect("the user payload changed type");
        log.clone()
    }

    extern "C" fn record_path(
        mut refany: RefAny,
        _info: CallbackInfo,
        state: FileInputState,
    ) -> Update {
        if let Some(mut log) = refany.downcast_mut::<PathLog>() {
            log.seen
                .push(state.path.as_ref().map(|p| p.as_str().to_string()));
        }
        Update::DoNothing
    }

    // Only reachable from `without_the_native_dialog`, which is compiled out on a
    // default (`extra`) desktop build.
    #[allow(dead_code)]
    extern "C" fn path_do_nothing(
        _refany: RefAny,
        _info: CallbackInfo,
        _state: FileInputState,
    ) -> Update {
        Update::DoNothing
    }

    extern "C" fn path_refresh_all(
        _refany: RefAny,
        _info: CallbackInfo,
        _state: FileInputState,
    ) -> Update {
        Update::RefreshDomAllWindows
    }

    /// A `Callback`-shaped (2-arg) function — the shape FFI bindings hand in, which the
    /// `From<Callback>` arm *transmutes* into the 3-arg file-input slot. Never called.
    extern "C" fn generic_shaped(_refany: RefAny, _info: CallbackInfo) -> Update {
        Update::DoNothing
    }

    fn cb_addr(cb: &FileInputOnPathChangeCallback) -> usize {
        cb.cb as *const () as usize
    }

    // ------------------------------------------------------------------
    // CallbackInfo harness
    // ------------------------------------------------------------------

    /// A `DomNodeId` in the root DOM pointing at flattened node `idx`.
    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "no concrete node was hit"
    /// case. `fileinput_on_click` never queries the hit node, so it must survive it.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// A `DomLayoutResult` carrying only a `styled_dom`: `fileinput_on_click` never
    /// queries the layout, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Runs `f` with a `CallbackInfo` whose window holds `styled_dom` as the root DOM
    /// and whose hit node is `hit`. Returns `f`'s value plus every change the callback
    /// pushed onto the transaction log.
    fn with_info<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled_dom));

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let mut info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let r = f(&mut info);
        let pushed = info.take_changes();
        (r, pushed)
    }

    /// Renders `file_input`, then hands back both the laid-out DOM *and* the very
    /// `RefAny` the widget registered on its own mouse-up callback. Driving the handler
    /// with these two is the real wiring — nothing is re-created by hand, so a mismatch
    /// between what `dom()` stores and what the handler expects cannot hide behind the
    /// fixture.
    #[allow(dead_code)] // only used by `without_the_native_dialog` (see its doc comment)
    fn laid_out(file_input: FileInput) -> (StyledDom, RefAny) {
        let dom = file_input.dom();
        let state = registered_state(&dom);
        (StyledDom::create_from_dom(dom), state)
    }

    /// One "mouse-up on `hit`" delivered to the widget's own registered handler.
    fn click(
        styled_dom: StyledDom,
        state: &RefAny,
        hit: DomNodeId,
    ) -> (Update, Vec<CallbackChange>) {
        with_info(styled_dom, hit, |info| {
            fileinput_on_click(state.clone(), *info)
        })
    }

    // ==================================================================
    // FileInput::create
    // ==================================================================

    #[test]
    fn create_stores_the_path_verbatim() {
        // Byte-for-byte: a NUL must not truncate (AzString is length-based), a 100k
        // path must not be capped, and no normalization may happen at construction.
        let mut paths: Vec<String> = file_name_cases().into_iter().map(|(p, _)| p).collect();
        paths.extend(no_file_name_cases());
        paths.extend(adversarial_strings());
        paths.push(format!("/tmp/{}", "x".repeat(100_000)));

        for p in paths {
            let fi = FileInput::create(opt(&p));
            let stored = fi
                .file_input_state
                .inner
                .path
                .as_ref()
                .expect("create(Some(..)) dropped the path");
            assert_eq!(stored.as_str(), p, "create({p:?}) altered the path");
            assert_eq!(
                stored.as_str().len(),
                p.len(),
                "create({p:?}) changed the byte length of the path",
            );
        }
    }

    #[test]
    fn create_with_no_path_equals_the_default_widget() {
        let created = FileInput::create(None.into());
        assert!(
            created.file_input_state.inner.path.is_none(),
            "create(None) invented a path",
        );
        assert_eq!(
            created,
            FileInput::default(),
            "create(None) and Default disagree — the two constructors have drifted",
        );
    }

    #[test]
    fn create_uses_the_documented_defaults_for_every_other_field() {
        for p in [None.into(), opt(""), opt("/tmp/x.txt")] {
            let fi = FileInput::create(p);
            assert_eq!(fi.default_text.as_str(), DEFAULT_TEXT);
            assert_eq!(
                fi.file_input_state.file_dialog_title.as_str(),
                DEFAULT_DIALOG_TITLE,
            );
            assert!(fi.file_input_state.default_dir.is_none());
            assert!(
                fi.file_input_state.on_path_change.as_ref().is_none(),
                "create() invented a path-change callback out of nowhere",
            );
            assert!(fi.image.as_ref().is_none());
        }
    }

    #[test]
    fn create_inherits_the_button_styling_verbatim() {
        // The widget is documented as "same as `Button`" — it must not fork the
        // button's style vecs, or a restyle of Button would silently skip it.
        let button = Button::create(AzString::from_const_str(""));
        let fi = FileInput::create(opt("/tmp/x.txt"));
        assert_eq!(fi.container_style, button.container_style);
        assert_eq!(fi.label_style, button.label_style);
        assert_eq!(fi.image_style, button.image_style);
    }

    #[test]
    fn create_is_pure_and_repeatable() {
        // Same argument, same widget — no hidden counter, clock or global state.
        for p in ["", "/tmp/a.txt", "\0"] {
            assert_eq!(FileInput::create(opt(p)), FileInput::create(opt(p)));
        }
    }

    // ==================================================================
    // FileInput::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_original_and_resets_self() {
        let mut fi = populated_with_image().with_on_path_change(
            log_refany(),
            record_path as FileInputOnPathChangeCallbackType,
        );
        let before = fi.clone();

        let taken = fi.swap_with_default();

        assert_eq!(
            taken, before,
            "swap_with_default did not return the original"
        );
        assert_eq!(
            fi,
            FileInput::default(),
            "swap_with_default left the widget in a non-default state",
        );
        assert!(
            fi.file_input_state.on_path_change.as_ref().is_none(),
            "the callback survived the reset — a stale RefAny would keep firing",
        );
        assert!(fi.image.as_ref().is_none(), "the image survived the reset");
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_intact() {
        let probe = log_refany();
        let mut fi = FileInput::create(opt("/tmp/x"))
            .with_on_path_change(probe, record_path as FileInputOnPathChangeCallbackType);

        let taken = fi.swap_with_default();

        let moved = taken
            .file_input_state
            .on_path_change
            .as_ref()
            .expect("the callback was lost in the swap");
        assert_eq!(
            cb_addr(&moved.callback),
            record_path as *const () as usize,
            "the moved-out callback points somewhere else",
        );
        assert_eq!(read_log(&moved.refany).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn swap_with_default_twice_yields_the_default_the_second_time() {
        let mut fi = populated_with_image();
        let first = fi.swap_with_default();
        let second = fi.swap_with_default();

        assert_ne!(
            first, second,
            "the first swap did not actually take anything"
        );
        assert_eq!(second, FileInput::default());
        assert_eq!(
            fi,
            FileInput::default(),
            "the second swap dirtied the widget"
        );
    }

    #[test]
    fn swap_with_default_on_a_default_widget_is_a_no_op() {
        let mut fi = FileInput::default();
        let taken = fi.swap_with_default();
        assert_eq!(taken, FileInput::default());
        assert_eq!(fi, FileInput::default());
    }

    #[test]
    fn swap_with_default_preserves_extreme_field_values() {
        // A 100k default text and a NUL-bearing path must move across the swap
        // untouched — `mem::swap` is byte-wise, so any change means a rebuild.
        let huge = "x".repeat(100_000);
        let mut fi = FileInput::create(opt("a\0b"));
        fi.set_default_text(huge.as_str().into());

        let taken = fi.swap_with_default();

        assert_eq!(taken.default_text.as_str().len(), huge.len());
        assert_eq!(
            taken
                .file_input_state
                .inner
                .path
                .as_ref()
                .map(|p| p.as_str().to_string()),
            Some("a\0b".to_string()),
        );
    }

    // ==================================================================
    // FileInput::set_default_text / with_default_text
    // ==================================================================

    #[test]
    fn set_default_text_stores_the_text_verbatim() {
        for s in adversarial_strings() {
            let mut fi = FileInput::default();
            fi.set_default_text(s.as_str().into());
            assert_eq!(
                fi.default_text.as_str(),
                s,
                "set_default_text({s:?}) altered the text"
            );
            assert_eq!(
                fi.default_text.as_str().len(),
                s.len(),
                "set_default_text({s:?}) changed the byte length (NUL truncation?)",
            );
        }
    }

    #[test]
    fn set_default_text_is_last_write_wins() {
        let mut fi = FileInput::default();
        for s in adversarial_strings() {
            fi.set_default_text(s.as_str().into());
            assert_eq!(fi.default_text.as_str(), s);
        }
        fi.set_default_text("final".into());
        assert_eq!(fi.default_text.as_str(), "final");
    }

    #[test]
    fn set_default_text_touches_nothing_else() {
        let mut fi = populated_with_image();
        let before = fi.clone();
        fi.set_default_text("something else entirely".into());

        assert_eq!(fi.file_input_state.inner, before.file_input_state.inner);
        assert_eq!(
            fi.file_input_state.file_dialog_title,
            before.file_input_state.file_dialog_title,
        );
        assert_eq!(
            fi.file_input_state.default_dir,
            before.file_input_state.default_dir,
        );
        assert_eq!(fi.container_style, before.container_style);
        assert_eq!(fi.label_style, before.label_style);
        assert_eq!(fi.image_style, before.image_style);
        assert_eq!(fi.image, before.image);
    }

    #[test]
    fn with_default_text_matches_the_setter() {
        for s in adversarial_strings() {
            let mut by_setter = populated();
            by_setter.set_default_text(s.as_str().into());
            let by_builder = populated().with_default_text(s.as_str().into());
            assert_eq!(
                by_builder, by_setter,
                "with_default_text({s:?}) and set_default_text disagree",
            );
        }
    }

    #[test]
    fn with_default_text_chains_last_wins() {
        let fi = FileInput::default()
            .with_default_text("first".into())
            .with_default_text("second".into())
            .with_default_text("".into());
        assert_eq!(fi.default_text.as_str(), "");
    }

    // ==================================================================
    // FileInput::set_on_path_change / with_on_path_change
    // ==================================================================

    #[test]
    fn set_on_path_change_stores_the_function_pointer_and_the_data() {
        let probe = log_refany();
        let mut fi = FileInput::default();
        fi.set_on_path_change(
            probe.clone(),
            record_path as FileInputOnPathChangeCallbackType,
        );

        let stored = fi
            .file_input_state
            .on_path_change
            .as_ref()
            .expect("the callback was not stored");
        assert_eq!(cb_addr(&stored.callback), record_path as *const () as usize);
        assert_eq!(
            stored.refany, probe,
            "the widget stored a different RefAny than the one it was handed",
        );
        assert_eq!(read_log(&stored.refany).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn set_on_path_change_overwrites_a_previous_callback() {
        // Two live callbacks would fire twice per click; the setter must replace.
        let mut fi = FileInput::default();
        fi.set_on_path_change(
            log_refany(),
            record_path as FileInputOnPathChangeCallbackType,
        );
        fi.set_on_path_change(
            RefAny::new(7_u32),
            path_refresh_all as FileInputOnPathChangeCallbackType,
        );

        let stored = fi
            .file_input_state
            .on_path_change
            .as_ref()
            .expect("no callback");
        assert_eq!(
            cb_addr(&stored.callback),
            path_refresh_all as *const () as usize,
            "the first callback survived the overwrite",
        );
        let mut data = stored.refany.clone();
        assert!(
            data.downcast_ref::<PathLog>().is_none(),
            "the first callback's data survived the overwrite",
        );
    }

    #[test]
    fn set_on_path_change_touches_nothing_else() {
        let mut fi = populated_with_image();
        let before = fi.clone();
        fi.set_on_path_change(
            log_refany(),
            record_path as FileInputOnPathChangeCallbackType,
        );

        assert_eq!(fi.default_text, before.default_text);
        assert_eq!(fi.file_input_state.inner, before.file_input_state.inner);
        assert_eq!(
            fi.file_input_state.file_dialog_title,
            before.file_input_state.file_dialog_title,
        );
        assert_eq!(
            fi.file_input_state.default_dir,
            before.file_input_state.default_dir,
        );
        assert_eq!(fi.image, before.image);
        assert_eq!(fi.container_style, before.container_style);
    }

    #[test]
    fn with_on_path_change_matches_the_setter() {
        let probe = log_refany();
        let mut by_setter = populated();
        by_setter.set_on_path_change(
            probe.clone(),
            record_path as FileInputOnPathChangeCallbackType,
        );
        let by_builder = populated()
            .with_on_path_change(probe, record_path as FileInputOnPathChangeCallbackType);
        assert_eq!(by_builder, by_setter);
    }

    #[test]
    fn a_generic_callback_keeps_its_address_and_context_through_the_transmute() {
        // FFI bindings hand in a 2-arg `Callback`; the `From<Callback>` arm transmutes
        // it into the 3-arg slot. The transmute must preserve the code address (calling
        // the wrong function) and the `ctx` payload (losing the Python/Lua callable).
        let ctx = RefAny::new(0xABCD_u32);
        let generic = Callback {
            cb: generic_shaped,
            ctx: OptionRefAny::Some(ctx.clone()),
        };
        let fi = FileInput::default().with_on_path_change(log_refany(), generic);

        let stored = fi
            .file_input_state
            .on_path_change
            .as_ref()
            .expect("no callback");
        assert_eq!(
            cb_addr(&stored.callback),
            generic_shaped as *const () as usize,
            "the transmute moved the function pointer",
        );
        assert_eq!(
            stored.callback.ctx,
            OptionRefAny::Some(ctx),
            "the FFI context was dropped by the transmute",
        );
    }

    #[test]
    fn a_raw_function_pointer_gets_no_ffi_context() {
        let fi = FileInput::default().with_on_path_change(
            log_refany(),
            record_path as FileInputOnPathChangeCallbackType,
        );
        let stored = fi
            .file_input_state
            .on_path_change
            .as_ref()
            .expect("no callback");
        assert_eq!(
            stored.callback.ctx,
            OptionRefAny::None,
            "a native Rust callback must not carry an FFI context",
        );
    }

    // ==================================================================
    // FileInput::dom
    // ==================================================================

    #[test]
    fn dom_labels_the_button_with_the_file_name() {
        for (path, expected) in file_name_cases() {
            let dom = FileInput::create(opt(&path)).dom();
            assert_eq!(
                rendered_label(&dom),
                expected,
                "dom() mislabelled the button for path {path:?}",
            );
        }
    }

    #[test]
    fn dom_falls_back_to_the_default_text_when_the_path_has_no_file_name() {
        // `/`, `.`, `..` and friends have no final `Normal` component. Rendering an
        // *empty* button there would leave the user with an unlabelled control.
        for path in no_file_name_cases() {
            let dom = FileInput::create(opt(&path)).dom();
            assert_eq!(
                rendered_label(&dom),
                DEFAULT_TEXT,
                "dom() did not fall back to the default text for path {path:?}",
            );
        }
    }

    #[test]
    fn dom_falls_back_to_the_default_text_when_no_path_is_set() {
        let dom = FileInput::create(None.into()).dom();
        assert_eq!(rendered_label(&dom), DEFAULT_TEXT);
    }

    #[test]
    fn dom_renders_a_custom_default_text_verbatim_when_there_is_no_file_name() {
        for s in adversarial_strings() {
            let fi = FileInput::create(None.into()).with_default_text(s.as_str().into());
            assert_eq!(
                rendered_label(&fi.dom()),
                s,
                "custom default text {s:?} was altered"
            );

            // ...and the same for a path that has no file-name component.
            let fi = FileInput::create(opt("/")).with_default_text(s.as_str().into());
            assert_eq!(rendered_label(&fi.dom()), s);
        }
    }

    #[test]
    fn dom_survives_a_100k_byte_file_name() {
        let huge = "x".repeat(100_000);
        let dom = FileInput::create(opt(&format!("/tmp/{huge}"))).dom();
        let label = rendered_label(&dom);
        assert_eq!(label.len(), huge.len(), "the 100k file name was truncated");
        assert_eq!(label, huge);
    }

    #[test]
    fn dom_prefers_the_file_name_over_the_default_text() {
        // A non-empty default text must not shadow a real selection.
        let fi = FileInput::create(opt("/tmp/chosen.txt")).with_default_text("NOT THIS".into());
        assert_eq!(rendered_label(&fi.dom()), "chosen.txt");
    }

    #[test]
    fn dom_renders_an_empty_label_when_both_the_path_and_the_default_text_are_empty() {
        let fi = FileInput::create(opt("")).with_default_text("".into());
        assert_eq!(rendered_label(&fi.dom()), "");
    }

    #[test]
    fn dom_ignores_the_dialog_title_and_default_dir_when_labelling() {
        // Only `path`/`default_text` may reach the label; leaking the dialog title or
        // the default directory into the button would be visible to the user.
        let mut fi = FileInput::create(None.into());
        fi.file_input_state.file_dialog_title = "TITLE-LEAK".into();
        fi.file_input_state.default_dir = opt("/DIR-LEAK");
        let label = rendered_label(&fi.dom());
        assert_eq!(label, DEFAULT_TEXT);
        assert!(!label.contains("LEAK"));
    }

    #[test]
    fn dom_renders_a_native_button_node() {
        let dom = FileInput::create(opt("/tmp/x.txt")).dom();
        assert!(
            matches!(dom.root.get_node_type(), NodeType::Button),
            "the file input no longer renders a <button>",
        );
        assert_eq!(
            classes(&dom),
            vec![
                "__azul-native-button".to_string(),
                ButtonType::Default.class_name().to_string(),
            ],
            "the file input must be styleable as a default-type native button",
        );
        assert!(
            matches!(dom.root.get_tab_index(), Some(TabIndex::Auto)),
            "the file input dropped the button's keyboard focusability",
        );
    }

    #[test]
    fn dom_registers_exactly_one_mouseup_callback_into_fileinput_on_click() {
        let dom = FileInput::create(opt("/tmp/x.txt")).dom();
        let callbacks = dom.root.callbacks.as_ref();
        assert_eq!(callbacks.len(), 1, "a click must fire exactly one handler");
        assert_eq!(
            callbacks[0].event,
            EventFilter::Hover(HoverEventFilter::Click)
        );
        assert_eq!(
            callbacks[0].callback.cb, fileinput_on_click as *const () as usize,
            "the DOM is wired to a different handler than fileinput_on_click",
        );
        assert_eq!(
            callbacks[0].callback.ctx,
            OptionRefAny::None,
            "the internal handler must not carry an FFI context",
        );
    }

    #[test]
    fn dom_hands_the_whole_state_wrapper_to_the_handler() {
        // The click handler reads the dialog title, the default dir *and* the user
        // callback out of this RefAny — dropping any of them silently disables them.
        let probe = log_refany();
        let mut fi = FileInput::create(opt("/tmp/original.txt"));
        fi.file_input_state.file_dialog_title = "custom title".into();
        fi.file_input_state.default_dir = opt("/tmp/custom-dir");
        fi.set_on_path_change(probe, record_path as FileInputOnPathChangeCallbackType);
        let expected = fi.file_input_state.clone();

        let state = state_of(&registered_state(&fi.dom()));
        assert_eq!(state.inner, expected.inner);
        assert_eq!(state.file_dialog_title, expected.file_dialog_title);
        assert_eq!(state.default_dir, expected.default_dir);
        let stored = state
            .on_path_change
            .as_ref()
            .expect("the user callback was dropped");
        assert_eq!(cb_addr(&stored.callback), record_path as *const () as usize);
    }

    #[test]
    fn dom_child_count_matches_the_cached_descendant_count() {
        // A too-small `estimated_total_children` makes `convert_dom_into_compact_dom`
        // under-allocate its arenas and panic on out-of-bounds writes.
        for fi in [
            FileInput::create(None.into()),
            FileInput::create(opt("/tmp/x.txt")),
            populated_with_image(),
        ] {
            let has_image = fi.image.as_ref().is_some();
            let dom = fi.dom();
            assert_eq!(
                dom.estimated_total_children,
                count_descendants(&dom),
                "estimated_total_children is out of sync with the real subtree",
            );
            assert_eq!(
                dom.children.as_ref().len(),
                usize::from(has_image) + 1,
                "unexpected child count",
            );
        }
    }

    #[test]
    fn dom_renders_the_image_before_the_label() {
        let fi = populated_with_image();
        let dom = fi.dom();
        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2);
        assert!(
            matches!(children[0].root.get_node_type(), NodeType::Image(_)),
            "the image is not the first child",
        );
        assert_eq!(rendered_label(&dom), "original.txt");
    }

    #[test]
    fn dom_is_deterministic_for_identical_widgets() {
        for path in ["/tmp/a.txt", "/", ""] {
            let a = FileInput::create(opt(path)).dom();
            let b = FileInput::create(opt(path)).dom();
            assert_eq!(a.root.get_node_type(), b.root.get_node_type());
            assert_eq!(classes(&a), classes(&b));
            assert_eq!(rendered_label(&a), rendered_label(&b));
            assert_eq!(a.children.as_ref().len(), b.children.as_ref().len());
        }
    }

    #[cfg(unix)]
    #[test]
    fn dom_does_not_treat_a_backslash_as_a_separator_on_unix() {
        // A backslash is a perfectly legal *character* in a POSIX file name; splitting
        // on it would mislabel (and, worse, misreport) the selected file.
        let dom = FileInput::create(opt("C:\\dir\\file.txt")).dom();
        assert_eq!(rendered_label(&dom), "C:\\dir\\file.txt");

        let dom = FileInput::create(opt("/tmp/a\\b.txt")).dom();
        assert_eq!(rendered_label(&dom), "a\\b.txt");
    }

    #[cfg(unix)]
    #[test]
    fn dom_handles_multiple_leading_slashes_on_unix() {
        assert_eq!(
            rendered_label(&FileInput::create(opt("//")).dom()),
            DEFAULT_TEXT
        );
        assert_eq!(
            rendered_label(&FileInput::create(opt("///")).dom()),
            DEFAULT_TEXT
        );
        assert_eq!(
            rendered_label(&FileInput::create(opt("//a.txt")).dom()),
            "a.txt"
        );
    }

    #[cfg(windows)]
    #[test]
    fn dom_treats_a_backslash_as_a_separator_on_windows() {
        let dom = FileInput::create(opt("C:\\dir\\file.txt")).dom();
        assert_eq!(rendered_label(&dom), "file.txt");
    }

    // ==================================================================
    // fileinput_on_click
    // ==================================================================

    #[test]
    fn click_with_a_foreign_refany_does_nothing() {
        // The handler is reached through an FFI-visible `usize` function pointer, so a
        // mismatched payload is reachable in practice; it must be rejected by the
        // type check rather than reinterpreted.
        let styled = StyledDom::create_from_dom(FileInput::create(opt("/tmp/x.txt")).dom());
        let foreign = RefAny::new(0xDEAD_BEEF_u32);

        let (update, changes) = click(styled, &foreign, node(0));

        assert_eq!(
            update,
            Update::DoNothing,
            "a foreign payload must not trigger a relayout",
        );
        assert!(
            changes.is_empty(),
            "a foreign payload pushed changes: {changes:?}"
        );
        let mut foreign = foreign;
        assert_eq!(
            *foreign
                .downcast_ref::<u32>()
                .expect("the payload was overwritten"),
            0xDEAD_BEEF,
        );
    }

    #[test]
    fn click_rejects_the_inner_state_mistaken_for_the_wrapper() {
        // `FileInputState` is the *inner* half of `FileInputStateWrapper` and is what
        // the user callback receives — passing it back in is the most likely confusion,
        // and reinterpreting it would read an `OptionString` as a callback pointer.
        let styled = StyledDom::create_from_dom(FileInput::create(opt("/tmp/x.txt")).dom());
        let inner = RefAny::new(FileInputState {
            path: opt("/tmp/x.txt"),
        });

        let (update, changes) = click(styled, &inner, node(0));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn click_with_a_foreign_refany_ignores_the_hit_node() {
        // `node_none()` is the "nothing concrete was hit" id that panics several
        // `CallbackInfo` queries — the handler must never reach them.
        let styled = StyledDom::create_from_dom(FileInput::create(None.into()).dom());
        let foreign = RefAny::new(0_u8);
        let (update, changes) = click(styled, &foreign, node_none());
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    /// `fileinput_on_click` opens a **modal native file dialog** before it ever reaches
    /// the user callback whenever the `extra` feature is on (which it is by default, on
    /// desktop). Driving the handler with a well-typed payload would block the test run
    /// on a GUI prompt, so everything past the downcast is only exercised in builds
    /// where that block is compiled out. The type-rejection path above runs everywhere.
    #[cfg(any(not(feature = "extra"), target_os = "android", target_os = "ios"))]
    mod without_the_native_dialog {
        use super::*;

        #[test]
        fn click_without_a_user_callback_refreshes_the_dom() {
            let (styled, state) = laid_out(FileInput::create(opt("/tmp/x.txt")));
            let (update, changes) = click(styled, &state, node(0));

            assert_eq!(
                update,
                Update::RefreshDom,
                "a click must relayout so the new label is drawn",
            );
            assert!(
                changes.is_empty(),
                "the handler pushed unexpected changes: {changes:?}"
            );
            assert_eq!(
                state_of(&state)
                    .inner
                    .path
                    .as_ref()
                    .map(|p| p.as_str().to_string()),
                Some("/tmp/x.txt".to_string()),
                "the handler mutated the path without a dialog",
            );
        }

        #[test]
        fn click_upgrades_a_do_nothing_user_callback_to_a_refresh() {
            // `result.max_self(Update::RefreshDom)` deliberately swallows the user's
            // `DoNothing` — the label may have changed, so the DOM must be rebuilt.
            let fi = FileInput::create(opt("/tmp/x.txt")).with_on_path_change(
                RefAny::new(0_u8),
                path_do_nothing as FileInputOnPathChangeCallbackType,
            );
            let (styled, state) = laid_out(fi);
            let (update, _) = click(styled, &state, node(0));
            assert_eq!(update, Update::RefreshDom);
        }

        #[test]
        fn click_preserves_a_stronger_user_update() {
            // `max_self` must not *downgrade* RefreshDomAllWindows to RefreshDom.
            let fi = FileInput::create(opt("/tmp/x.txt")).with_on_path_change(
                RefAny::new(0_u8),
                path_refresh_all as FileInputOnPathChangeCallbackType,
            );
            let (styled, state) = laid_out(fi);
            let (update, _) = click(styled, &state, node(0));
            assert_eq!(update, Update::RefreshDomAllWindows);
        }

        #[test]
        fn click_hands_the_current_path_to_the_user_callback() {
            for path in ["/tmp/a.txt", "", "a\0b", "/tmp/日本語.txt"] {
                let probe = log_refany();
                let fi = FileInput::create(opt(path)).with_on_path_change(
                    probe.clone(),
                    record_path as FileInputOnPathChangeCallbackType,
                );
                let (styled, state) = laid_out(fi);
                let (_, _) = click(styled, &state, node(0));

                assert_eq!(
                    read_log(&probe).seen,
                    vec![Some(path.to_string())],
                    "the callback saw the wrong path for {path:?}",
                );
            }
        }

        #[test]
        fn click_hands_a_missing_path_through_as_none() {
            let probe = log_refany();
            let fi = FileInput::create(None.into()).with_on_path_change(
                probe.clone(),
                record_path as FileInputOnPathChangeCallbackType,
            );
            let (styled, state) = laid_out(fi);
            let (_, _) = click(styled, &state, node(0));
            assert_eq!(read_log(&probe).seen, vec![None]);
        }

        #[test]
        fn repeated_clicks_fire_once_each_and_leave_the_state_alone() {
            let probe = log_refany();
            let fi = FileInput::create(opt("/tmp/a.txt")).with_on_path_change(
                probe.clone(),
                record_path as FileInputOnPathChangeCallbackType,
            );
            let (styled, state) = laid_out(fi);

            for _ in 0..3 {
                let (update, _) = click(styled.clone(), &state, node(0));
                assert_eq!(update, Update::RefreshDom);
            }

            assert_eq!(
                read_log(&probe).seen.len(),
                3,
                "clicks were dropped or doubled"
            );
            assert_eq!(
                state_of(&state)
                    .inner
                    .path
                    .as_ref()
                    .map(|p| p.as_str().to_string()),
                Some("/tmp/a.txt".to_string()),
            );
        }

        /// Records whether a *re-entrant* borrow of the widget state succeeded while the
        /// click handler still holds its own `downcast_mut` guard.
        struct ReentryProbe {
            state: RefAny,
            saw_mut: Option<bool>,
            saw_ref: Option<bool>,
        }

        extern "C" fn probe_reentry(
            mut refany: RefAny,
            _info: CallbackInfo,
            _state: FileInputState,
        ) -> Update {
            let Some(mut probe) = refany.downcast_mut::<ReentryProbe>() else {
                return Update::DoNothing;
            };
            let probe = &mut *probe;
            let saw_mut = probe
                .state
                .downcast_mut::<FileInputStateWrapper>()
                .is_some();
            let saw_ref = probe
                .state
                .downcast_ref::<FileInputStateWrapper>()
                .is_some();
            probe.saw_mut = Some(saw_mut);
            probe.saw_ref = Some(saw_ref);
            Update::DoNothing
        }

        #[test]
        fn a_reentrant_borrow_of_the_state_is_refused_rather_than_aliased() {
            // The handler holds a `RefMut` on the state for its whole body, including
            // the user callback. A user callback that grabs the same state must be told
            // "no" (None) — handing out a second `&mut` to the same bytes would be UB,
            // and panicking/deadlocking would take the app down on a mere double-borrow.
            let probe = RefAny::new(ReentryProbe {
                state: RefAny::new(0_u8),
                saw_mut: None,
                saw_ref: None,
            });
            let fi = FileInput::create(opt("/tmp/a.txt")).with_on_path_change(
                probe.clone(),
                probe_reentry as FileInputOnPathChangeCallbackType,
            );
            let (styled, state) = laid_out(fi);

            {
                let mut handle = probe.clone();
                let mut guard = handle
                    .downcast_mut::<ReentryProbe>()
                    .expect("the probe changed type");
                guard.state = state.clone();
            }

            let (update, _) = click(styled, &state, node(0));

            let mut handle = probe.clone();
            let guard = handle
                .downcast_mut::<ReentryProbe>()
                .expect("the probe changed type");
            assert_eq!(
                guard.saw_mut,
                Some(false),
                "a second &mut to the state was handed out"
            );
            assert_eq!(
                guard.saw_ref,
                Some(false),
                "a & alongside the live &mut was handed out"
            );
            drop(guard);

            assert_eq!(update, Update::RefreshDom);
            assert_eq!(
                state_of(&state)
                    .inner
                    .path
                    .as_ref()
                    .map(|p| p.as_str().to_string()),
                Some("/tmp/a.txt".to_string()),
                "the state was corrupted by the re-entrant attempt",
            );
        }
    }
}
