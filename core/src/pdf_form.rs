//! A PDF's interactive form (AcroForm) as data: the fields `ParsedPdf`
//! reads, the values and stamps it fills them with (azul-dll's
//! `desktop/extra/pdf`, printpdf's `forms` underneath).
//!
//! Geometry is in PDF points with the page's TOP-LEFT as origin and y
//! downwards - the page's SVG user space (`ParsedPdf::page_to_svg`), so a
//! field's rect is where to put an input over the page at any zoom: multiply
//! by the page's scale.

use azul_css::{AzString, StringVec};

/// What a form field is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum PdfFormFieldKind {
    /// A text box (single- or multi-line, maybe a password).
    #[default]
    Text,
    /// A check box: its value is its on-state name (`Yes`) or `Off`.
    CheckBox,
    /// A group of radio buttons: one field, one widget per button; its value
    /// is the chosen button's on-state name, its options the on-states.
    RadioButton,
    /// A drop-down list; its options are the choices.
    ComboBox,
    /// A list box; its options are the choices.
    ListBox,
    /// A push button (no value).
    PushButton,
    /// A signature field.
    Signature,
}

/// How a text field aligns its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum PdfTextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// A rectangle on a page, in points, from the page's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[repr(C)]
pub struct PdfRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Where a field is drawn: one of its widgets (a field can show on several
/// pages, a radio group has one widget per button).
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
#[repr(C)]
pub struct PdfFormWidget {
    /// The page, 0-based.
    pub page: usize,
    pub rect: PdfRect,
    /// A check box's / radio button's "on" state name (what the field's value
    /// is when this widget is checked); empty for other fields.
    pub on_state: AzString,
    /// Hidden or not to be shown: no viewer draws it.
    pub hidden: bool,
}

impl_option!(
    PdfFormWidget,
    OptionPdfFormWidget,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl_vec!(
    PdfFormWidget,
    PdfFormWidgetVec,
    PdfFormWidgetVecDestructor,
    PdfFormWidgetVecDestructorType,
    PdfFormWidgetVecSlice,
    OptionPdfFormWidget
);
impl_vec_debug!(PdfFormWidget, PdfFormWidgetVec);
impl_vec_clone!(PdfFormWidget, PdfFormWidgetVec, PdfFormWidgetVecDestructor);
impl_vec_partialeq!(PdfFormWidget, PdfFormWidgetVec);

/// One field of a PDF's form.
#[derive(Debug, Clone, PartialEq, Default)]
#[repr(C)]
pub struct PdfFormField {
    /// The fully qualified name (`parent.child`): what a value is set by.
    pub name: AzString,
    pub kind: PdfFormFieldKind,
    /// The value: a text, a choice's text, a check box's on-state name or
    /// `Off`.
    pub value: AzString,
    /// The value a reset gives it.
    pub default_value: AzString,
    /// A choice's options; a radio group's on-states.
    pub options: StringVec,
    /// Where it is drawn.
    pub widgets: PdfFormWidgetVec,
    pub read_only: bool,
    pub required: bool,
    /// A text field with several lines.
    pub multiline: bool,
    /// A text field showing bullets.
    pub password: bool,
    /// A text field's maximum length; 0 for no limit.
    pub max_length: u32,
    /// The font size the form asks for, in points; 0 = fit the field.
    pub font_size_pt: f32,
    pub alignment: PdfTextAlign,
}

impl_option!(
    PdfFormField,
    OptionPdfFormField,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl_vec!(
    PdfFormField,
    PdfFormFieldVec,
    PdfFormFieldVecDestructor,
    PdfFormFieldVecDestructorType,
    PdfFormFieldVecSlice,
    OptionPdfFormField
);
impl_vec_debug!(PdfFormField, PdfFormFieldVec);
impl_vec_clone!(PdfFormField, PdfFormFieldVec, PdfFormFieldVecDestructor);
impl_vec_partialeq!(PdfFormField, PdfFormFieldVec);

/// A value to fill a field with: its fully qualified name and the value - a
/// text, a choice's text, a check box's on-state name (or `true`), `Off` /
/// `false` to uncheck.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct PdfFieldValue {
    pub name: AzString,
    pub value: AzString,
}

impl PdfFieldValue {
    #[must_use]
    pub const fn create(name: AzString, value: AzString) -> Self {
        Self { name, value }
    }
}

impl_option!(
    PdfFieldValue,
    OptionPdfFieldValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    PdfFieldValue,
    PdfFieldValueVec,
    PdfFieldValueVecDestructor,
    PdfFieldValueVecDestructorType,
    PdfFieldValueVecSlice,
    OptionPdfFieldValue
);
impl_vec_debug!(PdfFieldValue, PdfFieldValueVec);
impl_vec_clone!(PdfFieldValue, PdfFieldValueVec, PdfFieldValueVecDestructor);
impl_vec_partialeq!(PdfFieldValue, PdfFieldValueVec);

/// A drawing put onto a page when a form is filled - a signature: an SVG
/// (its `viewBox` and its `<path>`s, stroked and / or filled) fit into
/// `rect`.
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
#[repr(C)]
pub struct PdfStamp {
    /// The page, 0-based.
    pub page: usize,
    pub rect: PdfRect,
    /// The drawing: `<svg viewBox="..."><path d="..." stroke="..."
    /// stroke-width="..." fill="..."/>...</svg>`.
    pub svg: AzString,
}

impl PdfStamp {
    #[must_use]
    pub const fn create(page: usize, rect: PdfRect, svg: AzString) -> Self {
        Self { page, rect, svg }
    }
}

impl_option!(
    PdfStamp,
    OptionPdfStamp,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl_vec!(
    PdfStamp,
    PdfStampVec,
    PdfStampVecDestructor,
    PdfStampVecDestructorType,
    PdfStampVecSlice,
    OptionPdfStamp
);
impl_vec_debug!(PdfStamp, PdfStampVec);
impl_vec_clone!(PdfStamp, PdfStampVec, PdfStampVecDestructor);
impl_vec_partialeq!(PdfStamp, PdfStampVec);

/// How `ParsedPdf::page_to_svg_with` renders a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct PdfPageSvgOptions {
    /// Draw the form fields' values on the page (what a filled form prints
    /// as). Off: the page without its form - an app overlays live inputs.
    pub include_form_fields: bool,
}
