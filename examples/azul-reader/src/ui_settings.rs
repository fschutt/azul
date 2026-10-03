//! The reader's own section of the settings page ("Reading", first on azul-appkit's page):
//! the text size, the line spacing, the margins, the font, the paper, the pages side by side
//! and the alignment - the same commands the ribbon's VIEW tab runs.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, SegmentedOnChangeCallbackType, Update,
    },
    dom::Dom,
    str::String as AzString,
    vec::StringVec,
    widgets::{Button, Segmented, SegmentedState},
};
use azul_appkit::ui::{self as kit, AppSection};

use crate::{
    app::{command, AppState, Command},
    commands::{self, on_command},
    ids,
    settings::{FontChoice, PageLayout, Paper},
};

fn labels(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
}

/// `- value +`: two buttons around the value.
fn stepper(app: &RefAny, value: String, less: Command, more: Command) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(
            Button::create("")
                .with_icon("remove")
                .with_on_click(command(app, less), on_command as ButtonOnClickCallbackType)
                .dom(),
        )
        .with_child(
            Dom::create_div()
                .with_css("min-width: 64px; text-align: center; font-size: 13px;")
                .with_child(Dom::create_span_with_text(value)),
        )
        .with_child(
            Button::create("")
                .with_icon("add")
                .with_on_click(command(app, more), on_command as ButtonOnClickCallbackType)
                .dom(),
        )
}

/// The "Reading" section.
#[must_use]
pub fn section(app: &RefAny, st: &AppState) -> AppSection {
    let s = &st.settings;
    let paper: Vec<&str> = Paper::ALL.iter().map(|p| p.label()).collect();
    let fonts: Vec<&str> = FontChoice::ALL.iter().map(|f| f.label()).collect();
    let layouts: Vec<&str> = PageLayout::ALL.iter().map(|l| l.label()).collect();
    let content = Dom::create_div()
        .with_id(ids::READING_SETTINGS)
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "Text size",
            stepper(
                app,
                format!("{} px", s.font_px),
                Command::FontSize(-1),
                Command::FontSize(1),
            ),
        ))
        .with_child(kit::row(
            "Line spacing",
            stepper(
                app,
                s.line_height(),
                Command::LineSpacing(-1),
                Command::LineSpacing(1),
            ),
        ))
        .with_child(kit::row(
            "Margins",
            stepper(
                app,
                format!("{} px", s.margin_px),
                Command::Margins(-8),
                Command::Margins(8),
            ),
        ))
        .with_child(kit::row(
            "Font",
            Segmented::create(labels(&fonts))
                .with_selected_index(s.font.index())
                .with_on_change(app.clone(), on_font as SegmentedOnChangeCallbackType)
                .dom(),
        ))
        .with_child(kit::row(
            "Paper",
            Segmented::create(labels(&paper))
                .with_selected_index(s.paper.index())
                .with_on_change(app.clone(), on_paper as SegmentedOnChangeCallbackType)
                .dom(),
        ))
        .with_child(kit::row(
            "Pages",
            Segmented::create(labels(&layouts))
                .with_selected_index(s.layout.index())
                .with_on_change(app.clone(), on_layout as SegmentedOnChangeCallbackType)
                .dom(),
        ))
        .with_child(kit::row(
            "Alignment",
            Segmented::create(labels(&["Justified", "Left"]))
                .with_selected_index(usize::from(!s.justify))
                .with_on_change(app.clone(), on_justify as SegmentedOnChangeCallbackType)
                .dom(),
        ))
        .with_child(kit::note(
            "Auto paper follows the light or dark mode. Two pages show side by side when the \
             window is wide enough (Auto) or always (Two pages).",
        ));
    AppSection {
        category: 0,
        title: "Reading".to_string(),
        content,
    }
}

extern "C" fn on_font(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let font = FontChoice::ALL[state.selected_index.min(FontChoice::ALL.len() - 1)];
    commands::run(&mut data, Command::Font(font), &mut info)
}

extern "C" fn on_paper(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let paper = Paper::ALL[state.selected_index.min(Paper::ALL.len() - 1)];
    commands::run(&mut data, Command::Paper(paper), &mut info)
}

extern "C" fn on_layout(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let layout = PageLayout::ALL[state.selected_index.min(PageLayout::ALL.len() - 1)];
    commands::run(&mut data, Command::Layout(layout), &mut info)
}

extern "C" fn on_justify(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    commands::run(
        &mut data,
        Command::Justify(state.selected_index == 0),
        &mut info,
    )
}
