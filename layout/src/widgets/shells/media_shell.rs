//! S7 - Media player: Music / Podcasts, the video player (04-app-shells.md,
//! S7).
//!
//! ```text
//! ┌ title row (optional) ────────────────────────────────────────────┐
//! ├──────────────┬───────────────────────────────────────────────────┤
//! │ sidebar      │ content (albums grid, songs table, the video)     │
//! │ (Home,       │                                                   │
//! │  Library)    │                                                   │
//! ├──────────────┴───────────────────────────────────────────────────┤
//! │ [▦] So What - Miles Davis  ⏮ ▶ ⏭  1:12 ━━●──── 9:22  🔊  now playing │
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! An [`OfficeShell`] of a sidebar (`shell-sidebar`, a `<nav>`) beside the
//! content (`shell-content`, the `<main>`), with the NOW-PLAYING bar - the
//! media controls and the seek bar - as the footer, full width under both,
//! always there (music.md). The video player variant hands in its
//! full-bleed video as the content and no sidebar.
//!
//! Key types: [`MediaShell`].

use azul_core::{
    dom::{Dom, OptionDom},
    refany::RefAny,
};
use azul_css::AzString;

use super::office_shell::{
    OfficeShell, OptionShellOnPaneFocus, OptionShellOnPaneResize, ShellOnPaneFocus,
    ShellOnPaneFocusCallback, ShellOnPaneResize, ShellOnPaneResizeCallback, ShellPane,
    ShellPaneKind,
};
use crate::widgets::themes::{OptionUiTheme, UiTheme};

/// The sidebar's DOM id.
pub const SIDEBAR_ID: &str = "shell-sidebar";
/// The content pane's DOM id.
pub const CONTENT_ID: &str = "shell-content";
/// The now-playing bar's DOM id (the shell's footer row).
pub const NOW_PLAYING_ID: &str = "shell-status";

/// S7: sidebar | content, the now-playing bar under both.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MediaShell {
    /// The app-drawn title row.
    pub title_row: OptionDom,
    /// The sidebar (Home, Search, the library).
    pub sidebar: OptionDom,
    /// The content.
    pub content: Dom,
    /// The now-playing bar: media controls and the seek bar.
    pub now_playing: Dom,
    /// F6 moved the focus to a pane.
    pub on_pane_focus: OptionShellOnPaneFocus,
    /// A splitter moved.
    pub on_pane_resize: OptionShellOnPaneResize,
    /// The sidebar's share of the width (default 0.22).
    pub sidebar_ratio: f32,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl MediaShell {
    /// A shell of the sidebar, the content and the now-playing bar.
    #[must_use]
    pub fn create(sidebar: Dom, content: Dom, now_playing: Dom) -> Self {
        Self {
            title_row: OptionDom::None,
            sidebar: OptionDom::Some(sidebar),
            content,
            now_playing,
            on_pane_focus: None.into(),
            on_pane_resize: None.into(),
            sidebar_ratio: 0.22,
            theme: OptionUiTheme::None,
        }
    }

    /// The video player variant: the content full-bleed, no sidebar.
    #[must_use]
    pub fn create_player(content: Dom, now_playing: Dom) -> Self {
        let mut s = Self::create(Dom::create_div(), content, now_playing);
        s.sidebar = OptionDom::None;
        s
    }

    /// The app-drawn title row.
    pub fn set_title_row(&mut self, title_row: Dom) {
        self.title_row = OptionDom::Some(title_row);
    }

    /// [`Self::set_title_row`] for the builder chain.
    #[must_use]
    pub fn with_title_row(mut self, title_row: Dom) -> Self {
        self.set_title_row(title_row);
        self
    }

    /// The sidebar's share of the width.
    pub const fn set_sidebar_ratio(&mut self, ratio: f32) {
        self.sidebar_ratio = ratio;
    }

    /// [`Self::set_sidebar_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_sidebar_ratio(mut self, ratio: f32) -> Self {
        self.set_sidebar_ratio(ratio);
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
        let mut s = Self::create(Dom::create_div(), Dom::create_div(), Dom::create_div());
        core::mem::swap(&mut s, self);
        s
    }

    /// The [`OfficeShell`] this shell is: sidebar | content, the
    /// now-playing bar as the footer.
    #[must_use]
    pub fn office_shell(self) -> OfficeShell {
        let Self {
            title_row,
            sidebar,
            content,
            now_playing,
            on_pane_focus,
            on_pane_resize,
            sidebar_ratio,
            theme,
        } = self;
        let mut shell = OfficeShell {
            title_row,
            status_bar: OptionDom::Some(now_playing),
            on_pane_focus,
            on_pane_resize,
            theme,
            ..OfficeShell::create()
        };
        if let Some(s) = sidebar.into_option() {
            shell.add_pane(
                ShellPane::create(AzString::from_const_str(SIDEBAR_ID), s)
                    .with_kind(ShellPaneKind::Navigation)
                    .with_label(AzString::from_const_str("Library"))
                    .with_ratio(sidebar_ratio),
            );
        }
        shell.add_pane(
            ShellPane::create(AzString::from_const_str(CONTENT_ID), content)
                .with_kind(ShellPaneKind::Main)
                .with_label(AzString::from_const_str("Content")),
        );
        shell
    }

    /// The shell's DOM.
    #[must_use]
    pub fn dom(self) -> Dom {
        self.office_shell().dom()
    }
}

impl From<MediaShell> for Dom {
    fn from(s: MediaShell) -> Self {
        s.dom()
    }
}

#[cfg(test)]
mod media_shell_tests {
    use azul_core::dom::{IdOrClass, NodeType};

    use super::*;
    use crate::widgets::{
        shells::fixtures::slot,
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn ids(dom: &Dom) -> Vec<(String, NodeType)> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| {
                n.root.get_ids_and_classes().as_ref().iter().find_map(|c| match c {
                    IdOrClass::Id(s) => Some((s.as_str().to_string(), n.root.get_node_type().clone())),
                    IdOrClass::Class(_) => None,
                })
            })
            .collect()
    }

    #[test]
    fn s7_is_a_sidebar_beside_the_content_with_the_now_playing_bar_as_the_footer() {
        let dom = MediaShell::create(slot(), slot(), slot())
            .with_title_row(slot())
            .with_theme(UiTheme::Flat)
            .dom();
        let found = ids(&dom);
        let order: Vec<&str> = found.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(order, vec!["shell-title", SIDEBAR_ID, CONTENT_ID, NOW_PLAYING_ID]);
        assert!(matches!(found[3].1, NodeType::Footer), "the bar is the window's footer");
        let player = MediaShell::create_player(slot(), slot()).with_theme(UiTheme::Flat).dom();
        let order: Vec<String> = ids(&player).into_iter().map(|(id, _)| id).collect();
        assert_eq!(order, vec![CONTENT_ID, NOW_PLAYING_ID]);
    }

    #[test]
    fn s7_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "media_shell",
            || MediaShell::create(slot(), slot(), slot()).dom(),
            |t: UiTheme| MediaShell::create(slot(), slot(), slot()).with_theme(t).dom(),
        );
    }
}
