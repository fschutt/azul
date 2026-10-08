//! File: the backstage. Info, New (the theme picker with live previews,
//! colour variant, fonts, slide size), Open (the decks under `show/`, the
//! sample deck), Save, Export (PDF, the slide as a picture, into
//! `show/exports/`), Close, Options (azul-appkit's settings page over the
//! whole window: the app theme and the mode, remembered; the data folder;
//! the shortcuts table) and About (the standard About box).

use std::collections::HashMap;

use azul::{
    callbacks::{
        BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType, CallbackInfo, RefAny,
        SegmentedOnChangeCallbackType, StandardDialogOnEventCallbackType, Update,
    },
    dom::Dom,
    shells::ShellEmptyState,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        AboutDialog, Backstage, BackstageNavItem, Button, ButtonType, Segmented, SegmentedState,
        StandardDialogEvent,
    },
};

use crate::{
    app::{command, AppState, BackstagePage, Command},
    commands::on_command,
    model::{Deck, FontScheme, PlaceholderRole, SlideSize},
    render::{self, RenderOptions},
    themes,
};

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn strings(items: &[&str]) -> StringVec {
    StringVec::from(items.iter().map(|i| AzString::from(*i)).collect::<Vec<_>>())
}

fn button(app: &RefAny, label: &str, cmd: Command) -> Dom {
    Button::create(s(label))
        .with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
        .dom()
}

fn primary(app: &RefAny, label: &str, cmd: Command) -> Dom {
    Button::create(s(label))
        .with_button_type(ButtonType::Primary)
        .with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
        .dom()
}

fn heading(text: &str) -> Dom {
    Dom::create_h2_with_text(text).with_css("margin: 0px 0px 16px 0px; font-size: 26px; font-weight: normal;")
}

fn line(text: &str) -> Dom {
    Dom::create_p_with_text(text).with_css("margin: 0px 0px 8px 0px; font-size: 13px;")
}

fn pane(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; padding: 24px 36px; overflow-y: auto;")
        .with_children(children)
}

/// What a segmented control in the backstage sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    Variant,
    Fonts,
    Size,
}

struct SegmentData {
    app: RefAny,
    segment: Segment,
}

extern "C" fn on_segment(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let (mut app, segment) = match data.downcast_ref::<SegmentData>() {
        Some(d) => (d.app.clone(), d.segment),
        None => return Update::DoNothing,
    };
    let i = state.selected_index;
    let cmd = match segment {
        Segment::Variant => Command::NewVariant(i),
        Segment::Fonts => Command::NewFonts(i),
        Segment::Size => Command::NewSize(if i == 1 { SlideSize::Standard } else { SlideSize::Wide }),
    };
    crate::commands::run(&mut app, cmd, &mut info)
}

fn segmented(app: &RefAny, segment: Segment, labels: &[&str], selected: usize) -> Dom {
    Segmented::create(strings(labels))
        .with_selected_index(selected)
        .with_on_change(
            RefAny::new(SegmentData {
                app: app.clone(),
                segment,
            }),
            on_segment as SegmentedOnChangeCallbackType,
        )
        .dom()
}

extern "C" fn on_nav(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let page = BackstagePage::NAV.get(index).copied().unwrap_or(BackstagePage::Info);
    let cmd = match page {
        BackstagePage::Close => Command::CloseDeck,
        page => Command::OpenBackstage(page),
    };
    crate::commands::run(&mut data, cmd, &mut info)
}

/// The About box's OK: back from the backstage.
extern "C" fn on_about_event(mut data: RefAny, mut info: CallbackInfo, _event: StandardDialogEvent) -> Update {
    crate::commands::run(&mut data, Command::CloseBackstage, &mut info)
}

/// A theme's card on File > New: a live title slide in the theme, its name.
fn theme_card(app: &RefAny, st: &AppState, index: usize, name: &str) -> Dom {
    let theme = themes::theme(index, st.new_variant, st.new_fonts);
    let mut deck = Deck::new("preview", name, theme, st.new_size);
    if let Some(body) = deck.slides[0]
        .elements
        .iter_mut()
        .find(|e| e.placeholder == Some(PlaceholderRole::Title))
        .and_then(|e| e.body_mut())
    {
        body.set_text(name);
    }
    let media = HashMap::new();
    let scale = 0.1;
    let picture = render::slide_dom(&deck, &deck.slides[0], &RenderOptions::still(scale, &media));
    // The theme's accents under its title slide (as PowerPoint's New shows
    // them): the grounds of the paper variants alone look alike.
    let colors = &deck.theme.colors;
    let mut accents = Dom::create_div().with_css("display: flex; flex-direction: row; height: 6px;");
    for c in [colors.accent, colors.accent2, colors.accent3, colors.title] {
        accents.add_child(Dom::create_div().with_css(format!("flex-grow: 1; background: {};", render::css_color(c))));
    }
    // The chosen theme's ring: PowerPoint's orange; under flora the stone (AzShow's red spin).
    let ring = if st.new_theme == index {
        "border: 3px solid #d24726; @theme(flora) { border: 3px solid system:accent; }"
    } else {
        "border: 3px solid transparent;"
    };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; align-items: stretch; margin: 0px 16px 16px 0px; {ring}"
        ))
        .with_child(picture)
        .with_child(accents)
        .with_child(button(app, name, Command::NewTheme(index)))
}

fn new_page(app: &RefAny, st: &AppState) -> Dom {
    let mut cards = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap; align-content: flex-start;");
    for (i, name) in themes::names().iter().enumerate() {
        cards.add_child(theme_card(app, st, i, name));
    }
    let fonts: Vec<String> = FontScheme::all().into_iter().map(|f| f.name).collect();
    let font_labels: Vec<&str> = fonts.iter().map(String::as_str).collect();
    let row = |label: &str, control: Dom| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin: 0px 0px 10px 0px;")
            .with_child(Dom::create_p_with_text(label).with_css("margin: 0px 12px 0px 0px; width: 90px; font-size: 13px;"))
            .with_child(control)
    };
    pane(vec![
        heading("New"),
        cards,
        row("Variant", segmented(app, Segment::Variant, &themes::VARIANTS, st.new_variant)),
        row("Fonts", segmented(app, Segment::Fonts, &font_labels, st.new_fonts)),
        row(
            "Slide size",
            segmented(
                app,
                Segment::Size,
                &["Widescreen (16:9)", "Standard (4:3)"],
                usize::from(st.new_size == SlideSize::Standard),
            ),
        ),
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin: 8px 0px 0px 0px;")
            .with_child(primary(app, "Create", Command::NewDeck))
            .with_child(Dom::create_div().with_css("width: 12px;"))
            .with_child(button(app, "Open the sample deck", Command::OpenSample)),
    ])
}

fn open_page(app: &RefAny, st: &AppState) -> Dom {
    let mut list = Dom::create_div().with_css("display: flex; flex-direction: column;");
    if st.decks.is_empty() {
        list.add_child(
            ShellEmptyState::create(s("No presentations yet"))
                .with_icon(s("slideshow"))
                .with_detail(s(&format!("Decks are saved under {}", st.data_root.join("show").display())))
                .with_action_label(s("Open the sample deck"))
                .with_on_action(command(app, Command::OpenSample), on_command as ButtonOnClickCallbackType)
                .dom(),
        );
    }
    for deck in &st.decks {
        list.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 6px 0px; \
                     border-bottom: 1px solid #c8c8c8; @theme(flora) { border-bottom: 1px solid \
                     system:separator; }",
                )
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
                        .with_child(Dom::create_p_with_text(deck.title.as_str()).with_css("margin: 0px; font-size: 15px;"))
                        .with_child(
                            Dom::create_p_with_text(format!("{} slides - show/{}", deck.slides, deck.id))
                                .with_css("margin: 0px; font-size: 11px;"),
                        ),
                )
                .with_child(button(app, "Open", Command::OpenDeck(deck.id.clone()))),
        );
    }
    pane(vec![
        heading("Open"),
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin: 0px 0px 12px 0px;")
            .with_child(button(app, "Refresh", Command::RefreshDecks))
            .with_child(Dom::create_div().with_css("width: 12px;"))
            .with_child(button(app, "Sample deck", Command::OpenSample)),
        list,
    ])
}

fn info_page(app: &RefAny, st: &AppState) -> Dom {
    let Some(ed) = st.editor.as_ref() else {
        return pane(vec![heading("Info"), line("No presentation is open."), button(app, "New", Command::OpenBackstage(BackstagePage::New))]);
    };
    let path = st.data_root.join("show").join(&ed.deck.id).join("deck.json");
    pane(vec![
        heading(&ed.deck.title),
        line(&format!("{} slides, {}", ed.deck.slides.len(), ed.deck.size.label())),
        line(&format!("Theme: {} ({})", ed.deck.theme.name, ed.deck.theme.fonts.name)),
        line(&format!("File: {}", path.display())),
        line(if ed.dirty { "Changed since the last save." } else { "Saved." }),
        Dom::create_div().with_css("height: 8px;"),
        primary(app, "Save", Command::Save),
    ])
}

fn export_page(app: &RefAny) -> Dom {
    pane(vec![
        heading("Export"),
        line("Create a PDF: every slide shown in the show, one per page, through azul's PDF path (opacity and filters are dropped). It is written to show/exports/ in the data folder."),
        primary(app, "Create PDF", Command::ExportPdf),
        Dom::create_div().with_css("height: 20px;"),
        line("Save the current slide as a PNG picture in show/exports/ (open the normal view first)."),
        button(app, "Save Slide as Picture", Command::ExportImages),
    ])
}

/// Options without a kit (with one, File > Options opens azul-appkit's settings page over the
/// whole window instead: commands.rs, `OpenBackstage(Options)`).
fn options_page() -> Dom {
    pane(vec![heading("Options"), line("The settings are not available.")])
}

/// About: the standard About box (DEDUP_OFFICE D12); OK goes back.
fn about_page(app: &RefAny) -> Dom {
    pane(vec![AboutDialog::create(s(crate::ABOUT.name), s(crate::ABOUT.version))
        .with_icon(s("slideshow"))
        .with_description(s(crate::ABOUT.summary))
        .with_copyright(s("Copyright 2026 Felix Schuett. MIT license."))
        .with_credit(s("azul"), s("MIT"))
        .with_on_event(app.clone(), on_about_event as StandardDialogOnEventCallbackType)
        .dom()])
}

/// The backstage for the app's state.
#[must_use]
pub fn backstage(app: &RefAny, st: &AppState) -> Dom {
    let content = match st.page {
        BackstagePage::Info | BackstagePage::Save => info_page(app, st),
        BackstagePage::New => new_page(app, st),
        BackstagePage::Open => open_page(app, st),
        BackstagePage::Export => export_page(app),
        BackstagePage::Close => info_page(app, st),
        BackstagePage::Options => options_page(),
        BackstagePage::About => about_page(app),
    };
    let nav: Vec<BackstageNavItem> = BackstagePage::NAV
        .iter()
        .map(|p| {
            let item = BackstageNavItem::create(s(p.label()));
            if *p == BackstagePage::Options {
                item.with_gap_before()
            } else {
                item
            }
        })
        .collect();
    Backstage::create(nav)
        .with_active_item(st.page.index())
        .with_on_nav_select(app.clone(), on_nav as BackstageOnNavSelectCallbackType)
        .with_on_back(command(app, Command::CloseBackstage), on_command as ButtonOnClickCallbackType)
        .with_content(content)
        .dom()
}
