//! Fluent localization through the real shell pipeline (`HeadlessWindow`).
//!
//! The app registers its `.ftl` sources in `AppConfig::fluent_locales` (the
//! "Startup" section of `doc/guide/en/architecture/localization.md`), its
//! `layout()` returns `AzString::tr` keys, and what the window lays out must
//! be the translation - not the key, and not last frame's translation.
#![cfg(feature = "fluent")]

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    },
};

use azul::desktop::shell2::{
    common::{event::SharedUndoManager, PlatformWindow},
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId, FluentArg, FluentArgKV, NodeType},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
    window::{AzStringPair, StringPairVec},
};
use azul_css::{corety::AzString, system::SystemLanguage};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

const EN: &str = "greeting = Hello
unread = { $count ->
    [one] one new email
   *[other] { $count } new emails
}
";

const DE: &str = "greeting = Hallo
unread = { $count ->
    [one] eine neue E-Mail
   *[other] { $count } neue E-Mails
}
";

#[derive(Clone)]
struct Model {
    unread: Arc<AtomicI32>,
    layout_calls: Arc<AtomicI32>,
}

impl Model {
    fn new(unread: i32) -> Self {
        Self {
            unread: Arc::new(AtomicI32::new(unread)),
            layout_calls: Arc::new(AtomicI32::new(0)),
        }
    }
}

/// The guide's shapes: a key as a paragraph's text, and a key whose
/// arguments hang on the paragraph.
extern "C" fn layout_cb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let unread = match data.downcast_ref::<Model>() {
        Some(model) => {
            model.layout_calls.fetch_add(1, Ordering::SeqCst);
            model.unread.load(Ordering::SeqCst)
        }
        None => 0,
    };
    Dom::create_body()
        .with_child(Dom::create_p_with_text(AzString::tr("greeting")))
        .with_child(
            Dom::create_p_with_text(AzString::tr("unread")).with_fluent_args(vec![FluentArgKV {
                key: "count".into(),
                value: FluentArg::I32(unread),
            }]),
        )
}

fn make_window(model: Model) -> HeadlessWindow {
    let mut config = AppConfig::default();
    config.fluent_locales = StringPairVec::from_vec(vec![
        AzStringPair {
            key: "en".into(),
            value: EN.into(),
        },
        AzStringPair {
            key: "de".into(),
            value: DE.into(),
        },
    ]);
    // Hermetic: the window starts in en-US whatever the host desktop says.
    config.system_style.language = SystemLanguage::new("en-US", false);

    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout_cb,
        ctx: OptionRefAny::None,
    };

    HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(model))),
        SharedUndoManager::new(),
        config,
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

/// Every text the root DOM lays out, in document order.
fn laid_out_texts(window: &HeadlessWindow) -> Vec<String> {
    let layout_window = window
        .common
        .layout_window
        .as_ref()
        .expect("layout window present after regenerate_layout");
    let layout_result = layout_window
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("root DOM laid out");
    layout_result
        .styled_dom
        .node_data
        .as_ref()
        .iter()
        .filter_map(|node| match node.get_node_type() {
            NodeType::Text(text) => Some(text.as_ref().as_str().to_string()),
            _ => None,
        })
        .collect()
}

fn has(texts: &[String], wanted: &str) -> bool {
    texts.iter().any(|t| t == wanted)
}

#[test]
fn a_translation_key_in_the_app_dom_is_laid_out_as_its_translation() {
    let mut window = make_window(Model::new(3));
    window.regenerate_layout().expect("first layout");

    let texts = laid_out_texts(&window);
    // The OS says en-US; the app registered `en` - the language fallback.
    assert!(has(&texts, "Hello"), "got {texts:?}");
    assert!(has(&texts, "3 new emails"), "got {texts:?}");
}

#[test]
fn a_changed_argument_is_laid_out_after_the_next_rebuild() {
    let model = Model::new(3);
    let mut window = make_window(model.clone());
    window.regenerate_layout().expect("first layout");

    model.unread.store(1, Ordering::SeqCst);
    window.regenerate_layout().expect("second layout");

    let texts = laid_out_texts(&window);
    assert!(has(&texts, "one new email"), "got {texts:?}");
}
