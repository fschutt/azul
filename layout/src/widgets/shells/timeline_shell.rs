//! S3 - Timeline editor: the video editor, the MIDI editor, the screen
//! recorder's trim, the voice memo editor (04-app-shells.md, S3).
//!
//! ```text
//! ┌ menu bar ──────────────────────────────────────────────────────────┐
//! ├ MEDIA ───────┬─ SOURCE ─────────┬─ PROGRAM ──────────┬─ INSPECTOR ─┤
//! │ [▦][▦][▦]    │   video frame    │    video frame     │ Transform   │
//! │              │ ⏮ ◀ ▶ ▶| 00:12   │ ⏮ ◀ ▶ ▶| 01:03     │ Opacity ●── │
//! ├──────────────┴──────────────────┴────────────────────┴─────────────┤
//! │ 00:00 ─────────── timeline (tracks, clips, playhead) ──── │ L▮▮ R▮ │
//! │ V1 [ clip1 ][ clip2 ]                                     │ meters │
//! └───────────────────────────────────────────────────────────┴────────┘
//! ```
//!
//! An [`OfficeShell`] with four panes in the row - the media bin
//! (`shell-media`), the source monitor (`shell-source`), the program
//! monitor (`shell-program`, the `<main>`) and the inspector
//! (`shell-inspector`, an `<aside>`) - and the timeline as the bottom pane
//! (`shell-timeline`), the audio meters at its right edge. The menu bar is
//! the title row.
//!
//! Key types: [`TimelineShell`].

use azul_core::{
    dom::{Dom, DomVec, OptionDom},
    refany::RefAny,
};
use azul_css::AzString;

use super::{
    office_shell::{
        OfficeShell, OptionShellOnPaneFocus, OptionShellOnPaneResize, ShellOnPaneFocus,
        ShellOnPaneFocusCallback, ShellOnPaneResize, ShellOnPaneResizeCallback, ShellPane,
        ShellPaneKind,
    },
    part, GROW_COLUMN_BASE, GROW_ROW_BASE, RAIL_BASE,
};
use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// The media bin's DOM id.
pub const MEDIA_ID: &str = "shell-media";
/// The source monitor's DOM id.
pub const SOURCE_ID: &str = "shell-source";
/// The program monitor's DOM id.
pub const PROGRAM_ID: &str = "shell-program";
/// The inspector's DOM id.
pub const INSPECTOR_ID: &str = "shell-inspector";
/// The timeline pane's DOM id.
pub const TIMELINE_ID: &str = "shell-timeline";
/// The class of the row that holds the timeline and the meters.
pub const BOTTOM_CLASS: &str = "__azul-native-timeline-shell-bottom";
/// The timeline host's class.
pub const TRACKS_CLASS: &str = "__azul-native-timeline-shell-tracks";
/// The meters host's class.
pub const METERS_CLASS: &str = "__azul-native-timeline-shell-meters";

/// S3: media | source | program | inspector over the timeline.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct TimelineShell {
    /// The in-window menu bar (or nothing under a native one).
    pub menu_bar: OptionDom,
    /// The media bin.
    pub media: Dom,
    /// The source monitor.
    pub source: Dom,
    /// The program monitor.
    pub program: Dom,
    /// The inspector.
    pub inspector: Dom,
    /// The timeline.
    pub timeline: Dom,
    /// The audio meters at the timeline's right edge.
    pub meters: OptionDom,
    /// F6 moved the focus to a pane.
    pub on_pane_focus: OptionShellOnPaneFocus,
    /// A splitter moved.
    pub on_pane_resize: OptionShellOnPaneResize,
    /// The timeline's share of the height (default 0.45).
    pub timeline_ratio: f32,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl TimelineShell {
    /// A shell of the four monitors' panes and the timeline.
    #[must_use]
    pub fn create(media: Dom, source: Dom, program: Dom, inspector: Dom, timeline: Dom) -> Self {
        Self {
            menu_bar: OptionDom::None,
            media,
            source,
            program,
            inspector,
            timeline,
            meters: OptionDom::None,
            on_pane_focus: None.into(),
            on_pane_resize: None.into(),
            timeline_ratio: 0.45,
            theme: OptionUiTheme::None,
        }
    }

    /// The in-window menu bar.
    pub fn set_menu_bar(&mut self, menu_bar: Dom) {
        self.menu_bar = OptionDom::Some(menu_bar);
    }

    /// [`Self::set_menu_bar`] for the builder chain.
    #[must_use]
    pub fn with_menu_bar(mut self, menu_bar: Dom) -> Self {
        self.set_menu_bar(menu_bar);
        self
    }

    /// The audio meters.
    pub fn set_meters(&mut self, meters: Dom) {
        self.meters = OptionDom::Some(meters);
    }

    /// [`Self::set_meters`] for the builder chain.
    #[must_use]
    pub fn with_meters(mut self, meters: Dom) -> Self {
        self.set_meters(meters);
        self
    }

    /// The timeline's share of the height.
    pub const fn set_timeline_ratio(&mut self, ratio: f32) {
        self.timeline_ratio = ratio;
    }

    /// [`Self::set_timeline_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_timeline_ratio(mut self, ratio: f32) -> Self {
        self.set_timeline_ratio(ratio);
        self
    }

    /// F6 moved the focus to a pane.
    pub fn set_on_pane_focus<C: Into<ShellOnPaneFocusCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_pane_focus = Some(ShellOnPaneFocus {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_pane_focus`] for the builder chain.
    #[must_use]
    pub fn with_on_pane_focus<C: Into<ShellOnPaneFocusCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_pane_focus(data, callback);
        self
    }

    /// A splitter moved.
    pub fn set_on_pane_resize<C: Into<ShellOnPaneResizeCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_pane_resize = Some(ShellOnPaneResize {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_pane_resize`] for the builder chain.
    #[must_use]
    pub fn with_on_pane_resize<C: Into<ShellOnPaneResizeCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_pane_resize(data, callback);
        self
    }

    /// Pin the widget theme; unset, the shell follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty shell and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(
            Dom::create_div(),
            Dom::create_div(),
            Dom::create_div(),
            Dom::create_div(),
            Dom::create_div(),
        );
        core::mem::swap(&mut s, self);
        s
    }

    /// The [`OfficeShell`] this shell is: the menu bar as the title row,
    /// the four panes in the row, the timeline (with the meters) under it.
    #[must_use]
    pub fn office_shell(self) -> OfficeShell {
        let Self {
            menu_bar,
            media,
            source,
            program,
            inspector,
            timeline,
            meters,
            on_pane_focus,
            on_pane_resize,
            timeline_ratio,
            theme,
        } = self;
        let mut bottom_row: alloc::vec::Vec<Dom> = alloc::vec![Dom::create_div()
            .with_class(AzString::from_const_str(TRACKS_CLASS))
            .with_css_props(part(GROW_COLUMN_BASE, &[]))
            .with_child(timeline)];
        if let Some(m) = meters.into_option() {
            bottom_row.push(
                Dom::create_div()
                    .with_class(AzString::from_const_str(METERS_CLASS))
                    .with_css_props(part(RAIL_BASE, &[]))
                    .with_child(m),
            );
        }
        let bottom = Dom::create_div()
            .with_class(AzString::from_const_str(BOTTOM_CLASS))
            .with_css_props(part(GROW_ROW_BASE, &[]))
            .with_children(DomVec::from_vec(bottom_row));
        OfficeShell {
            title_row: menu_bar,
            on_pane_focus,
            on_pane_resize,
            theme,
            ..OfficeShell::create()
        }
        .with_pane(
            ShellPane::create(AzString::from_const_str(MEDIA_ID), media)
                .with_label(AzString::from_const_str("Media"))
                .with_ratio(0.22),
        )
        .with_pane(
            ShellPane::create(AzString::from_const_str(SOURCE_ID), source)
                .with_label(AzString::from_const_str("Source"))
                .with_ratio(0.35),
        )
        .with_pane(
            ShellPane::create(AzString::from_const_str(PROGRAM_ID), program)
                .with_kind(ShellPaneKind::Main)
                .with_label(AzString::from_const_str("Program"))
                .with_ratio(0.6),
        )
        .with_pane(
            ShellPane::create(AzString::from_const_str(INSPECTOR_ID), inspector)
                .with_kind(ShellPaneKind::Side)
                .with_label(AzString::from_const_str("Inspector")),
        )
        .with_bottom(
            ShellPane::create(AzString::from_const_str(TIMELINE_ID), bottom)
                .with_label(AzString::from_const_str("Timeline"))
                .with_ratio(timeline_ratio),
        )
    }

    /// The shell's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        self.office_shell().dom()
    }
}

impl From<TimelineShell> for Dom {
    fn from(s: TimelineShell) -> Self {
        s.dom()
    }
}

#[cfg(test)]
mod timeline_shell_tests {
    use azul_core::dom::IdOrClass;

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn full() -> TimelineShell {
        TimelineShell::create(slot(), slot(), slot(), slot(), slot())
            .with_menu_bar(slot())
            .with_meters(slot())
    }

    #[test]
    fn s3_is_four_panes_over_the_timeline_with_the_meters_at_its_edge() {
        let dom = full().with_theme(UiTheme::Flat).dom();
        let ids: Vec<String> = tc::nodes(&dom)
            .into_iter()
            .filter_map(|(_, n)| {
                n.root.get_ids_and_classes().as_ref().iter().find_map(|c| match c {
                    IdOrClass::Id(s) => Some(s.as_str().to_string()),
                    IdOrClass::Class(_) => None,
                })
            })
            .collect();
        assert_eq!(
            ids,
            vec!["shell-title", MEDIA_ID, SOURCE_ID, PROGRAM_ID, INSPECTOR_ID, TIMELINE_ID]
        );
        // Three horizontal splits for four panes, one vertical for the timeline.
        assert_eq!(tc::find_all(&dom, "__azul-native-split-pane").len(), 4);
        let bottom = tc::find(&dom, BOTTOM_CLASS).expect("the bottom row");
        assert_eq!(bottom.children.as_ref().len(), 2, "the tracks, the meters");
        assert!(tc::has_class(&bottom.children.as_ref()[1], METERS_CLASS));
        assert_eq!(full().office_shell().cycle_ids().len(), 5);
    }

    #[test]
    fn s3_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "timeline_shell",
            || full().dom(),
            |t: UiTheme| full().with_theme(t).dom(),
        );
    }
}
