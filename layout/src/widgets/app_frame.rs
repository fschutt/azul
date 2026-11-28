//! The frame every heavy app shares: where the left sidebar, the tab ribbon, the body, the
//! Inspector and the footer line stand - once, so no app lays them out again.
//!
//! ```text
//!  desktop                                          phone (the drawer open)
//! ┌────┬──────────────────────────────────────┐    ┌──────────────┬──────────────────────┐
//! │ ●●●│ tab ribbon: tabs and their band      │    │ ●            │ pills │  (pushed aside,
//! │    ├─────────────────────────┬────────────┤    │  sidebar     │ band  │   clipped by the
//! │ the│ body                    │ Inspector  │    │  as a drawer │ body  │   window's edge)
//! │ side│                        │ (right)    │    │              │ pane  │
//! │ bar │                        │            │    │              │ footer│
//! │    ├─────────────────────────┴────────────┤    └──────────────┴───────┘
//! │    │ footer: the one-line command bar     │
//! └────┴──────────────────────────────────────┘
//! ```
//!
//! - The LEFT sidebar is full height: it runs beside the tab ribbon and the footer, up to the top
//!   edge of the window. There is no title bar: the OS's window controls sit over the sidebar's
//!   top ([`crate::widgets::sidebar::Sidebar::with_window_controls`]).
//! - The TAB RIBBON (`header`) runs over the full width of everything right of the sidebar, the
//!   Inspector included. Its band is as tall for every tab ([`crate::widgets::tabs::TAB_BAND_PX`]).
//! - The BODY is the app's main view; the INSPECTOR is docked beside it on a desktop (right, below
//!   the ribbon, down to the footer) and under it on a phone (the bottom pane); never both.
//! - On a PHONE an expanded sidebar is a DRAWER that PUSHES the whole main view aside (the pills
//!   included); the view keeps its width and slides out past the window's right edge. A tap on
//!   what is left of it closes the drawer. It is never an overlay with a dim.
//! - The FOOTER is the app's one-line command bar, under the Inspector.
//! - The SHEET (a phone's quick actions) lies over the whole frame.
//!
//! The frame is a layout: every part is a `Dom` or a widget the app builds, the frame owns no
//! state and no callback of its own besides the one that closes a pushed-aside view.
//!
//! Key types: [`AppFrame`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, OptionDom},
    refany::{OptionRefAny, RefAny},
};
use azul_css::AzString;

use super::{
    inspector::{Inspector, OptionInspector},
    sidebar::{OptionSidebar, Sidebar, SidebarEvent, SidebarEventKind, SidebarOnEvent},
    sidebar_layout::{pushes_main, FormFactor, SidebarState},
};
use crate::callbacks::CallbackInfo;

/// The frame's root.
pub const FRAME_CLASS: &str = "__azul-native-app-frame";
/// Added to the root on a phone.
pub const FRAME_PHONE_CLASS: &str = "__azul-native-app-frame-phone";
/// The column right of the sidebar: ribbon, body, footer.
pub const FRAME_MAIN_CLASS: &str = "__azul-native-app-frame-main";
/// The slot of the tab ribbon.
pub const FRAME_HEADER_CLASS: &str = "__azul-native-app-frame-header";
/// The slot of the body (and the Inspector).
pub const FRAME_BODY_CLASS: &str = "__azul-native-app-frame-body";
/// The slot of the footer line.
pub const FRAME_FOOTER_CLASS: &str = "__azul-native-app-frame-footer";
/// The catcher over a pushed-aside main view: a tap closes the drawer.
pub const FRAME_CATCHER_CLASS: &str = "__azul-native-app-frame-catcher";

/// The frame: a sidebar, the tab ribbon, the body with its Inspector, the footer.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct AppFrame {
    /// The left sidebar; none for an app without one.
    pub sidebar: OptionSidebar,
    /// The tab ribbon: the tab row and the active tab's band.
    pub header: Dom,
    /// The app's main view.
    pub body: Dom,
    /// The Inspector docked to the body (its own `body` is replaced by `body`); none for an app
    /// without one.
    pub inspector: OptionInspector,
    /// The one-line command bar under everything; none for none.
    pub footer: OptionDom,
    /// The phone's quick-action sheet: the last child, over the whole frame.
    pub sheet: OptionDom,
    /// Desktop or phone: `Auto` asks the window; handed to the sidebar and the Inspector.
    pub form: FormFactor,
}

impl AppFrame {
    /// A frame of `body` under `header`, with no sidebar, Inspector, footer or sheet.
    #[must_use]
    pub const fn create(header: Dom, body: Dom) -> Self {
        Self {
            sidebar: OptionSidebar::None,
            header,
            body,
            inspector: OptionInspector::None,
            footer: OptionDom::None,
            sheet: OptionDom::None,
            form: FormFactor::Auto,
        }
    }

    /// The left sidebar.
    pub fn set_sidebar(&mut self, sidebar: Sidebar) {
        self.sidebar = OptionSidebar::Some(sidebar);
    }

    /// [`Self::set_sidebar`] for the builder chain.
    #[must_use]
    pub fn with_sidebar(mut self, sidebar: Sidebar) -> Self {
        self.set_sidebar(sidebar);
        self
    }

    /// The Inspector docked to the body.
    pub fn set_inspector(&mut self, inspector: Inspector) {
        self.inspector = OptionInspector::Some(inspector);
    }

    /// [`Self::set_inspector`] for the builder chain.
    #[must_use]
    pub fn with_inspector(mut self, inspector: Inspector) -> Self {
        self.set_inspector(inspector);
        self
    }

    /// The one-line command bar under everything.
    pub fn set_footer(&mut self, footer: Dom) {
        self.footer = OptionDom::Some(footer);
    }

    /// [`Self::set_footer`] for the builder chain.
    #[must_use]
    pub fn with_footer(mut self, footer: Dom) -> Self {
        self.set_footer(footer);
        self
    }

    /// The phone's quick-action sheet (`ActionSheet::dom`).
    pub fn set_sheet(&mut self, sheet: Dom) {
        self.sheet = OptionDom::Some(sheet);
    }

    /// [`Self::set_sheet`] for the builder chain.
    #[must_use]
    pub fn with_sheet(mut self, sheet: Dom) -> Self {
        self.set_sheet(sheet);
        self
    }

    /// Desktop or phone: `Auto` (the default) asks the window.
    pub const fn set_form(&mut self, form: FormFactor) {
        self.form = form;
    }

    /// [`Self::set_form`] for the builder chain.
    #[must_use]
    pub const fn with_form(mut self, form: FormFactor) -> Self {
        self.set_form(form);
        self
    }

    /// The frame's DOM: it fills its parent (`width: 100%`, `height: 100%`).
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl From<AppFrame> for Dom {
    fn from(f: AppFrame) -> Self {
        f.dom()
    }
}

fn classes(list: &[&'static str]) -> IdOrClassVec {
    IdOrClassVec::from_vec(
        list.iter()
            .map(|c| IdOrClass::Class(AzString::from_const_str(c)))
            .collect::<Vec<_>>(),
    )
}

fn styled(mut dom: Dom, css: &str) -> Dom {
    dom.root.set_css(css);
    dom
}

/// What the catcher carries: the app's callback and the event it reports (the sidebar asks to be
/// the rail).
struct CatcherRef {
    on_event: super::sidebar::OptionSidebarOnEvent,
}

extern "C" fn on_catcher_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(c) = data.downcast_ref::<CatcherRef>() else {
        return Update::DoNothing;
    };
    match c.on_event.as_ref() {
        Some(SidebarOnEvent { callback, refany }) => callback.invoke(
            refany.clone(),
            info,
            SidebarEvent {
                kind: SidebarEventKind::StateChanged,
                index: 0,
                tag: 0,
                expand: false,
                state: SidebarState::Rail,
            },
        ),
        None => Update::DoNothing,
    }
}

fn build(frame: AppFrame) -> Dom {
    let AppFrame {
        sidebar,
        header,
        body,
        inspector,
        footer,
        sheet,
        form,
    } = frame;
    let phone = form.is_phone();
    let mut sidebar = sidebar.into_option();
    if let Some(sb) = sidebar.as_mut() {
        sb.form = form;
    }
    let pushed = sidebar
        .as_ref()
        .is_some_and(|sb| pushes_main(sb.state, if phone { FormFactor::Phone } else { FormFactor::Desktop }));
    let catcher_on_event = sidebar.as_ref().map(|sb| sb.on_event.clone());

    // ---- the middle: the body, with the Inspector beside or under it ----
    let middle = match inspector.into_option() {
        Some(mut insp) => {
            insp.form = form;
            insp.body = body;
            insp.dom()
        }
        None => body,
    };
    let body_slot = styled(
        Dom::create_div().with_child(middle),
        "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-height: 0px; \
         min-width: 0px; overflow: hidden; position: relative;",
    )
    .with_ids_and_classes(classes(&[FRAME_BODY_CLASS]));

    let mut main_kids: Vec<Dom> = Vec::with_capacity(4);
    main_kids.push(
        styled(
            Dom::create_div().with_child(header),
            "display: flex; flex-direction: column; flex-shrink: 0; width: 100%; min-width: 0px;",
        )
        .with_ids_and_classes(classes(&[FRAME_HEADER_CLASS])),
    );
    main_kids.push(body_slot);
    if let Some(f) = footer.into_option() {
        main_kids.push(
            styled(
                Dom::create_div().with_child(f),
                "display: flex; flex-direction: column; flex-shrink: 0; width: 100%; min-width: 0px;",
            )
            .with_ids_and_classes(classes(&[FRAME_FOOTER_CLASS])),
        );
    }
    // a phone's open drawer pushed the view aside: what is left of it closes the drawer instead
    // of reaching the controls under the finger (transparent: no dim)
    if pushed {
        let mut catcher = styled(
            Dom::create_div(),
            "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; z-index: 40;",
        )
        .with_ids_and_classes(classes(&[FRAME_CATCHER_CLASS]));
        if let Some(on_event) = catcher_on_event {
            catcher = catcher.with_callbacks(
                alloc::vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    callback: CoreCallback {
                        cb: on_catcher_click as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: RefAny::new(CatcherRef { on_event }),
                }]
                .into(),
            );
        }
        main_kids.push(catcher);
    }
    let main_css = if pushed {
        // keeps the width of the frame and is pushed out of the window by the drawer
        "display: flex; flex-direction: column; flex-grow: 0; flex-shrink: 0; width: 100%; height: 100%; \
         min-width: 0px; position: relative; overflow: hidden;"
    } else {
        "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; flex-basis: 0px; height: 100%; \
         min-width: 0px; position: relative; overflow: hidden;"
    };
    let main = styled(Dom::create_div().with_children(DomVec::from_vec(main_kids)), main_css)
        .with_ids_and_classes(classes(&[FRAME_MAIN_CLASS]));

    let mut root_kids: Vec<Dom> = Vec::with_capacity(3);
    if let Some(sb) = sidebar {
        root_kids.push(sb.dom());
    }
    root_kids.push(main);
    if let Some(s) = sheet.into_option() {
        root_kids.push(s);
    }
    let class_list: &[&'static str] = if phone {
        &[FRAME_CLASS, FRAME_PHONE_CLASS]
    } else {
        &[FRAME_CLASS]
    };
    styled(
        Dom::create_div().with_children(DomVec::from_vec(root_kids)),
        "display: flex; flex-direction: row; width: 100%; height: 100%; min-width: 0px; min-height: 0px; \
         overflow: hidden; position: relative; box-sizing: border-box;",
    )
    .with_ids_and_classes(classes(class_list))
}

#[cfg(test)]
#[path = "app_frame_test.rs"]
mod app_frame_test;

