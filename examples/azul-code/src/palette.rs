//! The palette over the window, azul's `ShellCommandPalette`: QUICK OPEN
//! (Mod+P, Go > Go to File...) over the folder's files - the best
//! [`crate::workspace::QUICK_MAX`] for what was typed - and the COMMAND
//! PALETTE (Mod+Shift+P, View > Command Palette..., or `>` typed into quick
//! open) over the window's commands ([`crate::actions`]), "View: Terminal"
//! and the rest, each with its keys.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, ShellCommandPaletteOnQueryCallbackType,
        ShellCommandPaletteOnRunCallbackType,
    },
    prelude::*,
    shells::{ShellCommandPalette, ShellPaletteCommand},
    str::String as AzString,
};

use azul_appkit::l10n::label;

use crate::{
    actions::{self, Action},
    app::{AppState, IndexState, Palette, PaletteKind},
    commands, ids, ui,
};

/// The palette over the window while one is showing.
#[must_use]
pub fn palette(app: &RefAny, st: &AppState) -> Option<Dom> {
    let p = st.palette.as_ref()?;
    Some(match p.kind {
        PaletteKind::Files => files(app, st, &p.query),
        PaletteKind::Commands => commands_palette(app, st, &p.query),
    })
}

/// Quick open: the folder's files for what was typed (the palette keeps
/// them all: they match by its own rule).
fn files(app: &RefAny, st: &AppState, query: &str) -> Dom {
    let commands: Vec<ShellPaletteCommand> = st
        .quick_files()
        .into_iter()
        .filter_map(|i| st.index.get(i))
        .map(|key| ShellPaletteCommand::create(key.as_str()).with_icon("description"))
        .collect();
    let placeholder = match st.index_state {
        IndexState::Done => "azcode-quick-open-placeholder",
        IndexState::None | IndexState::Running => "azcode-quick-open-listing",
    };
    Dom::create_div().with_id(ids::QUICK_OPEN).with_child(
        ShellCommandPalette::create()
            .with_commands(commands)
            .with_query(query)
            .with_placeholder(label(placeholder))
            .with_open(true)
            .with_on_query(app.clone(), on_query as ShellCommandPaletteOnQueryCallbackType)
            .with_on_run(app.clone(), on_run as ShellCommandPaletteOnRunCallbackType)
            .with_on_close(app.clone(), on_close as ButtonOnClickCallbackType)
            .dom(),
    )
}

/// The command palette: every command available now, "Category: Label"
/// with its keys.
fn commands_palette(app: &RefAny, st: &AppState, query: &str) -> Dom {
    let commands: Vec<ShellPaletteCommand> = actions::available(st)
        .into_iter()
        .map(|a| {
            ShellPaletteCommand::create(label(a.label()))
                .with_category(label(a.category()))
                .with_icon(a.icon())
                .with_shortcut(commands::keys(a.keys()))
        })
        .collect();
    Dom::create_div().with_id(ids::COMMAND_PALETTE).with_child(
        ShellCommandPalette::create()
            .with_commands(commands)
            .with_query(query)
            .with_placeholder(label("azcode-palette-placeholder"))
            .with_open(true)
            .with_on_query(app.clone(), on_query as ShellCommandPaletteOnQueryCallbackType)
            .with_on_run(app.clone(), on_run as ShellCommandPaletteOnRunCallbackType)
            .with_on_close(app.clone(), on_close as ButtonOnClickCallbackType)
            .dom(),
    )
}

/// What was typed. In quick open a leading `>` turns it into the command
/// palette (VSCode's way).
extern "C" fn on_query(mut data: RefAny, mut info: CallbackInfo, query: AzString) -> Update {
    let query = query.as_str().to_string();
    ui::with_state(&mut data, &mut info, |st, info, _| {
        let Some(p) = st.palette.as_mut() else {
            return;
        };
        if p.kind == PaletteKind::Files {
            if let Some(rest) = query.strip_prefix('>') {
                *p = Palette {
                    kind: PaletteKind::Commands,
                    query: rest.trim_start().to_string(),
                };
                commands::focus_soon(info, ids::COMMAND_PALETTE.as_str());
                return;
            }
        }
        p.query = query;
    })
}

/// A row picked (a click, or Enter on the first or a chosen row): the file
/// opens, or the command runs (`AZCODE_COMMAND <name>`).
extern "C" fn on_run(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    ui::with_state(&mut data, &mut info, |st, info, app| {
        let Some(kind) = st.palette.as_ref().map(|p| p.kind) else {
            return;
        };
        match kind {
            PaletteKind::Files => {
                let key = st
                    .quick_files()
                    .get(index)
                    .and_then(|&i| st.index.get(i))
                    .cloned();
                st.palette = None;
                if let Some(key) = key {
                    commands::open_file(st, info, app, &key);
                }
            }
            PaletteKind::Commands => {
                let action: Option<Action> = actions::available(st).get(index).copied();
                st.palette = None;
                if let Some(action) = action {
                    println!("AZCODE_COMMAND {}", action.name());
                    actions::run(st, info, app, action);
                }
            }
        }
    })
}

/// The palette closed (Escape, a click beside it).
extern "C" fn on_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, |st, info, _| {
        st.palette = None;
        if st.tabs.active().is_some() {
            commands::focus_soon(info, ids::EDITOR.as_str());
        }
    })
}
