//! The frame every heavy app shares: where the left sidebar, the tab ribbon, the body, the
//! Preview and the footer line stand - once, so no app lays them out again.
//!
//! ```text
//!  desktop                                  phone, the rail      phone, the drawer open
//! ┌────┬─────────────────────────────┐    ┌──────────────────┐    ┌─────────┬──────────────┐
//! │ ●●●│ ^ tab ribbon: tabs, band    │    │ ^ pills (full)   │    │ sidebar │ pills │ (pushed
//! │    ├────────────────┬────────────┤    ├─┬────────────────┤    │ as a    │ band  │  aside,
//! │ the│ body           │ Preview    │    │r│ band, then the │    │ drawer  │ body  │  clipped)
//! │side│                │ (right,    │    │a│ body           │    │ (full   │ pane  │
//! │bar │                │  optional) │    │i├────────────────┤    │ height) │ footer│
//! │    ├────────────────┴────────────┤    │l│ Preview (opt.) │    └─────────┴───────┘
//! │    │ footer: the command bar     │    │ │ footer         │
//! └────┴─────────────────────────────┘    └─┴────────────────┘
//! ```
//!
//! - The LEFT sidebar is full height on a desktop: it runs beside the tab ribbon and the footer,
//!   up to the top edge of the window. There is no title bar: the OS's window controls sit over
//!   the sidebar's top ([`crate::widgets::sidebar::Sidebar::with_window_controls`]) and overhang
//!   the 18 px rail, the tab row starts after them.
//! - The TAB RIBBON (`header`) runs over the full width of everything right of the sidebar, the
//!   Preview included. Its band is as tall for every tab ([`crate::widgets::tabs::TAB_BAND_PX`]).
//! - On a PHONE the tab row is the full width and the 18 px RAIL starts BELOW it, beside the tab
//!   content, the body, the Preview and the footer (they keep [`AppFrame::rail_inset`] free at the
//!   left; the app hands the same inset to the ribbon's content). The chevron of the tab row sits
//!   above the rail.
//! - The BODY is the app's main view; the PREVIEW (opt-in: an app that adds none has no dock, no
//!   grip, no bottom pane) is docked beside it on a desktop (right, below the ribbon, down to the
//!   footer) and under it on a phone (the bottom pane); never both.
//! - On a PHONE an expanded sidebar is a DRAWER, full height, that PUSHES the whole main view
//!   aside (the pills included); the view keeps its width and slides out past the window's right
//!   edge. A tap on what is left of it closes the drawer. It is never an overlay with a dim.
//! - The FOOTER is the app's one-line command bar, under the Preview.
//! - The SHEET (a phone's quick actions) lies over the whole frame.
//!
//! The frame is a layout: every part is a `Dom` or a widget the app builds, the frame owns no
//! state and no callback of its own besides the one that closes a pushed-aside view.
//!
//! # Wiring an app (the example is `azul_appkit::frame::example`, examples/azul-appkit)
//!
//! 1. The app keeps ONE model (the sidebar's state, the chosen row, the Preview's size and
//!    whether it shows, what the location's field holds) and builds the whole frame from it in
//!    its layout callback; the widgets own nothing.
//! 2. Every widget reports through ONE callback (`SidebarOnEvent`, `PreviewOnEvent`,
//!    `LocationOnEvent`, `ActionSheetOnEvent`): the app changes its model and returns
//!    `Update::RefreshDom`. The toggle, the 16 px handle, a swipe from the rail and a tap on a
//!    pushed-aside view all arrive as `SidebarEventKind::StateChanged` with the state asked for.
//! 3. `sidebar_in_window` (appkit) hands the sidebar the window's width and the room of the OS's
//!    window controls; the tab row keeps `Sidebar::header_left_pad()` clear at its left. The form
//!    factor is `FormFactor::Auto` everywhere: a phone is a touch platform with a window under
//!    600 dp, anything else is the desktop's layout.
//! 4. On a phone the ribbon's content keeps [`AppFrame::rail_inset`] clear at its left (the rail
//!    stands beside it); `Ribbon::with_content_inset` says so.
//! 5. The first tab is the location (`RibbonTab::with_location`); the Preview is opt-in: no
//!    `with_preview`, no dock, no grip and no bottom pane.
//! 6. SAFE AREA: the engine insets the window's root by the shell's insets (status bar, navigation
//!    bar, camera housing) unless the app sets `WindowFlags::extend_into_safe_area`: the frame
//!    then needs nothing. An app that extends into it (AzWriter puts its tab row into the status
//!    bar's band) pads the frame itself from `get_safe_area_insets()` and says where the tab row
//!    ends with [`AppFrame::with_rail_top`].
//! 7. TOUCH SIZES: a row, a slot of the rail and a button of the location block are 48 px tall on
//!    a touch platform (Android's 48 dp, iOS's 44 pt); the rail's WIDTH is the one deliberate
//!    exception (18 px drawn, 36 px hit): the user's design.
//!
//! Key types: [`AppFrame`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, OptionDom},
    refany::{OptionRefAny, RefAny},
};
use azul_css::AzString;

use super::{
    preview::{Preview, OptionPreview},
    sidebar::{OptionSidebar, OptionSidebarOnEvent, Sidebar, SidebarEvent, SidebarEventKind, SidebarOnEvent},
    sidebar_layout::{pushes_main, sidebar_edge, Edge, FormFactor, SidebarState, BUILT_FOR},
    tab_strip::TOUCH_ROW,
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
/// The slot of the body (and the Preview).
pub const FRAME_BODY_CLASS: &str = "__azul-native-app-frame-body";
/// The slot of the footer line.
pub const FRAME_FOOTER_CLASS: &str = "__azul-native-app-frame-footer";
/// The catcher over a pushed-aside main view: a tap closes the drawer.
pub const FRAME_CATCHER_CLASS: &str = "__azul-native-app-frame-catcher";
/// The slot of a phone's rail: below the tab row, beside everything else.
pub const FRAME_RAIL_CLASS: &str = "__azul-native-app-frame-rail";

/// The frame: a sidebar, the tab ribbon, the body with its Preview, the footer.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct AppFrame {
    /// The left sidebar; none for an app without one.
    pub sidebar: OptionSidebar,
    /// The tab ribbon: the tab row and the active tab's band.
    pub header: Dom,
    /// The app's main view.
    pub body: Dom,
    /// The Preview docked to the body (its own `body` is replaced by `body`); none for an app
    /// without one.
    pub preview: OptionPreview,
    /// The one-line command bar under everything; none for none.
    pub footer: OptionDom,
    /// The phone's quick-action sheet: the last child, over the whole frame.
    pub sheet: OptionDom,
    /// The DOM id of the frame's root (`#id` in a selector, an e2e script finds it by it); empty
    /// for none.
    pub dom_id: AzString,
    /// Desktop or phone: `Auto` asks the window; handed to the sidebar and the Preview.
    pub form: FormFactor,
    /// Where a phone's rail starts, px from the frame's top: below the tab row. 0 is the touch
    /// tab row's height (48 px); an app whose tab row stands in a taller status band says so.
    pub rail_top: f32,
}

impl AppFrame {
    /// A frame of `body` under `header`, with no sidebar, Preview, footer or sheet.
    #[must_use]
    pub const fn create(header: Dom, body: Dom) -> Self {
        Self {
            sidebar: OptionSidebar::None,
            header,
            body,
            preview: OptionPreview::None,
            footer: OptionDom::None,
            sheet: OptionDom::None,
            dom_id: AzString::from_const_str(""),
            form: FormFactor::Auto,
            rail_top: 0.0,
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

    /// The Preview docked to the body.
    pub fn set_preview(&mut self, preview: Preview) {
        self.preview = OptionPreview::Some(preview);
    }

    /// [`Self::set_preview`] for the builder chain.
    #[must_use]
    pub fn with_preview(mut self, preview: Preview) -> Self {
        self.set_preview(preview);
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

    /// The DOM id of the frame's root, so that a selector (`#mail-frame`) names it.
    pub fn set_dom_id(&mut self, dom_id: AzString) {
        self.dom_id = dom_id;
    }

    /// [`Self::set_dom_id`] for the builder chain.
    #[must_use]
    pub fn with_dom_id(mut self, dom_id: AzString) -> Self {
        self.set_dom_id(dom_id);
        self
    }

    /// Where a phone's rail starts, px from the top of the frame (0: below a 48 px tab row).
    pub const fn set_rail_top(&mut self, rail_top: f32) {
        self.rail_top = rail_top;
    }

    /// [`Self::set_rail_top`] for the builder chain.
    #[must_use]
    pub const fn with_rail_top(mut self, rail_top: f32) -> Self {
        self.set_rail_top(rail_top);
        self
    }

    /// How far the body, the Preview and the footer keep from the left edge in this frame: a
    /// phone's rail (18 px) or handle (16 px) stands beside them below the tab row; 0 on a
    /// desktop (the sidebar is a column of the frame there), with the drawer open (it pushes
    /// everything aside) and with no sidebar. What the app hands the ribbon's content as its
    /// inset, so that the tab content clears the rail as well.
    #[must_use]
    pub fn rail_inset(&self) -> f32 {
        self.sidebar.as_ref().map_or(0.0, |sb| {
            let mut sb = sb.clone();
            sb.form = self.form;
            sb.phone_content_inset()
        })
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
    on_event: OptionSidebarOnEvent,
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
        preview,
        footer,
        sheet,
        dom_id,
        form,
        rail_top,
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
    // the edge the sidebar stands at (the one place that decides it, `sidebar_edge`)
    let edge = sidebar_edge(BUILT_FOR);
    // a phone's closed sidebar (the rail, the handle) stands below the tab row, beside the rest
    let inset = sidebar.as_ref().map_or(0.0, Sidebar::phone_content_inset);
    let inset_css = if inset > 0.0 { format!(" {}: {inset}px;", edge.margin_outer()) } else { String::new() };

    // ---- the middle: the body, with the Preview beside or under it ----
    let middle = match preview.into_option() {
        Some(mut insp) => {
            insp.form = form;
            insp.body = body;
            insp.dom()
        }
        None => body,
    };
    let body_slot = styled(
        Dom::create_div().with_child(middle),
        &format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-height: 0px; \
             min-width: 0px; overflow: hidden; position: relative;{inset_css}"
        ),
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
                &format!(
                    "display: flex; flex-direction: column; flex-shrink: 0; width: {}; min-width: 0px;{inset_css}",
                    if inset > 0.0 { format!("calc(100% - {inset}px)") } else { String::from("100%") }
                ),
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
        // with no callback of the app to tell there is nothing to ask: the catcher only keeps the
        // pushed view from taking the tap
        if let Some(on_event) = catcher_on_event.filter(OptionSidebarOnEvent::is_some) {
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

    // the sidebar's column in the row of the frame: first at the left edge, last at the right one
    let mut root_kids: Vec<Dom> = Vec::with_capacity(3);
    let mut sidebar_column: Option<Dom> = None;
    if let Some(sb) = sidebar {
        if inset > 0.0 {
            // the phone's rail: out of the flow, below the tab row, over the left edge of the rest
            let top = if rail_top.is_finite() && rail_top > 0.0 { rail_top } else { TOUCH_ROW };
            root_kids.push(
                styled(
                    Dom::create_div().with_child(sb.dom()),
                    &format!(
                        "position: absolute; {}: 0px; top: {top}px; bottom: 0px; display: flex; \
                         flex-direction: column; z-index: 6;",
                        edge.offset()
                    ),
                )
                .with_ids_and_classes(classes(&[FRAME_RAIL_CLASS])),
            );
        } else {
            sidebar_column = Some(sb.dom());
        }
    }
    if edge == Edge::Left {
        root_kids.extend(sidebar_column.take());
        root_kids.push(main);
    } else {
        root_kids.push(main);
        root_kids.extend(sidebar_column.take());
    }
    if let Some(s) = sheet.into_option() {
        root_kids.push(s);
    }
    let class_list: &[&'static str] = if phone {
        &[FRAME_CLASS, FRAME_PHONE_CLASS]
    } else {
        &[FRAME_CLASS]
    };
    let root = styled(
        Dom::create_div().with_children(DomVec::from_vec(root_kids)),
        "display: flex; flex-direction: row; width: 100%; height: 100%; min-width: 0px; min-height: 0px; \
         overflow: hidden; position: relative; box-sizing: border-box;",
    )
    .with_ids_and_classes(classes(class_list));
    if dom_id.as_str().is_empty() {
        root
    } else {
        root.with_id(dom_id)
    }
}

#[cfg(test)]
#[path = "app_frame_test.rs"]
mod app_frame_test;

