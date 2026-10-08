//! Reading pane widget - the right pane of a mail window that shows the
//! open message: the subject in large type over the sender line and the
//! date, a notice strip (the [`InfoBar`]: "Click here to download pictures
//! ..."), the header fields ("Sent", "To", "Cc") as key / value rows that
//! wrap, the attachments as chips, the BODY on paper (a `Dom` the app hands
//! in: the sanitized mail), and the people footer ("More about: ..." with
//! avatars). Outlook 2010's reading pane.
//!
//! The pane shows what it is given and owns nothing: the app decides which
//! message is open and rebuilds. It reports the sender line and the people
//! line (`on_link`), the notice's action (`on_load_images`) and an
//! attachment (`on_attachment`). The parts are the toolkit's own widgets:
//! [`InfoBar`] for the notice, [`Chip`] for an attachment, [`Avatar`] for a
//! person, a link [`Button`](crate::widgets::button::Button) for the sender
//! and the people line (flora's text link under flora). For
//! assistive technology the pane is a document named by its subject.
//!
//! Key types: [`ReadingPane`], [`ReadingPaneEvent`], [`ReadingPaneEventKind`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec, OptionDom},
    refany::RefAny,
    window::StringPairVec,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutFlexWrap, LayoutMinHeight, LayoutMinWidth, LayoutOverflow,
        },
        property::CssProperty,
        style::StyleUserSelect,
    },
    AzString, StringVec,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        avatar::{Avatar, AvatarSize},
        button::{ButtonOnClick, ButtonOnClickCallback, ButtonOnClickCallbackType},
        chip::{Chip, ChipOnClickCallbackType, ChipState},
        details_pane::{PANE_KEY_BASE, PANE_ROW_BASE, PANE_VALUE_BASE},
        info_bar::{InfoBar, OptionInfoBar},
    },
};

static PANE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-reading-pane"))];
static HEADER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-header",
))];
static SUBJECT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-subject",
))];
static SENDER_LINE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-sender-line",
))];
static SENDER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-sender",
))];
static DATE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-date",
))];
static NOTICE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-notice",
))];
static FIELDS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-fields",
))];
static FIELD_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-field",
))];
static FIELD_KEY_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-field-key",
))];
static FIELD_VALUE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-field-value",
))];
static ATTACHMENTS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-attachments",
))];
static BODY_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-body",
))];
static FOOTER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-footer",
))];
static FOOTER_LINE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-reading-pane-footer-line",
))];

/// What was clicked in the pane.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReadingPaneEventKind {
    /// The sender line (`text`): open the contact, start a reply.
    Sender,
    /// The notice's action: load the pictures.
    LoadImages,
    /// An attachment chip (`index`, `text` its name): open or save it.
    Attachment,
    /// The people line (`text`): more about the people in the message.
    People,
}

/// One action in the pane: what, which, with what text.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingPaneEvent {
    /// The sender line, the attachment's name or the people line; empty
    /// for `LoadImages`.
    pub text: AzString,
    /// The attachment (`Attachment`); 0 otherwise.
    pub index: usize,
    /// What was clicked.
    pub kind: ReadingPaneEventKind,
}

/// Callback invoked for an action in the pane.
pub type ReadingPaneOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ReadingPaneEvent) -> Update;
impl_widget_callback!(
    ReadingPaneOnEvent,
    OptionReadingPaneOnEvent,
    ReadingPaneOnEventCallback,
    ReadingPaneOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ReadingPaneOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: READING_PANE_ON_EVENT_INVOKER,
    invoker_ty:     AzReadingPaneOnEventCallbackInvoker,
    thunk_fn:       az_reading_pane_on_event_callback_thunk,
    setter_fn:      AzApp_setReadingPaneOnEventCallbackInvoker,
    from_handle_fn: AzReadingPaneOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzReadingPaneOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ReadingPaneEvent ],
}

/// The open message: its header, notice, fields, attachments, body and
/// people.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ReadingPane {
    /// The subject, in large type.
    pub subject: AzString,
    /// The sender line ("Google Mail-Team <mail-noreply@google.com>").
    pub sender: AzString,
    /// The date, at the right of the sender line.
    pub date: AzString,
    /// The header fields in order, each key written with a colon after it
    /// ("Sent", "To", "Cc"); the values wrap.
    pub fields: StringPairVec,
    /// The attachments' names, as chips.
    pub attachments: StringVec,
    /// The notice strip under the header, or `None`.
    pub info_bar: OptionInfoBar,
    /// The body on paper: the app's sanitized mail, or `None` for an empty
    /// sheet.
    pub body: OptionDom,
    /// The initials of the people in the footer, one avatar each.
    pub people: StringVec,
    /// The footer's line ("More about: Google Mail-Team"), a link; empty
    /// with no people hides the footer.
    pub people_line: AzString,
    /// The sender line or the people line was clicked.
    pub on_link: OptionReadingPaneOnEvent,
    /// The notice's action was clicked. Set, it is what the notice's action
    /// does; unset, the notice keeps its own action.
    pub on_load_images: OptionReadingPaneOnEvent,
    /// An attachment chip was clicked.
    pub on_attachment: OptionReadingPaneOnEvent,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
}

/// What a theme decides about a reading pane: the SKIN of each part, laid
/// over the part's base (the pane's structure, the same in every theme:
/// `READING_PANE_*_BASE`) by [`build`].
pub(crate) struct ReadingPaneLook {
    /// The pane.
    pub pane: Vec<CssPropertyWithConditions>,
    /// The header block.
    pub header: Vec<CssPropertyWithConditions>,
    /// The subject.
    pub subject: Vec<CssPropertyWithConditions>,
    /// The sender line (the row of the sender link and the date).
    pub sender_line: Vec<CssPropertyWithConditions>,
    /// The date.
    pub date: Vec<CssPropertyWithConditions>,
    /// The notice's box.
    pub notice: Vec<CssPropertyWithConditions>,
    /// The column of field rows.
    pub fields: Vec<CssPropertyWithConditions>,
    /// A field's key.
    pub field_key: Vec<CssPropertyWithConditions>,
    /// A field's value.
    pub field_value: Vec<CssPropertyWithConditions>,
    /// The attachments row.
    pub attachments: Vec<CssPropertyWithConditions>,
    /// The paper the body sits on.
    pub body: Vec<CssPropertyWithConditions>,
    /// The people footer.
    pub footer: Vec<CssPropertyWithConditions>,
    /// The box around the footer's line.
    pub footer_line: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the pane, if it has one.
    pub marker: Option<&'static str>,
}

// ---- the base: the pane's structure, in every theme ----

/// The pane: a column that takes its space and scrolls.
pub(crate) static READING_PANE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Auto)),
];

/// A block of the pane (the header, the fields, the body's paper): a column
/// that never grows past its content.
pub(crate) static READING_PANE_BLOCK_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A line of the pane (the sender line, the footer): one row on its midline,
/// its text never selected by a drag.
pub(crate) static READING_PANE_LINE_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The attachments row wraps its chips.
pub(crate) static READING_PANE_WRAP_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_wrap(LayoutFlexWrap::Wrap)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A part that takes the rest of its line.
pub(crate) static READING_PANE_GROW_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A part that keeps its size.
pub(crate) static READING_PANE_FIXED_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

impl ReadingPane {
    /// A pane showing a message with `subject` from `sender`, no date, no
    /// fields, no notice, no attachments, an empty body and no people.
    #[must_use]
    pub fn create(subject: AzString, sender: AzString) -> Self {
        Self {
            subject,
            sender,
            date: AzString::from_const_str(""),
            fields: StringPairVec::from_const_slice(&[]),
            attachments: StringVec::from_const_slice(&[]),
            info_bar: OptionInfoBar::None,
            body: OptionDom::None,
            people: StringVec::from_const_slice(&[]),
            people_line: AzString::from_const_str(""),
            on_link: None.into(),
            on_load_images: None.into(),
            on_attachment: None.into(),
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// The date at the right of the sender line.
    pub fn set_date(&mut self, date: AzString) {
        self.date = date;
    }

    /// [`Self::set_date`] for the builder chain.
    #[must_use]
    pub fn with_date(mut self, date: AzString) -> Self {
        self.set_date(date);
        self
    }

    /// The header fields, in order.
    pub fn set_fields(&mut self, fields: StringPairVec) {
        self.fields = fields;
    }

    /// [`Self::set_fields`] for the builder chain.
    #[must_use]
    pub fn with_fields(mut self, fields: StringPairVec) -> Self {
        self.set_fields(fields);
        self
    }

    /// Adds one header field.
    pub fn add_field(&mut self, key: AzString, value: AzString) {
        self.fields
            .push(azul_core::window::AzStringPair::create(key, value));
    }

    /// [`Self::add_field`] for the builder chain.
    #[must_use]
    pub fn with_field(mut self, key: AzString, value: AzString) -> Self {
        self.add_field(key, value);
        self
    }

    /// The attachments' names.
    pub fn set_attachments(&mut self, attachments: StringVec) {
        self.attachments = attachments;
    }

    /// [`Self::set_attachments`] for the builder chain.
    #[must_use]
    pub fn with_attachments(mut self, attachments: StringVec) -> Self {
        self.set_attachments(attachments);
        self
    }

    /// The notice strip under the header.
    pub fn set_info_bar(&mut self, info_bar: InfoBar) {
        self.info_bar = OptionInfoBar::Some(info_bar);
    }

    /// [`Self::set_info_bar`] for the builder chain.
    #[must_use]
    pub fn with_info_bar(mut self, info_bar: InfoBar) -> Self {
        self.set_info_bar(info_bar);
        self
    }

    /// The body on paper.
    pub fn set_body(&mut self, body: Dom) {
        self.body = OptionDom::Some(body);
    }

    /// [`Self::set_body`] for the builder chain.
    #[must_use]
    pub fn with_body(mut self, body: Dom) -> Self {
        self.set_body(body);
        self
    }

    /// The people footer: the avatars' initials and the line.
    pub fn set_people(&mut self, people: StringVec, line: AzString) {
        self.people = people;
        self.people_line = line;
    }

    /// [`Self::set_people`] for the builder chain.
    #[must_use]
    pub fn with_people(mut self, people: StringVec, line: AzString) -> Self {
        self.set_people(people, line);
        self
    }

    /// Pin the widget theme; unset, the pane follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// The sender line or the people line was clicked.
    pub fn set_on_link<C: Into<ReadingPaneOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_link = OptionReadingPaneOnEvent::Some(ReadingPaneOnEvent::create(data, cb));
    }

    /// [`Self::set_on_link`] for the builder chain.
    #[must_use]
    pub fn with_on_link<C: Into<ReadingPaneOnEventCallback>>(mut self, data: RefAny, cb: C) -> Self {
        self.set_on_link(data, cb);
        self
    }

    /// The notice's action was clicked.
    pub fn set_on_load_images<C: Into<ReadingPaneOnEventCallback>>(
        &mut self,
        data: RefAny,
        cb: C,
    ) {
        self.on_load_images = OptionReadingPaneOnEvent::Some(ReadingPaneOnEvent::create(data, cb));
    }

    /// [`Self::set_on_load_images`] for the builder chain.
    #[must_use]
    pub fn with_on_load_images<C: Into<ReadingPaneOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_load_images(data, cb);
        self
    }

    /// An attachment chip was clicked.
    pub fn set_on_attachment<C: Into<ReadingPaneOnEventCallback>>(
        &mut self,
        data: RefAny,
        cb: C,
    ) {
        self.on_attachment = OptionReadingPaneOnEvent::Some(ReadingPaneOnEvent::create(data, cb));
    }

    /// [`Self::set_on_attachment`] for the builder chain.
    #[must_use]
    pub fn with_on_attachment<C: Into<ReadingPaneOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_attachment(data, cb);
        self
    }

    /// Replaces `self` with an empty pane and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""), AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// The pane's DOM. The look comes from the theme module
    /// (`themes::flat::reading_pane` / `themes::flora::reading_pane`);
    /// `None` carries both looks, each in its `@theme(<name>)` block, and
    /// the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::reading_pane(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::reading_pane(self),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                self,
                crate::widgets::themes::flat::reading_pane,
                crate::widgets::themes::flora::reading_pane,
            ),
        }
    }
}

impl Default for ReadingPane {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""), AzString::from_const_str(""))
    }
}

impl From<ReadingPane> for Dom {
    fn from(p: ReadingPane) -> Self {
        p.dom()
    }
}

/// What every part of one pane shares: the app's hooks and the texts they
/// report.
struct PaneShared {
    on_link: OptionReadingPaneOnEvent,
    on_load_images: OptionReadingPaneOnEvent,
    on_attachment: OptionReadingPaneOnEvent,
    sender: AzString,
    people_line: AzString,
}

/// Hands `event` to `hook`.
fn fire(hook: &OptionReadingPaneOnEvent, info: CallbackInfo, event: ReadingPaneEvent) -> Update {
    match hook.as_ref() {
        Some(ReadingPaneOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, event),
        None => Update::DoNothing,
    }
}

extern "C" fn on_sender_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(shared) = data.downcast_ref::<PaneShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_link,
        info,
        ReadingPaneEvent {
            text: shared.sender.clone(),
            index: 0,
            kind: ReadingPaneEventKind::Sender,
        },
    )
}

extern "C" fn on_people_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(shared) = data.downcast_ref::<PaneShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_link,
        info,
        ReadingPaneEvent {
            text: shared.people_line.clone(),
            index: 0,
            kind: ReadingPaneEventKind::People,
        },
    )
}

extern "C" fn on_load_images_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(shared) = data.downcast_ref::<PaneShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_load_images,
        info,
        ReadingPaneEvent {
            text: AzString::from_const_str(""),
            index: 0,
            kind: ReadingPaneEventKind::LoadImages,
        },
    )
}

/// An attachment chip's payload.
struct AttachmentData {
    index: usize,
    name: AzString,
    shared: RefAny,
}

extern "C" fn on_attachment_click(mut data: RefAny, info: CallbackInfo, _: ChipState) -> Update {
    let (index, name, mut shared) = {
        let Some(a) = data.downcast_ref::<AttachmentData>() else {
            return Update::DoNothing;
        };
        (a.index, a.name.clone(), a.shared.clone())
    };
    let Some(shared) = shared.downcast_ref::<PaneShared>() else {
        return Update::DoNothing;
    };
    fire(
        &shared.on_attachment,
        info,
        ReadingPaneEvent {
            text: name,
            index,
            kind: ReadingPaneEventKind::Attachment,
        },
    )
}

/// The pane's DOM in `look`: pane [header [subject, sender line [sender,
/// date]], notice?, fields [row [key, value]..], attachments?, body,
/// footer? [avatars.., line]]. Every part is its base (the structure),
/// then the look's skin; the notice, the chips, the avatars and the links
/// are the toolkit's own widgets, pinned to the pane's theme (or following
/// the app theme with it).
#[allow(clippy::too_many_lines)]
pub(crate) fn build(pane: ReadingPane, look: &ReadingPaneLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let ReadingPane {
        subject,
        sender,
        date,
        fields,
        attachments,
        info_bar,
        body,
        people,
        people_line,
        on_link,
        on_load_images,
        on_attachment,
        theme,
    } = pane;
    let theme = theme.into_option();
    let has_load_images = on_load_images.is_some();
    let shared = RefAny::new(PaneShared {
        on_link,
        on_load_images,
        on_attachment,
        sender: sender.clone(),
        people_line: people_line.clone(),
    });
    // The sender and the people line are data a user opens: a link, in
    // flora flora's text link rather than its quiet command.
    let link = |label: AzString, cb: ButtonOnClickCallbackType| {
        crate::widgets::button::data_link(
            crate::widgets::button::DataLink {
                label,
                data: shared.clone(),
                on_click: cb,
            },
            theme,
        )
    };

    // The header: the subject over the sender line.
    let sender_line = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SENDER_LINE_CLASS))
        .with_css_props(part(READING_PANE_LINE_BASE, &look.sender_line))
        .with_children(DomVec::from_vec(alloc::vec![
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(SENDER_CLASS))
                .with_css_props(part(READING_PANE_GROW_BASE, &[]))
                .with_child(link(sender, on_sender_click)),
            crate::widgets::widget_p_with_text(date)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(DATE_CLASS))
                .with_css_props(part(READING_PANE_FIXED_BASE, &look.date)),
        ]));
    let header = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_CLASS))
        .with_css_props(part(READING_PANE_BLOCK_BASE, &look.header))
        .with_children(DomVec::from_vec(alloc::vec![
            crate::widgets::widget_p_with_text(subject.clone())
                .with_ids_and_classes(IdOrClassVec::from_const_slice(SUBJECT_CLASS))
                .with_css_props(part(&[], &look.subject)),
            sender_line,
        ]));

    let mut children: Vec<Dom> = Vec::with_capacity(6);
    children.push(header);

    // The notice: the app's bar; its action reports `on_load_images` when
    // the pane has that hook.
    if let Some(mut bar) = info_bar.into_option() {
        if has_load_images {
            bar.on_action = Some(ButtonOnClick {
                refany: shared.clone(),
                callback: ButtonOnClickCallback::from(
                    on_load_images_click as ButtonOnClickCallbackType,
                ),
            })
            .into();
        }
        if let Some(theme) = theme {
            bar = bar.with_theme(theme);
        }
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(NOTICE_CLASS))
                .with_css_props(part(READING_PANE_BLOCK_BASE, &look.notice))
                .with_child(bar.dom()),
        );
    }

    // The fields: "key: value" rows, the values wrapping.
    if !fields.as_ref().is_empty() {
        let rows: Vec<Dom> = fields
            .as_ref()
            .iter()
            .map(|pair| {
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(FIELD_CLASS))
                    .with_css_props(part(PANE_ROW_BASE, &[]))
                    .with_children(DomVec::from_vec(alloc::vec![
                        crate::widgets::widget_p_with_text(AzString::from(alloc::format!(
                            "{}:",
                            pair.key.as_str()
                        )))
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(FIELD_KEY_CLASS))
                        .with_css_props(part(PANE_KEY_BASE, &look.field_key)),
                        crate::widgets::widget_p_with_text(pair.value.clone())
                            .with_ids_and_classes(IdOrClassVec::from_const_slice(FIELD_VALUE_CLASS))
                            .with_css_props(part(PANE_VALUE_BASE, &look.field_value)),
                    ]))
            })
            .collect();
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(FIELDS_CLASS))
                .with_css_props(part(READING_PANE_BLOCK_BASE, &look.fields))
                .with_children(DomVec::from_vec(rows)),
        );
    }

    // The attachments: a chip each.
    if !attachments.as_ref().is_empty() {
        let chips: Vec<Dom> = attachments
            .as_ref()
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let mut chip = Chip::create(name.clone()).with_on_click(
                    RefAny::new(AttachmentData {
                        index,
                        name: name.clone(),
                        shared: shared.clone(),
                    }),
                    on_attachment_click as ChipOnClickCallbackType,
                );
                if let Some(theme) = theme {
                    chip = chip.with_theme(theme);
                }
                chip.dom()
            })
            .collect();
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ATTACHMENTS_CLASS))
                .with_css_props(part(READING_PANE_WRAP_BASE, &look.attachments))
                .with_children(DomVec::from_vec(chips)),
        );
    }

    // The body on paper.
    let mut paper = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(BODY_CLASS))
        .with_css_props(part(READING_PANE_BLOCK_BASE, &look.body));
    if let Some(body) = body.into_option() {
        paper = paper.with_child(body);
    }
    children.push(paper);

    // The people footer.
    if !people.as_ref().is_empty() || !people_line.as_str().is_empty() {
        let mut footer: Vec<Dom> = Vec::with_capacity(people.as_ref().len() + 1);
        for initials in people.as_ref() {
            let mut avatar = Avatar::create(initials.clone()).with_size(AvatarSize::Small);
            if let Some(theme) = theme {
                avatar = avatar.with_theme(theme);
            }
            footer.push(avatar.dom());
        }
        if !people_line.as_str().is_empty() {
            footer.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(FOOTER_LINE_CLASS))
                    .with_css_props(part(READING_PANE_GROW_BASE, &look.footer_line))
                    .with_child(link(people_line, on_people_click)),
            );
        }
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(FOOTER_CLASS))
                .with_css_props(part(READING_PANE_LINE_BASE, &look.footer))
                .with_children(DomVec::from_vec(footer)),
        );
    }

    let mut classes: Vec<IdOrClass> = PANE_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(READING_PANE_BASE, &look.pane))
        // The open message: a DOCUMENT named by its subject.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Document,
            accessibility_name: Some(subject).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(children))
}

#[cfg(test)]
mod reading_pane_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    };

    type Log = Arc<Mutex<Vec<(ReadingPaneEventKind, usize, String)>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: ReadingPaneEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push((
                event.kind,
                event.index,
                event.text.as_str().to_string(),
            ));
        }
        Update::RefreshDom
    }

    fn strs(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    fn welcome(log: &Log) -> ReadingPane {
        let data = || RefAny::new(log.clone());
        let cb = record as ReadingPaneOnEventCallbackType;
        ReadingPane::create(
            AzString::from("Welcome to Gmail"),
            AzString::from("Google Mail-Team <mail-noreply@google.com>"),
        )
        .with_date(AzString::from("Wed 30.09.2026 21:12"))
        .with_field(AzString::from("Sent"), AzString::from("Wed 30.09.2026 21:12"))
        .with_field(AzString::from("To"), AzString::from("felix@example.com"))
        .with_attachments(strs(&["invoice.pdf", "photo.jpg"]))
        .with_info_bar(
            InfoBar::create(AzString::from("Click here to download pictures."))
                .with_icon(AzString::from("info"))
                .with_action(AzString::from("Download pictures")),
        )
        .with_body(Dom::create_p_with_text("Hello"))
        .with_people(strs(&["GM"]), AzString::from("More about: Google Mail-Team"))
        .with_on_link(data(), cb)
        .with_on_load_images(data(), cb)
        .with_on_attachment(data(), cb)
    }

    /// Every text of the subtree, in document order.
    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            out.push(s.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The nodes of `styled` carrying `class`, in document order.
    fn nodes_with(styled: &StyledDom, class: &str) -> Vec<NodeId> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .map(|(i, _)| NodeId::new(i))
            .collect()
    }

    /// The first keyboard stop under `node` (a button's root).
    fn first_stop_under(styled: &StyledDom, node: NodeId) -> NodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let end = hierarchy[node.index()]
            .next_sibling_id()
            .map_or(nodes.len(), |n| n.index());
        (node.index() + 1..end)
            .map(NodeId::new)
            .find(|n| nodes[n.index()].get_tab_index().is_some())
            .expect("a keyboard stop under the node")
    }

    #[test]
    fn the_pane_is_the_header_the_notice_the_fields_the_attachments_the_body_and_the_people() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for theme in checks::BOTH {
            let dom = welcome(&log).with_theme(theme).dom();
            let parts = dom.children.as_ref();
            assert_eq!(
                parts.len(),
                6,
                "{}: header, notice, fields, attachments, body, footer",
                theme.name()
            );
            let mut header = Vec::new();
            texts(&parts[0], &mut header);
            assert_eq!(
                header,
                vec![
                    "Welcome to Gmail",
                    "Google Mail-Team <mail-noreply@google.com>",
                    "Wed 30.09.2026 21:12"
                ],
                "{}",
                theme.name()
            );
            assert!(
                theme_checks::find(&parts[1], "__azul-native-info-bar").is_some(),
                "{}: the notice is the info bar",
                theme.name()
            );
            let mut fields = Vec::new();
            texts(&parts[2], &mut fields);
            assert_eq!(
                fields,
                vec!["Sent:", "Wed 30.09.2026 21:12", "To:", "felix@example.com"],
                "{}: the keys end in a colon",
                theme.name()
            );
            assert_eq!(
                parts[3].children.as_ref().len(),
                2,
                "{}: one chip per attachment",
                theme.name()
            );
            let mut body = Vec::new();
            texts(&parts[4], &mut body);
            assert_eq!(body, vec!["Hello"], "{}: the app's body on the paper", theme.name());
            assert_eq!(
                parts[5].children.as_ref().len(),
                2,
                "{}: an avatar and the line",
                theme.name()
            );
        }
        let bare = ReadingPane::create(AzString::from("Hi"), AzString::from("Bob"))
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(
            bare.children.as_ref().len(),
            2,
            "nothing but the header and an empty sheet"
        );
    }

    #[test]
    fn the_pane_is_a_document_named_by_its_subject_and_its_links_are_buttons() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = welcome(&log).with_theme(UiTheme::Flat).dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Document);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str()),
            Some("Welcome to Gmail")
        );
        assert!(dom.root.get_tab_index().is_none(), "the pane takes no focus");
        let stops = theme_checks::focusable(&dom);
        // The sender, the notice's action, two chips, the people line.
        assert_eq!(stops.len(), 5, "{stops:?}");
    }

    #[test]
    fn the_sender_the_notice_the_attachments_and_the_people_line_report() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(welcome(&log).with_theme(UiTheme::Flat).dom());
        let click = |node: NodeId| {
            rv::fire(&styled, id(node), EventFilter::Hover(HoverEventFilter::Click))
                .expect("a click target")
        };
        let sender = nodes_with(&styled, "__azul-native-reading-pane-sender")[0];
        let (update, _) = click(first_stop_under(&styled, sender));
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        let notice = nodes_with(&styled, "__azul-native-reading-pane-notice")[0];
        click(first_stop_under(&styled, notice));
        let attachments = nodes_with(&styled, "__azul-native-reading-pane-attachments")[0];
        let hierarchy = styled.node_hierarchy.as_ref();
        let first_chip = hierarchy[attachments.index()]
            .first_child_id(attachments)
            .expect("a chip");
        let second_chip = hierarchy[first_chip.index()]
            .next_sibling_id()
            .expect("another chip");
        click(first_stop_under(&styled, second_chip));
        let line = nodes_with(&styled, "__azul-native-reading-pane-footer-line")[0];
        click(first_stop_under(&styled, line));
        assert_eq!(
            *log.lock().expect("log"),
            vec![
                (
                    ReadingPaneEventKind::Sender,
                    0,
                    "Google Mail-Team <mail-noreply@google.com>".to_string()
                ),
                (ReadingPaneEventKind::LoadImages, 0, String::new()),
                (ReadingPaneEventKind::Attachment, 1, "photo.jpg".to_string()),
                (
                    ReadingPaneEventKind::People,
                    0,
                    "More about: Google Mail-Team".to_string()
                ),
            ]
        );
    }

    #[test]
    fn a_pane_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        checks::assert_follows_the_app_theme(
            "reading_pane",
            || welcome(&log).dom(),
            |t: UiTheme| welcome(&log).with_theme(t).dom(),
        );
        for theme in checks::BOTH {
            let dom = checks::under(theme, || welcome(&log).dom());
            theme_checks::assert_structure_is_shared(
                &format!("reading_pane built for {}", theme.name()),
                &dom,
                &[],
            );
        }
    }
}
