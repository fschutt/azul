//! AzCode on a phone: the window. See [`crate::phone_model`] for what the phone's window is (the numbers,
//! the state); this builds it - a top bar (the drawer's button, the file tabs, the command palette), the
//! editor, a key row above the soft keyboard, a status bar of one line, the explorer / search / source
//! control / run as a DRAWER from the left over a scrim, and the bottom panel as a SHEET from the bottom
//! (azul's `BottomSheet`).

pub use crate::phone_model::*;

// ==== The window ====

use azul::{
    callbacks::{BottomSheetOnEventCallbackType, ButtonOnClickCallbackType, CallbackType},
    dom::{AccessibilityInfo, AccessibilityRole},
    prelude::*,
    str::String as AzString,
    widgets::{BottomSheet, BottomSheetEvent, BottomSheetEventKind, Button, SheetDetent},
};
use azul_appkit::l10n::{self, t, t_args, Arg};

use crate::{
    app::{AppState, Doc, PanelTab, Side},
    commands, explorer, find_in_files, git_panel, ids, keyrow, palette, panel, run_panel, terminal,
    ui::{self, clickable, column, icon_button_sized, label, with_state, RULE},
};

/// The face of what is not in front (the tab strip's ground, the key row's): the same shade the desktop's
/// recessed parts have.
const RECESSED: &str = "rgba(128, 128, 128, 0.10)";

/// The phone's window: the top bar, the editor, the key row or the status bar, and over them the
/// drawer (with its scrim), the panel's sheet and the palette. The engine insets the window's root by the status bar, the cutout
/// and the home indicator, so none of them is kept clear of here: the keyboard (`lift`), which overlays the window from its
/// bottom edge, is the one room of the system's that is left.
#[must_use]
pub fn window(app: &RefAny, st: &AppState) -> Dom {
    let lift = st.insets.lift();
    let has_file = st.tabs.active().is_some();
    let mut root = Dom::create_div()
        .with_id(ids::PHONE_ROOT)
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px; position: relative; \
             box-sizing: border-box; padding-bottom: {lift}px;"
        ))
        .with_child(top_bar(app, st))
        .with_child(editor_area(app, st));
    if !st.notice.is_empty() {
        root.add_child(notice_strip(app, st));
    }
    if key_row_visible(true, has_file, st.phone.editing, lift) {
        root.add_child(key_row(app));
    }
    // The keyboard covers the status bar's place: with it up the key row is what stands above it.
    if lift == 0.0 {
        root.add_child(status_bar(app, st));
    }
    if st.panel.open {
        root.add_child(sheet(app, st));
    }
    if st.phone.drawer {
        root.add_child(scrim(app));
        root.add_child(drawer(app, st));
    }
    if let Some(overlay) = palette::palette(app, st) {
        root.add_child(overlay);
    }
    root
}

/// The editor: the find and go-to bars over the code view of the file in front, or the welcome page.
fn editor_area(app: &RefAny, st: &AppState) -> Dom {
    let body = match st.tabs.active() {
        Some(doc) => {
            let mut kids = Vec::new();
            if st.find.open {
                kids.push(find_bar(app, st));
            }
            if let Some(typed) = st.goto.as_ref() {
                kids.push(ui::goto_bar(app, typed, true));
            }
            kids.push(ui::code_view(app, st, doc));
            column(kids)
        }
        None => ui::welcome(app, st),
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;")
        .with_child(body)
}

// ---- the top bar ----

/// The top bar: the drawer's button, the file tabs (they scroll sideways), the command palette.
fn top_bar(app: &RefAny, st: &AppState) -> Dom {
    let mut tabs = Dom::create_div().with_id(ids::TABS).with_css(
        "display: flex; flex-direction: row; align-items: stretch; flex-grow: 1; flex-shrink: 1; min-width: 0px; \
         height: 100%; overflow-x: auto; overflow-y: hidden; scrollbar-width: none;",
    );
    for (i, doc) in st.tabs.docs.iter().enumerate() {
        tabs.add_child(tab(app, i, doc, i == st.tabs.active));
    }
    Dom::create_div()
        .with_id(ids::TOP_BAR)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: {TOP_BAR}px; \
             box-sizing: border-box; border-bottom: 1px solid {RULE};"
        ))
        .with_child(icon_button_sized(
            ids::DRAWER_BUTTON,
            "menu",
            if st.phone.drawer { "azcode-close-drawer" } else { "azcode-open-drawer" },
            app.clone(),
            on_drawer,
            TOUCH,
        ))
        .with_child(tabs)
        .with_child(icon_button_sized(
            ids::PALETTE_BUTTON,
            "more_vert",
            "azcode-action-command-palette",
            app.clone(),
            on_palette,
            TOUCH,
        ))
}

/// Tab `index`: the file's name (a tap brings it to front) and a close button, each a finger wide, the
/// one in front under the accent's line, a dot over the close button while the file has unsaved changes.
fn tab(app: &RefAny, index: usize, doc: &Doc, active: bool) -> Dom {
    let face = if active {
        // Under flora the line is the stone (its glow at night).
        "border-bottom: 2px solid var(--az-accent, #0078D4); @theme(flora) { border-bottom: 2px solid system:accent; }"
    } else {
        "border-bottom: 2px solid transparent; opacity: 0.75;"
    };
    let reference = || RefAny::new(ui::TabRef { app: app.clone(), index });
    let name = clickable(
        ids::tab(index),
        &format!(
            "display: flex; flex-direction: row; align-items: center; height: 100%; padding: 0px 4px 0px 12px; \
             font-size: 14px; white-space: nowrap; cursor: pointer; box-sizing: border-box; {face}"
        ),
        &doc.name,
        AccessibilityRole::PageTab,
        reference(),
        ui::on_tab_click,
    )
    .with_class(ids::TAB_CLASS)
    .with_child(Dom::create_span_with_text(doc.name.as_str()));
    let glyph = if doc.dirty {
        Dom::create_div()
            .with_class(ids::TAB_DIRTY_CLASS)
            .with_css("width: 10px; height: 10px; border-radius: 5px; background: system:text; opacity: 0.85;")
    } else {
        Dom::create_icon("close").with_css(format!("font-size: 18px; {}", ui::FLORA_GLYPH))
    };
    let close = clickable(
        ids::tab_close(index),
        "display: flex; flex-direction: row; align-items: center; justify-content: center; width: 48px; height: 100%; \
         flex-shrink: 0; cursor: pointer;",
        &t_args("azcode-close-tab", &[("name", Arg::from(doc.name.as_str()))]),
        AccessibilityRole::PushButton,
        reference(),
        ui::on_tab_close,
    )
    .with_child(glyph);
    let mut tab = Dom::create_div()
        .with_id(ids::tab_slot(index))
        .with_css(format!("display: flex; flex-direction: row; align-items: stretch; flex-shrink: 0; height: 100%; border-right: 1px solid {RULE};"))
        .with_child(name)
        .with_child(close);
    if active {
        tab = tab.with_class(ids::TAB_ACTIVE_CLASS);
    }
    tab
}

extern "C" fn on_drawer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, _| {
        st.phone.toggle_drawer();
        println!("AZCODE_DRAWER {}", if st.phone.drawer { "open" } else { "closed" });
        // The keyboard has no business over the drawer.
        info.request_soft_keyboard(false);
    })
}

extern "C" fn on_palette(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, app| {
        st.phone.editing = false;
        info.request_soft_keyboard(false);
        crate::actions::run(st, info, app, crate::actions::Action::CommandPalette);
    })
}

// ---- the drawer ----

/// The scrim over the editor while the drawer is open: a tap anywhere on it closes the drawer.
fn scrim(app: &RefAny) -> Dom {
    clickable(
        ids::SCRIM,
        "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; background: rgba(0, 0, 0, 0.45); cursor: pointer;",
        "azcode-close-drawer",
        AccessibilityRole::PushButton,
        app.clone(),
        on_scrim,
    )
}

extern "C" fn on_scrim(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| st.phone.drawer = false)
}

/// The drawer: the four views' icons in a row (a finger each), Settings, and the view in front.
fn drawer(app: &RefAny, st: &AppState) -> Dom {
    let side = match st.side {
        Side::Explorer => explorer::explorer(app, st),
        Side::Search => find_in_files::search_panel(app, st),
        Side::Git => git_panel::git_panel(app, st),
        Side::Run => run_panel::run_panel(app, st),
    };
    Dom::create_div()
        .with_id(ids::DRAWER)
        .with_accessibility_info(AccessibilityInfo::named(l10n::label("azcode-drawer"), AccessibilityRole::Pane))
        .with_css(format!(
            "position: absolute; left: 0px; top: 0px; bottom: 0px; width: {}px; display: flex; flex-direction: column; \
             box-sizing: border-box; background: system:window-background; color: system:text; \
             box-shadow: 2px 0px 12px rgba(0, 0, 0, 0.4);",
            drawer_width(st.window.0)
        ))
        .with_child(activity_row(app, st))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
                .with_child(side),
        )
}

/// The drawer's icons: Explorer, Search, Source Control, Run, Settings.
fn activity_row(app: &RefAny, st: &AppState) -> Dom {
    let entries: [(AzString, &str, &str, Side, CallbackType); 4] = [
        (ids::ACTIVITY_EXPLORER, "content_copy", "azcode-action-explorer", Side::Explorer, on_pick_explorer),
        (ids::ACTIVITY_SEARCH, "search", "azcode-category-search", Side::Search, on_pick_search),
        (ids::ACTIVITY_GIT, "call_split", "azcode-action-git", Side::Git, on_pick_git),
        (ids::ACTIVITY_RUN, "play_arrow", "azcode-action-run", Side::Run, on_pick_run),
    ];
    let mut row = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: {TOUCH}px; border-bottom: 1px solid {RULE};"
    ));
    for (id, icon, name, side, callback) in entries {
        row.add_child(activity_icon(app, id, icon, name, st.side == side, callback));
    }
    row.add_child(activity_icon(app, ids::ACTIVITY_SETTINGS, "settings", "azcode-action-settings", false, on_pick_settings));
    row
}

/// One icon of the drawer's row: a finger wide, the view in front under the accent's line.
fn activity_icon(app: &RefAny, id: AzString, icon: &str, name: &str, active: bool, callback: CallbackType) -> Dom {
    let look = if active {
        "border-bottom: 2px solid var(--az-accent, #0078D4); @theme(flora) { border-bottom: 2px solid system:accent; }"
    } else {
        "border-bottom: 2px solid transparent; opacity: 0.7;"
    };
    clickable(
        id,
        &format!(
            "display: flex; flex-direction: row; align-items: center; justify-content: center; flex-grow: 1; height: 100%; \
             min-width: {TOUCH}px; cursor: pointer; box-sizing: border-box; {look}"
        ),
        name,
        AccessibilityRole::PageTab,
        app.clone(),
        callback,
    )
    .with_child(Dom::create_icon(icon).with_css(format!("font-size: 24px; {}", ui::FLORA_GLYPH)))
}

extern "C" fn on_pick_explorer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| st.side = Side::Explorer)
}

extern "C" fn on_pick_search(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, _| {
        st.side = Side::Search;
        commands::focus_soon(info, ids::SEARCH_INPUT.as_str());
    })
}

extern "C" fn on_pick_git(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, app| {
        st.side = Side::Git;
        commands::refresh_git(st, info, app);
    })
}

extern "C" fn on_pick_run(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| st.side = Side::Run)
}

extern "C" fn on_pick_settings(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| {
        st.phone.drawer = false;
        azul_appkit::ui::open_settings(&st.kit, None);
    })
}

// ---- the sheet ----

/// The widget's detent for ours, and back.
const fn sheet_detent(d: Detent) -> SheetDetent {
    match d {
        Detent::Collapsed => SheetDetent::Collapsed,
        Detent::Peek => SheetDetent::Peek,
        Detent::Half => SheetDetent::Half,
        Detent::Full => SheetDetent::Full,
    }
}

/// Our detent for the widget's.
const fn detent_of(d: SheetDetent) -> Detent {
    match d {
        SheetDetent::Collapsed => Detent::Collapsed,
        SheetDetent::Peek => Detent::Peek,
        SheetDetent::Half => Detent::Half,
        SheetDetent::Full => Detent::Full,
    }
}

/// The panel (terminal, problems, output) as a sheet over the editor's lower part: dragged by its grip and
/// the tabs under it between its detents, above the keyboard when one is up.
fn sheet(app: &RefAny, st: &AppState) -> Dom {
    let insets = st.insets;
    BottomSheet::create(panel::panel(app, st, true))
        .with_label(l10n::label("azcode-sheet"))
        .with_grip_label(l10n::label("azcode-sheet-grip"))
        .with_detent(sheet_detent(st.phone.sheet))
        .with_lowest(SheetDetent::Collapsed)
        .with_highest(SheetDetent::Full)
        // the root's height (the window less the bars): the sheet lies on its lower edge, and drawn up it stops under its top
        .with_container_height(st.window.1)
        .with_collapsed_height(SHEET_COLLAPSED)
        .with_top_margin(SHEET_TOP_MARGIN)
        .with_bottom_inset(insets.lift())
        .with_safe_bottom(insets.bottom_clear())
        // The grip and the tabs under it take the drag.
        .with_grab_height(32.0 + panel::PHONE_HEADER_HEIGHT)
        .with_on_event(app.clone(), on_sheet as BottomSheetOnEventCallbackType)
        .dom()
        .with_id(ids::SHEET)
}

/// The sheet was let go: its detent is kept for the next build (the sheet has set its own height already).
extern "C" fn on_sheet(mut data: RefAny, _info: CallbackInfo, event: BottomSheetEvent) -> Update {
    if event.kind == BottomSheetEventKind::Settled {
        if let Some(mut st) = data.downcast_mut::<AppState>() {
            st.phone.sheet = detent_of(event.detent);
        }
    }
    Update::DoNothing
}

// ---- the key row ----

/// What a key of the row acts on.
struct KeyRef {
    app: RefAny,
    index: usize,
}

/// The key row: Tab, the braces and brackets, the quotes, the arrows - a finger each, it scrolls sideways.
fn key_row(app: &RefAny) -> Dom {
    let mut row = Dom::create_div()
        .with_id(ids::KEY_ROW)
        .with_accessibility_info(AccessibilityInfo::named(l10n::label("azcode-key-row"), AccessibilityRole::Toolbar))
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: {KEY_ROW}px; padding: 0px 4px; \
             box-sizing: border-box; overflow-x: auto; overflow-y: hidden; scrollbar-width: none; border-top: 1px solid {RULE}; \
             background: {RECESSED};"
        ));
    for (index, key) in keyrow::KEYS.iter().enumerate() {
        let content = if key.icon.is_empty() {
            Dom::create_span_with_text(key.label)
        } else {
            Dom::create_icon(key.icon).with_css(format!("font-size: 22px; {}", ui::FLORA_GLYPH))
        };
        row.add_child(
            clickable(
                ids::key(index),
                "display: flex; flex-direction: row; align-items: center; justify-content: center; min-width: 48px; \
                 height: 48px; padding: 0px 8px; margin: 0px 2px; flex-shrink: 0; border-radius: 6px; font-size: 16px; \
                 font-family: monospace; cursor: pointer; background: rgba(128, 128, 128, 0.18); \
                 :hover { background: rgba(128, 128, 128, 0.30); }",
                &l10n::t(key.name),
                AccessibilityRole::PushButton,
                RefAny::new(KeyRef { app: app.clone(), index }),
                on_key,
            )
            .with_child(content),
        );
    }
    row
}

extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<KeyRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |st, info, _| {
        let Some(key) = keyrow::KEYS.get(index) else {
            return;
        };
        println!("AZCODE_KEY {}", key.name);
        let pressed = st.tabs.active_mut().is_some_and(|doc| doc.press_key(key.action));
        if pressed && st.find.open {
            st.refresh_find();
        }
        // The press must not take the editor's focus, and with it the keyboard, away.
        commands::focus_soon(info, ids::EDITOR.as_str());
        info.request_soft_keyboard(true);
    })
}

// ---- the status bar ----

/// One segment of the status bar: an icon and/or a text, a finger tall; a tap runs `callback` when there is one.
fn segment(app: &RefAny, id: AzString, icon: &str, text: &str, name: &str, callback: Option<CallbackType>) -> Dom {
    let css = format!(
        "display: flex; flex-direction: row; align-items: center; justify-content: center; height: 100%; min-width: 48px; \
         padding: 0px 8px; flex-shrink: 0; font-size: 12px; white-space: nowrap; {}",
        if callback.is_some() { "cursor: pointer; :hover { background: rgba(128, 128, 128, 0.20); }" } else { "" }
    );
    let mut dom = match callback {
        Some(callback) => clickable(id, &css, name, AccessibilityRole::PushButton, app.clone(), callback),
        None => Dom::create_div().with_id(id).with_css(css.as_str()).with_accessibility_name(name),
    };
    if !icon.is_empty() {
        dom.add_child(Dom::create_icon(icon).with_css(format!("font-size: 18px; {}", ui::FLORA_GLYPH)));
    }
    if !text.is_empty() {
        dom.add_child(Dom::create_span_with_text(text).with_css(if icon.is_empty() { "" } else { "padding-left: 4px;" }));
    }
    dom
}

/// The status bar, one line a finger tall (the root ends over the home indicator): the branch, the panel's three
/// views, the caret (a tap goes to a line), the language.
fn status_bar(app: &RefAny, st: &AppState) -> Dom {
    let mut bar = Dom::create_div().with_id(ids::STATUS_BAR).with_css(format!(
        "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: {STATUS_BAR}px; \
         box-sizing: border-box; overflow-x: auto; overflow-y: hidden; scrollbar-width: none; border-top: 1px solid {RULE}; \
         background: {RECESSED};"
    ));
    if let Some(branch) = st.branch.as_deref() {
        bar.add_child(segment(app, ids::STATUS_BRANCH, "call_split", branch, branch, None));
    }
    if terminal::AVAILABLE {
        let name = l10n::t("azcode-action-terminal");
        bar.add_child(segment(app, ids::STATUS_TERMINAL, "terminal", "", &name, Some(on_status_terminal)));
    }
    let problems = st.diag.problems().len();
    bar.add_child(segment(
        app,
        ids::STATUS_PROBLEMS,
        "error_outline",
        &problems.to_string(),
        &t_args("azcode-status-problems", &[("count", Arg::from(problems))]),
        Some(on_status_problems),
    ));
    bar.add_child(segment(app, ids::STATUS_OUTPUT, "article", "", &l10n::t("azcode-panel-output"), Some(on_status_output)));
    bar.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    if let Some(doc) = st.tabs.active() {
        let caret = doc.caret_label(ui::TAB_WIDTH as usize);
        bar.add_child(segment(app, ids::STATUS_CARET, "", &caret, &caret, Some(on_status_caret)));
        bar.add_child(segment(app, ids::STATUS_LANGUAGE, "", &doc.language, &doc.language, None));
    }
    bar
}

extern "C" fn on_status_terminal(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, app| panel::toggle_tab(st, info, app, PanelTab::Terminal))
}

extern "C" fn on_status_problems(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, app| panel::toggle_tab(st, info, app, PanelTab::Problems))
}

extern "C" fn on_status_output(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, app| panel::toggle_tab(st, info, app, PanelTab::Output))
}

extern "C" fn on_status_caret(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, app| {
        crate::actions::run(st, info, app, crate::actions::Action::GoToLine);
    })
}

/// The last notice, one line over the status bar; a tap puts it away.
fn notice_strip(app: &RefAny, st: &AppState) -> Dom {
    clickable(
        ids::NOTICE,
        &format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; min-height: 36px; padding: 4px 12px; \
             font-size: 12px; white-space: normal; cursor: pointer; border-top: 1px solid {RULE}; background: {RECESSED};"
        ),
        &st.notice,
        AccessibilityRole::PushButton,
        app.clone(),
        on_notice,
    )
    .with_child(Dom::create_span_with_text(st.notice.as_str()))
}

extern "C" fn on_notice(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, _, _| st.notice.clear())
}

// ---- the find bar ----

/// The find bar of a phone: the field, "3 of 14", previous, next, close - a finger each; with replace
/// open, a second row with the replacement and its two buttons.
fn find_bar(app: &RefAny, st: &AppState) -> Dom {
    let mut bar = Dom::create_div()
        .with_id(ids::FIND_BAR)
        .with_css(format!(
            "display: flex; flex-direction: column; flex-shrink: 0; padding: 2px 4px; border-bottom: 1px solid {RULE}; background: {RECESSED};"
        ))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(Dom::create_div().with_css("flex-grow: 1; min-width: 0px;").with_child(ui::find_input(app, st, ids::FIND_INPUT)))
                .with_child(label(&ui::find_count(st), "flex-shrink: 0; font-size: 12px; padding: 0px 6px; white-space: nowrap;").with_id(ids::FIND_COUNT))
                .with_child(icon_button_sized(ids::FIND_PREVIOUS, "keyboard_arrow_up", "azcode-find-previous", app.clone(), ui::on_find_previous, TOUCH))
                .with_child(icon_button_sized(ids::FIND_NEXT, "keyboard_arrow_down", "azcode-find-next", app.clone(), ui::on_find_next, TOUCH))
                .with_child(icon_button_sized(ids::FIND_CLOSE, "close", "azcode-find-close", app.clone(), on_find_close, TOUCH)),
        );
    if st.find.replace_open {
        bar.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(Dom::create_div().with_css("flex-grow: 1; min-width: 0px;").with_child(ui::replace_input(app, st)))
                .with_child(
                    Button::create(l10n::label("azcode-action-replace"))
                        .with_on_click(app.clone(), ui::on_replace_one as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::REPLACE_ONE),
                )
                .with_child(
                    Button::create(l10n::label("azcode-replace-all"))
                        .with_on_click(app.clone(), ui::on_replace_all as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::REPLACE_ALL),
                ),
        );
    }
    bar
}

extern "C" fn on_find_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |st, info, _| {
        st.find.open = false;
        commands::focus_soon(info, ids::EDITOR.as_str());
    })
}
