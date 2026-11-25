//! Variable fonts: the axes of a font file, and static instances of it for embedding.
//!
//! A variable font is one file that holds a whole family along continuous *axes*: weight
//! (`wght`), width (`wdth`), optical size (`opsz`), slant (`slnt`), italic (`ital`) and custom
//! ones. PDF has no variable fonts: a viewer reads an embedded font program as its default
//! instance. A variable font therefore has to be converted into a static font at chosen axis values
//! before it is embedded. The HTML layout does that on its own, with one instance per CSS weight.
//! From Rust code this module does it:
//!
//! - [`font_variation_axes`] lists the axes of a font file.
//! - [`instantiate_variable_font_bytes`] creates a static font file at given axis values.
//! - [`PdfDocument::add_variable_font`] creates an instance and registers it in a document.
//!
//! A variable font that goes through [`PdfDocument::add_font`] is embedded at its default
//! instance, and [`PdfDocument::save`] adds a warning to say so.
//!
//! Requires the `text_layout` feature.

use std::{borrow::Cow, collections::BTreeMap, fmt, str::FromStr};

use allsorts::{
    binary::read::ReadScope,
    error::{ParseError, ReadWriteError},
    font_data::{DynamicFontTableProvider, FontData},
    subset::{subset, whole_font, CmapTarget, SubsetProfile},
    tables::{
        variable_fonts::fvar::FvarTable, Fixed, FontTableProvider, MaxpTable, OpenTypeData,
    },
    tag,
    variations::{axis_names, instance, VariationError},
};
use serde_derive::{Deserialize, Serialize};

use crate::{FontId, ParsedFont, PdfDocument, PdfWarnMsg};

/// Four-byte tag of a variation axis, such as `wght` or `opsz`.
///
/// A tag is four ASCII letters or digits that start with a letter, and it may be padded with
/// spaces at the end. Serialized with serde as the four-character string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct FontVariationTag([u8; 4]);

impl FontVariationTag {
    /// Weight (`wght`): 400 is regular and 700 is bold.
    pub const WGHT: Self = Self(*b"wght");
    /// Width (`wdth`), in percent of the normal width.
    pub const WDTH: Self = Self(*b"wdth");
    /// Optical size (`opsz`), in points.
    pub const OPSZ: Self = Self(*b"opsz");
    /// Slant (`slnt`), in degrees; negative values lean to the right.
    pub const SLNT: Self = Self(*b"slnt");
    /// Italic (`ital`): 0 is upright and 1 is italic.
    pub const ITAL: Self = Self(*b"ital");

    /// Makes a tag from its four bytes.
    ///
    /// # Errors
    ///
    /// [`VariableFontError::InvalidTag`] when the bytes are not a tag (see the type).
    pub fn new(bytes: [u8; 4]) -> Result<Self, VariableFontError> {
        let [first, rest @ ..] = &bytes;
        // After the first space only spaces may follow.
        let mut padding = false;
        let rest_is_valid = rest.iter().all(|byte| {
            padding |= *byte == b' ';
            if padding {
                *byte == b' '
            } else {
                byte.is_ascii_alphanumeric()
            }
        });
        if first.is_ascii_alphabetic() && rest_is_valid {
            Ok(Self(bytes))
        } else {
            Err(VariableFontError::InvalidTag(
                String::from_utf8_lossy(&bytes).into_owned(),
            ))
        }
    }

    /// The four bytes of the tag.
    pub const fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    /// The tag as the big-endian number that font tables store.
    pub const fn as_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }
}

impl TryFrom<&str> for FontVariationTag {
    type Error = VariableFontError;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        let bytes = <[u8; 4]>::try_from(text.as_bytes())
            .map_err(|_| VariableFontError::InvalidTag(text.to_string()))?;
        Self::new(bytes)
    }
}

impl TryFrom<String> for FontVariationTag {
    type Error = VariableFontError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::try_from(text.as_str())
    }
}

impl FromStr for FontVariationTag {
    type Err = VariableFontError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from(text)
    }
}

impl From<FontVariationTag> for String {
    fn from(tag: FontVariationTag) -> Self {
        tag.to_string()
    }
}

impl fmt::Display for FontVariationTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The bytes are ASCII, so each byte is one character.
        let text: String = self.0.iter().copied().map(char::from).collect();
        f.pad(&text)
    }
}

/// One variation axis of a font, as its `fvar` table declares it.
///
/// Values are in the unit of the axis: weight units for `wght`, percent for `wdth`, points for
/// `opsz`, degrees for `slnt`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontVariationAxis {
    /// Tag of the axis.
    pub tag: FontVariationTag,
    /// Display name of the axis (such as "Weight"): the name that the `STAT` table points to in
    /// the `name` table. `None` for a font without a `STAT` table.
    pub name: Option<String>,
    /// Smallest value of the axis.
    pub min: f32,
    /// Value of the default instance, which is what a PDF viewer shows of an unresolved font.
    pub default: f32,
    /// Largest value of the axis.
    pub max: f32,
}

/// Values for the axes of a variable font: axis tag to value, in the unit of the axis.
///
/// An axis that has no value stays at its default. Serialized with serde as a map from the tag
/// string to the number, such as `{"wght":700.0}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FontVariationSettings {
    /// The value of each axis that is set.
    pub coordinates: BTreeMap<FontVariationTag, f32>,
}

impl FontVariationSettings {
    /// Settings without a value: every axis at its default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the value of one axis and returns the value it had before, if any.
    pub fn insert(&mut self, tag: FontVariationTag, value: f32) -> Option<f32> {
        self.coordinates.insert(tag, value)
    }

    /// Returns the settings with the value of one axis set, for chaining.
    pub fn with(mut self, tag: FontVariationTag, value: f32) -> Self {
        self.insert(tag, value);
        self
    }

    /// The value set for an axis, if any.
    pub fn get(&self, tag: FontVariationTag) -> Option<f32> {
        self.coordinates.get(&tag).copied()
    }
}

/// Why a variable font could not be read or instantiated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableFontError {
    /// The bytes are not a font file, or a table that is needed is damaged. Holds the reason.
    InvalidFont(String),
    /// The font file has no face with this index.
    InvalidFaceIndex {
        /// The index that was asked for.
        index: usize,
    },
    /// The font has no variation axes, so there is nothing to instantiate.
    NotVariable,
    /// The text is not an axis tag. Holds the text.
    InvalidTag(String),
    /// The font has no axis with this tag.
    UnknownAxis(FontVariationTag),
    /// The value for the axis is NaN or infinite.
    NonFiniteValue {
        /// The axis with the bad value.
        tag: FontVariationTag,
    },
    /// The outlines are neither TrueType with `gvar` nor CFF2. Holds the reason.
    UnsupportedOutlineFormat(String),
    /// The instance could not be created. Holds the reason.
    Instancing(String),
    /// The created instance does not parse as a font. Holds the reason.
    StaticFontParse(String),
    /// The created instance could not be converted to a font that a PDF can embed (CFF2 to CFF).
    /// Holds the reason.
    PdfConversion(String),
}

impl fmt::Display for VariableFontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFont(reason) => write!(f, "invalid font: {reason}"),
            Self::InvalidFaceIndex { index } => write!(f, "the font file has no face {index}"),
            Self::NotVariable => f.write_str("the font has no variation axes (no fvar table)"),
            Self::InvalidTag(text) => write!(
                f,
                "{text:?} is not an axis tag (four letters or digits, starting with a letter)"
            ),
            Self::UnknownAxis(tag) => write!(f, "the font has no variation axis {tag}"),
            Self::NonFiniteValue { tag } => {
                write!(f, "the value for variation axis {tag} is not a finite number")
            }
            Self::UnsupportedOutlineFormat(reason) => {
                write!(f, "unsupported variable font outlines: {reason}")
            }
            Self::Instancing(reason) => write!(f, "cannot create the instance: {reason}"),
            Self::StaticFontParse(reason) => {
                write!(f, "the created instance does not parse as a font: {reason}")
            }
            Self::PdfConversion(reason) => {
                write!(f, "cannot convert the instance for embedding in a PDF: {reason}")
            }
        }
    }
}

impl std::error::Error for VariableFontError {}

fn invalid_font(error: impl fmt::Display) -> VariableFontError {
    VariableFontError::InvalidFont(error.to_string())
}

fn conversion_error(error: impl fmt::Display) -> VariableFontError {
    VariableFontError::PdfConversion(error.to_string())
}

/// An axis of the `fvar` table, with its range as the 16.16 fixed point numbers of the font.
struct Axis {
    tag: FontVariationTag,
    min: Fixed,
    default: Fixed,
    max: Fixed,
}

/// The tables of the face `face_index` of a font file.
fn face_provider<'a>(
    font_file: &FontData<'a>,
    face_index: usize,
) -> Result<DynamicFontTableProvider<'a>, VariableFontError> {
    // A file with one face has no other index, but allsorts hands out that face for any index.
    let one_face = match font_file {
        FontData::OpenType(font) => matches!(font.data, OpenTypeData::Single(_)),
        FontData::Woff(_) => true,
        FontData::Woff2(font) => font.collection_directory.is_none(),
    };
    if one_face && face_index != 0 {
        return Err(VariableFontError::InvalidFaceIndex { index: face_index });
    }
    font_file.table_provider(face_index).map_err(|error| match error {
        ReadWriteError::Read(ParseError::BadIndex) => {
            VariableFontError::InvalidFaceIndex { index: face_index }
        }
        other => invalid_font(other),
    })
}

/// The axes of the `fvar` table; empty when there is none.
fn read_axes(provider: &impl FontTableProvider) -> Result<Vec<Axis>, VariableFontError> {
    let Some(data) = provider.table_data(tag::FVAR).map_err(invalid_font)? else {
        return Ok(Vec::new());
    };
    let fvar = ReadScope::new(&data)
        .read::<FvarTable<'_>>()
        .map_err(invalid_font)?;
    fvar.axes()
        .map(|axis| {
            let tag = FontVariationTag::new(axis.axis_tag.to_be_bytes()).map_err(|_| {
                invalid_font(format!("the fvar axis tag {:#010x} is not valid", axis.axis_tag))
            })?;
            if !(axis.min_value <= axis.default_value && axis.default_value <= axis.max_value) {
                return Err(invalid_font(format!(
                    "the range of variation axis {tag} is not in order (minimum, default, maximum)"
                )));
            }
            Ok(Axis {
                tag,
                min: axis.min_value,
                default: axis.default_value,
                max: axis.max_value,
            })
        })
        .collect()
}

/// Lists the variation axes of one face of a font file, in the order of its `fvar` table.
///
/// `face_index` selects the face of a font collection; use 0 for a plain `.ttf` or `.otf`. A
/// static font has no axes and gives an empty list. The name of an axis comes from the `STAT` and
/// `name` tables and is `None` for a font without a `STAT` table.
///
/// # Errors
///
/// [`VariableFontError::InvalidFont`] when the bytes are not a font file or the `fvar` table is
/// damaged, and [`VariableFontError::InvalidFaceIndex`] when the file has no face `face_index`.
pub fn font_variation_axes(
    bytes: &[u8],
    face_index: usize,
) -> Result<Vec<FontVariationAxis>, VariableFontError> {
    let font_file = ReadScope::new(bytes)
        .read::<FontData<'_>>()
        .map_err(invalid_font)?;
    let provider = face_provider(&font_file, face_index)?;
    let axes = read_axes(&provider)?;
    if axes.is_empty() {
        return Ok(Vec::new());
    }
    let names = axis_names(&provider).unwrap_or_default();
    Ok(axes
        .into_iter()
        .map(|axis| FontVariationAxis {
            name: names
                .iter()
                .find(|named| named.tag == axis.tag.as_u32())
                .map(|named| named.name.to_string()),
            tag: axis.tag,
            min: f32::from(axis.min),
            default: f32::from(axis.default),
            max: f32::from(axis.max),
        })
        .collect())
}

/// The 16.16 fixed point number closest to `value` that lies in `min..=max`.
fn to_fixed(value: f32, min: Fixed, max: Fixed) -> Fixed {
    // The conversion to an integer saturates, so a huge value ends at the edge of the range.
    let raw = (f64::from(value) * 65536.0).round() as i32;
    Fixed::from_raw(raw.max(min.raw_value()).min(max.raw_value()))
}

/// Creates a static font file from a variable font at chosen axis values.
///
/// `face_index` selects the face of a font collection; use 0 for a plain `.ttf` or `.otf`. An
/// axis that `settings` does not name stays at its default. A value outside the range of the axis
/// is clamped to it, and the values are rounded to the 16.16 fixed point numbers of the font.
/// The second result holds the value that every axis of the font was set to, so a clamped value
/// shows there.
///
/// The result is a static font without variation tables, so it parses and embeds like any other
/// font. TrueType outlines (`glyf` with `gvar`) and CFF2 outlines are supported. CFF2 is not
/// valid in a PDF, so the outlines of a CFF2 font are converted to CFF; the glyph ids and the
/// other tables of the font, including its layout tables, stay as they are.
///
/// # Errors
///
/// - [`VariableFontError::NotVariable`] for a font without axes.
/// - [`VariableFontError::UnknownAxis`] when `settings` names an axis that the font does not have,
///   and [`VariableFontError::NonFiniteValue`] for a NaN or infinite value.
/// - [`VariableFontError::InvalidFont`] and [`VariableFontError::InvalidFaceIndex`] as for
///   [`font_variation_axes`].
/// - [`VariableFontError::UnsupportedOutlineFormat`], [`VariableFontError::Instancing`] and
///   [`VariableFontError::PdfConversion`] when the font cannot be instantiated.
pub fn instantiate_variable_font_bytes(
    bytes: &[u8],
    face_index: usize,
    settings: &FontVariationSettings,
) -> Result<(Vec<u8>, FontVariationSettings), VariableFontError> {
    let font_file = ReadScope::new(bytes)
        .read::<FontData<'_>>()
        .map_err(invalid_font)?;
    let provider = face_provider(&font_file, face_index)?;
    let axes = read_axes(&provider)?;
    if axes.is_empty() {
        return Err(VariableFontError::NotVariable);
    }

    for (&tag, &value) in &settings.coordinates {
        if !axes.iter().any(|axis| axis.tag == tag) {
            return Err(VariableFontError::UnknownAxis(tag));
        }
        if !value.is_finite() {
            return Err(VariableFontError::NonFiniteValue { tag });
        }
    }

    // One value for each axis of the font, in the order of the fvar table.
    let mut user_instance = Vec::with_capacity(axes.len());
    let mut resolved = FontVariationSettings::new();
    for axis in &axes {
        let value = match settings.get(axis.tag) {
            Some(requested) => to_fixed(requested, axis.min, axis.max),
            None => axis.default,
        };
        resolved.insert(axis.tag, f32::from(value));
        user_instance.push(value);
    }

    let (static_bytes, _) = instance(&provider, &user_instance).map_err(|error| match error {
        VariationError::NotVariableFont => VariableFontError::NotVariable,
        VariationError::NotImplemented => VariableFontError::UnsupportedOutlineFormat(
            "expected TrueType outlines with a gvar table, or CFF2 outlines".to_string(),
        ),
        other => VariableFontError::Instancing(other.to_string()),
    })?;
    let static_bytes = if has_cff2_outlines(&static_bytes)? {
        cff2_to_cff1(&static_bytes)?
    } else {
        static_bytes
    };
    Ok((static_bytes, resolved))
}

/// Whether a static font has CFF2 outlines, which a PDF cannot embed.
fn has_cff2_outlines(bytes: &[u8]) -> Result<bool, VariableFontError> {
    let font_file = ReadScope::new(bytes)
        .read::<FontData<'_>>()
        .map_err(conversion_error)?;
    let provider = font_file.table_provider(0).map_err(conversion_error)?;
    Ok(provider.has_table(tag::CFF2) && !provider.has_table(tag::GLYF))
}

/// The tables of a font, except that its `CFF2` table is replaced by a `CFF ` table.
struct WithCff<'a, P> {
    font: &'a P,
    cff: Vec<u8>,
}

impl<P: FontTableProvider> FontTableProvider for WithCff<'_, P> {
    fn table_data(&self, table: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError> {
        match table {
            tag::CFF => Ok(Some(Cow::Borrowed(self.cff.as_slice()))),
            tag::CFF2 => Ok(None),
            _ => self.font.table_data(table),
        }
    }

    fn has_table(&self, table: u32) -> bool {
        match table {
            tag::CFF => true,
            tag::CFF2 => false,
            _ => self.font.has_table(table),
        }
    }

    fn table_tags(&self) -> Option<Vec<u32>> {
        let mut tags: Vec<u32> = self
            .font
            .table_tags()?
            .into_iter()
            .filter(|&table| table != tag::CFF && table != tag::CFF2)
            .collect();
        tags.push(tag::CFF);
        Some(tags)
    }
}

/// Converts the CFF2 outlines of a static font to CFF, which is what a PDF can embed.
///
/// The allsorts subsetter writes a CFF table when it is given a CFF2 font. Given every glyph in
/// order, it keeps the glyph ids, so the rest of the font (character map, metrics, `GSUB`,
/// `GPOS`, `GDEF`) stays valid and is carried over unchanged.
fn cff2_to_cff1(bytes: &[u8]) -> Result<Vec<u8>, VariableFontError> {
    let font_file = ReadScope::new(bytes)
        .read::<FontData<'_>>()
        .map_err(conversion_error)?;
    let provider = font_file.table_provider(0).map_err(conversion_error)?;
    let maxp = ReadScope::new(&provider.read_table_data(tag::MAXP).map_err(conversion_error)?)
        .read::<MaxpTable>()
        .map_err(conversion_error)?;
    let glyph_ids: Vec<u16> = (0..maxp.num_glyphs).collect();

    let converted = subset(
        &provider,
        &glyph_ids,
        &SubsetProfile::Minimal,
        CmapTarget::Unicode,
    )
    .map_err(conversion_error)?;
    let converted_file = ReadScope::new(&converted)
        .read::<FontData<'_>>()
        .map_err(conversion_error)?;
    let converted_provider = converted_file.table_provider(0).map_err(conversion_error)?;
    let converted_glyphs = ReadScope::new(
        &converted_provider
            .read_table_data(tag::MAXP)
            .map_err(conversion_error)?,
    )
    .read::<MaxpTable>()
    .map_err(conversion_error)?
    .num_glyphs;
    if converted_glyphs != maxp.num_glyphs {
        return Err(conversion_error(format!(
            "the converted outlines have {converted_glyphs} glyphs, the font has {}",
            maxp.num_glyphs
        )));
    }
    let cff = converted_provider
        .read_table_data(tag::CFF)
        .map_err(conversion_error)?
        .into_owned();

    let with_cff = WithCff {
        font: &provider,
        cff,
    };
    let tags = with_cff
        .table_tags()
        .ok_or_else(|| conversion_error("the list of tables of the font is not available"))?;
    whole_font(&with_cff, &tags).map_err(conversion_error)
}

/// The warning that `save` adds for a font that is embedded with its variation tables.
///
/// A PDF has no variable fonts, so such a font is embedded at its default instance. `None` for a
/// font without an `fvar` table.
pub(crate) fn unresolved_variation_warning(font_id: &FontId, font: &ParsedFont) -> Option<PdfWarnMsg> {
    let bytes = font.source_bytes()?;
    let font_file = ReadScope::new(bytes.as_slice()).read::<FontData<'_>>().ok()?;
    let provider = font_file.table_provider(font.original_index).ok()?;
    if !provider.has_table(tag::FVAR) {
        return None;
    }
    let name = font.font_name.as_deref().unwrap_or(&font_id.0);
    Some(PdfWarnMsg::warning(
        0,
        0,
        format!(
            "font {name} is a variable font (it has an fvar table) and is embedded at its default \
             instance, because a PDF has no variable fonts; PdfDocument::add_variable_font embeds \
             an instance at chosen axis values"
        ),
    ))
}

impl PdfDocument {
    /// Creates one instance of a variable font as a static font and registers it, like
    /// [`PdfDocument::add_font`].
    ///
    /// `bytes` is the variable font file, `face_index` selects the face of a font collection (0
    /// for a plain `.ttf` or `.otf`) and `settings` gives the axis values; an axis without a value
    /// stays at its default and a value outside the range of the axis is clamped (see
    /// [`instantiate_variable_font_bytes`]). The registered font is the static instance, so each
    /// call adds one font: two calls with different settings give two fonts that both embed.
    /// Requires the `text_layout` feature.
    ///
    /// # Errors
    ///
    /// Everything that [`instantiate_variable_font_bytes`] reports, and
    /// [`VariableFontError::StaticFontParse`] when the instance does not parse. The document
    /// is unchanged then.
    ///
    /// # Examples
    ///
    /// ```
    /// use printpdf::*;
    /// # let font_bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/variable-fonts/RedHatMono-VariableFont_wght.ttf"));
    /// let axes = font_variation_axes(font_bytes, 0).unwrap();
    /// assert_eq!(axes[0].tag, FontVariationTag::WGHT); // a weight axis from 300 to 700
    ///
    /// let mut doc = PdfDocument::new("Weights");
    /// let bold = FontVariationSettings::new().with(FontVariationTag::WGHT, axes[0].max);
    /// let font_id = doc.add_variable_font(font_bytes, 0, &bold).unwrap();
    ///
    /// let ops = vec![
    ///     Op::StartTextSection,
    ///     Op::SetTextCursor { pos: Point::new(Mm(10.0), Mm(280.0)) },
    ///     Op::SetFont { font: PdfFontHandle::External(font_id), size: Pt(16.0) },
    ///     Op::ShowText { items: vec![TextItem::Text("Bold".to_string())] },
    ///     Op::EndTextSection,
    /// ];
    /// doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
    /// let pdf = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
    /// assert!(pdf.starts_with(b"%PDF"));
    /// ```
    pub fn add_variable_font(
        &mut self,
        bytes: &[u8],
        face_index: usize,
        settings: &FontVariationSettings,
    ) -> Result<FontId, VariableFontError> {
        let (static_bytes, _) = instantiate_variable_font_bytes(bytes, face_index, settings)?;
        let mut warnings = Vec::new();
        let font = ParsedFont::from_bytes(&static_bytes, 0, &mut warnings).ok_or_else(|| {
            let reasons: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
            VariableFontError::StaticFontParse(reasons.join("; "))
        })?;
        Ok(self.add_font(&font))
    }
}
