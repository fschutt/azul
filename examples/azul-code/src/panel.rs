//! The bottom panel (View > Terminal, Ctrl+`, Mod+J, or a tab of the status bar): VSCode's panel under
//! the editor - a header with its three tabs, TERMINAL, PROBLEMS and OUTPUT, over the view of the one
//! in front. The terminal is [`crate::terminal`]'s (a shell on a PTY in azul's `TerminalView`); the
//! problems and the output are what AzCode says about its own work ([`crate::diagnostics`]): a file
//! that could not be read, a folder opened, a search run - each problem with a file opens it on a
//! click. On a phone the same panel is the content of the bottom sheet, its tabs a finger tall.
//!
//! On stdout (for scripts): `AZCODE_PANEL open|closed`, `AZCODE_PANEL_TAB terminal|problems|output`.

use azul::{
    dom::{AccessibilityInfo, AccessibilityRole},
    prelude::*,
    str::String as AzString,
};
use azul_appkit::l10n::{self, t};

use crate::{
    app::{AppState, PanelTab},
    commands,
    diagnostics::Severity,
    ids, terminal, ui,
};

/// The header's height on a desktop, px.
pub const HEADER_HEIGHT: f32 = 35.0;
/// The header's height on a phone, px: a finger's.
pub const PHONE_HEADER_HEIGHT: f32 = 48.0;
/// The lines of output the panel shows, the newest first (the list keeps more).
pub const OUTPUT_SHOWN: usize = 300;

/// The tabs this device can show, in the order they stand: the terminal where a shell can run,
/// then the problems and the output.
#[must_use]
pub fn tabs() -> Vec<PanelTab> {
    if terminal::AVAILABLE {
        vec![PanelTab::Terminal, PanelTab::Problems, PanelTab::Output]
    } else {
        vec![PanelTab::Problems, PanelTab::Output]
    }
}

/// The key of the words that name a tab.
#[must_use]
pub const fn tab_name(tab: PanelTab) -> &'static str {
    match tab {
        PanelTab::Terminal => "azcode-action-terminal",
        PanelTab::Problems => "azcode-panel-problems",
        PanelTab::Output => "azcode-panel-output",
    }
}

/// What stdout says of a tab.
#[must_use]
pub const fn tab_key(tab: PanelTab) -> &'static str {
    match tab {
        PanelTab::Terminal => "terminal",
        PanelTab::Problems => "problems",
        PanelTab::Output => "output",
    }
}

/// The DOM id of a tab's button.
fn tab_id(tab: PanelTab) -> AzString {
    match tab {
        PanelTab::Terminal => ids::PANEL_TAB_TERMINAL,
        PanelTab::Problems => ids::PANEL_TAB_PROBLEMS,
        PanelTab::Output => ids::PANEL_TAB_OUTPUT,
    }
}

/// A status bar tab: the panel shows `tab`; when it already does, it closes.
pub fn toggle_tab(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, tab: PanelTab) {
    if st.panel.open && st.panel_tab == tab {
        terminal::close(st, info);
        return;
    }
    terminal::show_tab(st, info, app, tab);
    println!("AZCODE_PANEL_TAB {}", tab_key(st.panel_tab));
}

/// What a tab's button acts on.
struct TabRef {
    app: RefAny,
    tab: PanelTab,
}

/// What a problem's row acts on.
struct ProblemRef {
    app: RefAny,
    index: usize,
}

/// The panel: its header over the view of the tab in front; `phone`: the tabs and the buttons a
/// finger's size (the panel is the bottom sheet's content).
#[must_use]
pub fn panel(app: &RefAny, st: &AppState, phone: bool) -> Dom {
    let height = if phone { PHONE_HEADER_HEIGHT } else { HEADER_HEIGHT };
    let button = if phone { PHONE_HEADER_HEIGHT } else { 22.0 };
    let tab = if tabs().contains(&st.panel_tab) { st.panel_tab } else { PanelTab::Problems };
    let mut header = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; height: {height}px; flex-shrink: 0; \
             padding: 0px 8px 0px {}px; border-top: 1px solid {}; overflow: hidden;",
            if phone { 4 } else { 16 },
            ui::RULE
        ))
        .with_id(ids::PANEL_HEADER);
    for t in tabs() {
        header.add_child(tab_button(app, st, t, t == tab, height));
    }
    header.add_child(Dom::create_div().with_css("flex-grow: 1; min-width: 4px;"));
    match tab {
        PanelTab::Terminal => {
            for tab in terminal::shell_tabs(app, st, if phone { 36.0 } else { 22.0 }) {
                header.add_child(tab);
            }
            for action in terminal::actions(app, button) {
                header.add_child(action);
            }
        }
        PanelTab::Problems | PanelTab::Output => {
            header.add_child(ui::icon_button_sized(
                ids::PANEL_CLEAR,
                "delete_sweep",
                "azcode-panel-clear",
                app.clone(),
                on_clear,
                button,
            ));
        }
    }
    header.add_child(ui::icon_button_sized(
        ids::PANEL_CLOSE,
        "close",
        "azcode-close-panel",
        app.clone(),
        terminal::on_close_panel,
        button,
    ));
    let body = match tab {
        PanelTab::Terminal => terminal::view(app, st),
        PanelTab::Problems => problems_view(app, st, phone),
        PanelTab::Output => output_view(st, phone),
    };
    Dom::create_div()
        .with_id(ids::PANEL)
        // The split pane is a block: the panel takes its whole height itself
        // (`flex-grow` alone left it as tall as its header, and the terminal
        // under it 0 px tall - its view never rendered a row).
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; height: 100%; min-height: 0px; \
             min-width: 0px;",
        )
        .with_child(header)
        .with_child(body)
}

/// One tab of the header: its name (and the problems' count), the one in front underlined.
fn tab_button(app: &RefAny, st: &AppState, tab: PanelTab, active: bool, height: f32) -> Dom {
    let mut name = l10n::t(tab_name(tab)).to_uppercase();
    if tab == PanelTab::Problems && !st.diag.problems().is_empty() {
        name = format!("{name} {}", st.diag.problems().len());
    }
    let look = if active {
        // Under flora the underline is the stone (its glow at night).
        "opacity: 1; border-bottom: 1px solid var(--az-accent, #0078D4); @theme(flora) { border-bottom: 1px solid system:accent; }"
    } else {
        "opacity: 0.7; border-bottom: 1px solid transparent; :hover { opacity: 1; }"
    };
    let size = if height > 40.0 { 13 } else { 11 };
    ui::clickable(
        tab_id(tab),
        &format!(
            "display: flex; flex-direction: row; align-items: center; height: {height}px; flex-shrink: 0; \
             padding: 0px {}px; font-size: {size}px; white-space: nowrap; cursor: pointer; box-sizing: border-box; {look}",
            if height > 40.0 { 14 } else { 10 }
        ),
        &l10n::t(tab_name(tab)),
        AccessibilityRole::PageTab,
        RefAny::new(TabRef { app: app.clone(), tab }),
        on_tab,
    )
    .with_child(Dom::create_span_with_text(name))
}

/// A line that says there is nothing to show, in the middle of the view.
fn empty(key: &str, id: AzString) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; justify-content: center; flex-grow: 1; \
             padding: 12px 16px; font-size: 13px; opacity: 0.7;",
        )
        .with_child(Dom::create_span_with_text(l10n::label(key)))
}

/// The colour of a problem's icon.
const fn severity_colour(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "#f14c4c",
        Severity::Warning => "#cca700",
        Severity::Info => "#3794ff",
    }
}

/// The Problems list, the newest first: an icon, the message, the file it is about (a click opens it).
fn problems_view(app: &RefAny, st: &AppState, phone: bool) -> Dom {
    let problems = st.diag.problems();
    if problems.is_empty() {
        return empty("azcode-no-problems", ids::PROBLEMS);
    }
    let row_height = if phone { 48.0 } else { 24.0 };
    let mut list = Dom::create_div()
        .with_id(ids::PROBLEMS)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto;")
        .with_accessibility_info(AccessibilityInfo::named(l10n::label("azcode-panel-problems"), AccessibilityRole::List));
    for (i, problem) in problems.iter().enumerate().rev() {
        let mut row = ui::clickable(
            ids::problem(i),
            &format!(
                "display: flex; flex-direction: row; align-items: center; min-height: {row_height}px; padding: 2px 16px; \
                 flex-shrink: 0; font-size: 13px; cursor: pointer; :hover {{ background: rgba(128, 128, 128, 0.15); }}"
            ),
            &format!("{}: {}", t(problem.severity.name()), problem.message),
            AccessibilityRole::ListItem,
            RefAny::new(ProblemRef { app: app.clone(), index: i }),
            on_problem,
        )
        .with_child(
            Dom::create_icon(problem.severity.icon())
                .with_css(format!("font-size: 16px; padding-right: 8px; flex-shrink: 0; color: {};", severity_colour(problem.severity))),
        )
        .with_child(ui::label(&problem.message, "min-width: 0px; white-space: normal;"));
        if let Some(file) = &problem.file {
            row.add_child(ui::label(file, "font-size: 12px; opacity: 0.6; padding-left: 10px; white-space: nowrap; flex-shrink: 0;"));
        }
        list.add_child(row);
    }
    list
}

/// The Output, the newest line first.
fn output_view(st: &AppState, _phone: bool) -> Dom {
    if st.diag.output_len() == 0 {
        return empty("azcode-no-output", ids::OUTPUT);
    }
    let lines: Vec<&str> = st.diag.output().collect();
    let mut list = Dom::create_div()
        .with_id(ids::OUTPUT)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; padding: 4px 0px;")
        .with_accessibility_info(AccessibilityInfo::named(l10n::label("azcode-panel-output"), AccessibilityRole::List));
    for line in lines.iter().rev().take(OUTPUT_SHOWN) {
        list.add_child(
            Dom::create_div()
                .with_css("padding: 1px 16px; font-size: 12px; font-family: monospace; white-space: pre-wrap; flex-shrink: 0;")
                .with_child(Dom::create_span_with_text(*line)),
        );
    }
    list
}

extern "C" fn on_tab(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, tab)) = data.downcast_ref::<TabRef>().map(|r| (r.app.clone(), r.tab)) else {
        return Update::DoNothing;
    };
    ui::with_state(&mut app, &mut info, |st, info, app| {
        if tab == PanelTab::Terminal {
            terminal::open(st, info, app);
        } else {
            terminal::show_tab(st, info, app, tab);
        }
        println!("AZCODE_PANEL_TAB {}", tab_key(st.panel_tab));
    })
}

extern "C" fn on_clear(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, |st, _, _| match st.panel_tab {
        PanelTab::Problems => st.diag.clear_problems(),
        PanelTab::Output => st.diag.clear_output(),
        PanelTab::Terminal => {}
    })
}

extern "C" fn on_problem(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<ProblemRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    ui::with_state(&mut app, &mut info, |st, info, app| {
        let file = st.diag.problems().get(index).and_then(|p| p.file.clone());
        if let Some(file) = file {
            commands::open_file(st, info, app, &file);
        }
    })
}
