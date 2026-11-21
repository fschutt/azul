//! A settings window as narrow as an installer's (640 px, AzSetup's) keeps every control whole
//! and inside it.
//!
//! A row is the label column, then the control. The label column kept its full 240 px however
//! narrow the window was, so the control got what was left: a shortcut field (160 px at least)
//! ran out of the window's right edge, and a folder field shrank to a sliver beside its Browse
//! button ("/User"). Labels wrap; controls do not. In Chrome a flex item with `flex-shrink: 0`
//! keeps its width the same way, so the label column has to give way instead.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::VirtualKeyCode,
};
use azul_css::{AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        path_input::PATH_FIELD_CLASS,
        shells::{
            settings_dialog::ShellSettingVec, ShellSetting, ShellSettingShortcut,
            ShellSettingValue, ShellSettingsDialog,
        },
        shortcut_recorder::RECORDER_CLASS,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 514.0;

fn dialog() -> Dom {
    let hotkey = azul_core::global_hotkey::GlobalHotkey::create(
        azul_core::global_hotkey::HotkeyModifiers {
            ctrl: true,
            alt: false,
            shift: true,
            meta: false,
        },
        VirtualKeyCode::P,
    );
    ShellSettingsDialog::create(StringVec::from_vec(vec![AzString::from("General")]))
        .with_settings(ShellSettingVec::from_vec(vec![
            ShellSetting::create(
                AzString::from("general.folder"),
                AzString::from("Documents folder"),
                0,
                AzString::from("Files"),
                ShellSettingValue::Path(AzString::from("/Users/someone/Documents")),
            )
            .with_help(AzString::from("Where Save puts a new document.")),
            ShellSetting::create(
                AzString::from("editing.palette"),
                AzString::from("Command palette"),
                0,
                AzString::from("Keyboard"),
                ShellSettingValue::Shortcut(ShellSettingShortcut::create(hotkey)),
            ),
        ]))
        .dom()
}

fn lay_out() -> LayoutWindow {
    let mut body = Dom::create_body().with_child(dialog());
    body.root.set_css(&format!(
        "margin: 0px; width: {WIDTH}px; height: {HEIGHT}px; display: flex; flex-direction: column;"
    ));
    let styled = StyledDom::create_from_dom(body);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("the dialog lays out");
    lw
}

fn rect_of_class(lw: &LayoutWindow, class: &str) -> LogicalRect {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|n| {
            n.get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(name) if name.as_str() == class))
        })
        .unwrap_or_else(|| panic!("no .{class}"));
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!(".{class} is laid out"))
}

#[test]
fn a_narrow_settings_window_keeps_every_control_whole_and_inside() {
    let lw = lay_out();
    let recorder = rect_of_class(&lw, RECORDER_CLASS);
    assert!(
        recorder.origin.x + recorder.size.width <= WIDTH + 0.5,
        "the shortcut field ends inside the {WIDTH} px window: {recorder:?}"
    );
    let field = rect_of_class(&lw, PATH_FIELD_CLASS);
    assert!(
        field.size.width >= 120.0,
        "the folder field shows a path, not a sliver beside Browse: {field:?}"
    );
    assert!(
        field.origin.x + field.size.width <= WIDTH + 0.5,
        "the folder field ends inside the window: {field:?}"
    );
}
