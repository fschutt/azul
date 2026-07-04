#![deny(missing_docs)]

//! Generate a HTML font specimen sheet.
//!
//! The [font specimen](https://en.wikipedia.org/wiki/Font_catalog) provides sample text set
//! in a variety of sizes as well as details about the font like its glyph coverage, layout
//! features, style, and type.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fmt::Write;

use features::REGISTERED_FEATURES;
use langsys::LANGSYS;
use scripts::SCRIPTS;

use crate::big5::{big5_to_unicode, unicode_to_big5};
use crate::binary::read::ReadScope;
use crate::error::{ParseError, ReadWriteError, WriteError};
use crate::font::{find_good_cmap_subtable, Encoding};
use crate::font_data::{DynamicFontTableProvider, FontData};
use crate::layout::{LangSys, LayoutTable, GPOS, GSUB};
use crate::macroman::{char_to_macroman, macroman_to_char};
use crate::tables::cmap::{Cmap, CmapSubtable};
use crate::tables::os2::Os2;
use crate::tables::variable_fonts::fvar::FvarTable;
use crate::tables::{
    FontTableProvider, MaxpTable, NameTable, OpenTypeData, OpenTypeFont, SfntVersion,
};
use crate::tag::DisplayTag;
use crate::{tables, tag, SafeFrom};

mod features;
mod langsys;
mod scripts;

const HEAD_TEMPLATE: &str = include_str!("font_specimen/head.html");
const BODY_TEMPLATE: &str = include_str!("font_specimen/body.html");
const SAMPLE_TEXT: &str = "How razorback-jumping frogs can level six piqued gymnasts!";
const SAMPLE_UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUMWXYZ";
const SAMPLE_LOWERCASE: &str = "abcdefghijklmnopqrstumwxyz";
const SAMPLE_DIGITS_SYMS: &str = "0123456789.:,;(*!?')";

// https://learn.microsoft.com/en-us/typography/opentype/spec/dvaraxisreg
const REGISTERED_AXES: &[(u32, &str)] = &[
    (tag::ITAL, "Italic"),
    (tag::OPSZ, "Optical size"),
    (tag::SLNT, "Slant"),
    (tag::WDTH, "Width"),
    (tag::WGHT, "Weight"),
];

/// Options for controlling the generation of a font specimen.
#[derive(Debug, Default)]
pub struct SpecimenOptions {
    /// The index of the font within a collection (TTC).
    pub index: u32,
    /// The sample text to use in the font specimen.
    ///
    /// If not supplied some default text will be used.
    pub sample_text: Option<String>,
}

#[derive(Default)]
struct Seen {
    scripts: HashSet<u32>,
    langs: HashSet<u32>,
    features: HashSet<u32>,
}

#[derive(Default)]
struct LayoutInfo {
    features: Vec<String>,
    scripts: Vec<String>,
    langs: Vec<String>,
}

struct TagNames {
    axes: HashMap<u32, &'static str>,
    features: HashMap<u32, &'static str>,
    scripts: HashMap<u32, &'static str>,
    langsys: HashMap<u32, &'static str>,
}

/// Error produced when generating a font specimen.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum SpecimenError {
    /// A parsing error occurred.
    Read(ParseError),
    /// A write error occurred.
    ///
    /// Encountered when handling WOFF2
    Write(WriteError),
    /// A template error occurred.
    ///
    /// The font specimen HTML is generated using a template. This error is returned if
    /// an error is encountered rendering the template. This should not normally happen.
    Template(String),
}

/// Generate a font specimen as HTML.
///
/// The [font specimen](https://en.wikipedia.org/wiki/Font_catalog) provides sample text set
/// in a variety of sizes as well as details about the font like its glyph coverage, layout
/// features, style, and type.
///
/// Arguments:
///
/// - `font_src` — the path or URL to the font file. This is used in the `src` attribute
///   of the `@font-face` rule in the generated CSS.
/// - `font_data` — the binary font data. This can be any format supported by Allsorts,
///   such as TrueType, CFF, WOFF, TTC.
/// - `options` — options to control the generation of the specimen. Use
///   `SpecimenOptions::default` for default options.
///
/// The `specimen` function returns a tuple of HTML content for the `<head>` element,
/// and HTML `<body>` element. They can be used to construct a complete HTML document.
/// They are returned separately to allow embedding in other document or customisation.
///
/// ### Example
///
/// ```
/// use allsorts::error::ParseError;
/// use allsorts::font_specimen::{self, SpecimenError, SpecimenOptions};
/// use std::fs;
/// use std::path::Path;
///
/// fn main() -> Result<(), SpecimenError> {
///     let path = Path::new(env!("CARGO_MANIFEST_DIR"))
///         .join("tests/font_specimen/fonts/SourceSans3.abc.otf");
///     let data = fs::read(&path).map_err(|_err| ParseError::BadValue)?;
///     let options = SpecimenOptions::default();
///
///     let (head, body) = font_specimen::specimen(
///         "tests/font_specimen/fonts/SourceSans3.abc.otf",
///         &data,
///         options,
///     )?;
///     println!(
///         r#"<!DOCTYPE html>
/// <html lang="en">
/// <head>
///     {head}
/// </head>
/// <body>
///     {body}
///     <footer style="text-align: center">
///         <img src="https://github.com/yeslogic/allsorts/raw/master/allsorts.svg?sanitize=1" width="32" style="vertical-align: middle" alt="">
///         Generated with <a href="https://github.com/yeslogic/allsorts">Allsorts</a>.
///     </footer>
/// </body>
/// </html>"#
///     );
///     Ok(())
/// }
/// ```
pub fn specimen(
    font_src: &str,
    font_data: &[u8],
    options: SpecimenOptions,
) -> Result<(String, String), SpecimenError> {
    // Read the font
    let scope = ReadScope::new(font_data);
    let font = scope.read::<FontData<'_>>()?;

    // Extract info from the font
    let tag_names = TagNames {
        axes: REGISTERED_AXES.iter().copied().collect::<HashMap<_, _>>(),
        features: REGISTERED_FEATURES
            .iter()
            .copied()
            .collect::<HashMap<_, _>>(),
        scripts: SCRIPTS.iter().copied().collect::<HashMap<_, _>>(),
        langsys: LANGSYS.iter().copied().collect::<HashMap<_, _>>(),
    };

    let provider = font.table_provider(usize::safe_from(options.index))?;

    let cmap_data = provider.read_table_data(tag::CMAP)?;
    let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>()?;

    let (cmap_subtable_encoding, cmap_subtable_offset) = find_good_cmap_subtable(&cmap)
        .map(|(encoding, encoding_record)| (encoding, encoding_record.offset))
        .ok_or(ParseError::UnsuitableCmap)?;
    let cmap_subtable = ReadScope::new(&cmap_data[usize::safe_from(cmap_subtable_offset)..])
        .read::<CmapSubtable<'_>>()?;

    let name_data = provider.read_table_data(tag::NAME)?;
    let name = ReadScope::new(&name_data).read::<NameTable<'_>>()?;

    let maxp_data = provider.read_table_data(tag::MAXP)?;
    let maxp = ReadScope::new(&maxp_data).read::<MaxpTable>()?;

    let fvar_data = provider.table_data(tag::FVAR)?;
    let fvar = fvar_data
        .as_ref()
        .map(|fvar_data| ReadScope::new(fvar_data).read::<FvarTable<'_>>())
        .transpose()?;

    let gsub_data = provider.table_data(tag::GSUB)?;
    let gsub = gsub_data
        .as_ref()
        .map(|gsub_data| ReadScope::new(gsub_data).read::<LayoutTable<GSUB>>())
        .transpose()?;
    let gpos_data = provider.table_data(tag::GPOS)?;
    let gpos = gpos_data
        .as_ref()
        .map(|gpos_data| ReadScope::new(gpos_data).read::<LayoutTable<GPOS>>())
        .transpose()?;

    let os2_data = provider.read_table_data(tag::OS_2)?;
    let os2 = ReadScope::new(&os2_data).read_dep::<Os2>(os2_data.len())?;

    let family_name = name
        .string_for_id(NameTable::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| name.string_for_id(NameTable::FONT_FAMILY_NAME));
    let subfamily_name = name
        .string_for_id(NameTable::TYPOGRAPHIC_SUBFAMILY_NAME)
        .or_else(|| name.string_for_id(NameTable::FONT_SUBFAMILY_NAME));

    // Sample text
    let mut sample_text = options
        .sample_text
        .or_else(|| name.string_for_id(NameTable::SAMPLE_TEXT))
        .or_else(|| any_string_for_id(&name, NameTable::SAMPLE_TEXT))
        .unwrap_or(SAMPLE_TEXT.to_string());

    // Check that the font can render the sample text.
    if !font_has_glyphs_for_text(
        &cmap_subtable,
        cmap_subtable_encoding,
        os2.us_first_char_index,
        &sample_text,
    ) {
        // If not then pull some glyphs from it instead.
        sample_text = sample_text_from_available_glyphs(
            &cmap_subtable,
            cmap_subtable_encoding,
            os2.us_first_char_index,
        )
        .unwrap_or_default();
    }
    let sample_uppercase = font_has_glyphs_for_text(
        &cmap_subtable,
        cmap_subtable_encoding,
        os2.us_first_char_index,
        SAMPLE_UPPERCASE,
    )
    .then_some(SAMPLE_UPPERCASE);
    let sample_lowercase = font_has_glyphs_for_text(
        &cmap_subtable,
        cmap_subtable_encoding,
        os2.us_first_char_index,
        SAMPLE_LOWERCASE,
    )
    .then_some(SAMPLE_LOWERCASE);
    let sample_digits_syms = font_has_glyphs_for_text(
        &cmap_subtable,
        cmap_subtable_encoding,
        os2.us_first_char_index,
        SAMPLE_DIGITS_SYMS,
    )
    .then_some(SAMPLE_DIGITS_SYMS);

    let mut font_type = font_type(&provider).to_string();
    if provider.has_table(tag::CFF) {
        font_type.push_str("; CFF")
    } else if provider.has_table(tag::CFF2) {
        font_type.push_str("; CFF2")
    }
    match font {
        FontData::OpenType(OpenTypeFont {
            data: OpenTypeData::Single(_),
            ..
        }) => {}
        FontData::OpenType(OpenTypeFont {
            data: OpenTypeData::Collection(ttc),
            ..
        }) => {
            font_type.push_str(&format!(
                " collection ({} of {})",
                options.index + 1,
                ttc.offset_tables.len()
            ));
        }
        FontData::Woff(_) => {
            font_type.push_str(" (WOFF)");
        }
        FontData::Woff2(_) => {
            font_type.push_str(" (WOFF2)");
        }
    };
    let version = name
        .string_for_id(NameTable::VERSION_STRING)
        .unwrap_or_default();
    let copyright = name
        .string_for_id(NameTable::COPYRIGHT_NOTICE)
        .unwrap_or_default();
    let manufacturer = name
        .string_for_id(NameTable::MANUFACTURER_NAME)
        .unwrap_or_default();
    let designer = name.string_for_id(NameTable::DESIGNER).unwrap_or_default();
    let license = name
        .string_for_id(NameTable::LICENSE_DESCRIPTION)
        .unwrap_or_default();
    let license_url = name
        .string_for_id(NameTable::LICENSE_INFO_URL)
        .unwrap_or_default();

    // Glyph info
    let colour_glyphs = [tag::CBLC, tag::SBIX, tag::SVG, tag::COLR]
        .iter()
        .any(|tag| provider.has_table(*tag));
    let glyph_count = maxp.num_glyphs;

    // Variable font info
    let (variation_axes, variation_instances) = fvar
        .map(|fvar| {
            let axes = fvar
                .axes()
                .map(|axis| {
                    let axis_name = name
                        .string_for_id(axis.axis_name_id)
                        .or_else(|| {
                            tag_names
                                .axes
                                .get(&axis.axis_tag)
                                .map(|name| name.to_string())
                        })
                        .unwrap_or_else(|| DisplayTag(axis.axis_tag).to_string());
                    format!(
                        "{axis_name} {}–{}, default {}",
                        f32::from(axis.min_value),
                        f32::from(axis.max_value),
                        f32::from(axis.default_value)
                    )
                })
                .collect::<Vec<_>>();

            let instances = fvar
                .instances()
                .filter_map(|instance| {
                    let instance = instance.ok()?;
                    name.string_for_id(instance.subfamily_name_id)
                })
                .collect::<Vec<_>>();
            (axes, instances)
        })
        .unwrap_or_default();

    // Layout info
    let mut seen = Seen::default();
    let mut layout_info = LayoutInfo::default();
    if let Some(gsub) = gsub {
        let mut gsub_features = layout_feature_names(&gsub, &tag_names, &mut seen);
        layout_info.append(&mut gsub_features);
    }
    if let Some(gpos) = gpos {
        let mut gpos_features = layout_feature_names(&gpos, &tag_names, &mut seen);
        layout_info.append(&mut gpos_features);
    }

    // Unicode coverage
    let mut blocks = BTreeMap::new();
    let to_unicode = to_unicode_fn(cmap_subtable_encoding);
    cmap_subtable.mappings_fn(|char_code, _glyph_id| {
        // Convert the char code to unicode
        let ch = to_unicode(char_code, os2.us_first_char_index);

        // See what Unicode block it belongs to
        if let Some(block) = ch.and_then(yeslogic_unicode_blocks::find_unicode_block) {
            *blocks.entry(block.name()).or_insert(0u32) += 1;
        }
    })?;

    // Render
    let mut engine = upon::Engine::new();

    // Overrides the default formatter, so that by default all values are
    // escaped for HTML.
    engine.set_default_formatter(&escape_html);

    // Adds a custom formatter that can be manually specified when rendering
    // in order to not escape anything.
    engine.add_formatter("unescape", upon::fmt::default);

    engine
        .add_template("head", HEAD_TEMPLATE)
        .expect("invalid template");
    engine
        .add_template("body", BODY_TEMPLATE)
        .expect("invalid template");

    let map = IntoIterator::into_iter([
        ("font_src", upon::Value::from(font_src)),
        ("family_name", upon::Value::from(family_name)),
        ("subfamily_name", upon::Value::from(subfamily_name)),
        ("sample_text", upon::Value::from(sample_text)),
        ("sample_lowercase", upon::Value::from(sample_lowercase)),
        ("sample_uppercase", upon::Value::from(sample_uppercase)),
        ("sample_digits_syms", upon::Value::from(sample_digits_syms)),
        ("font_type", upon::Value::from(font_type)),
        ("version", upon::Value::from(version)),
        ("copyright", upon::Value::from(copyright)),
        ("manufacturer", upon::Value::from(manufacturer)),
        ("designer", upon::Value::from(designer)),
        ("license", upon::Value::from(license)),
        ("license_url", upon::Value::from(license_url)),
        ("glyph_count", upon::Value::from(glyph_count)),
        ("colour_glyphs", upon::Value::from(colour_glyphs)),
        ("variation_axes", upon::Value::from(variation_axes)),
        (
            "variation_instances",
            upon::Value::from(variation_instances),
        ),
        ("layout_info", upon::Value::from(layout_info)),
        ("unicode_blocks", upon::Value::from(blocks)),
    ])
    .map(|(key, value)| (key.to_string(), value))
    .collect::<BTreeMap<_, _>>();

    let context = upon::Value::Map(map);
    let head = engine.template("head").render_from(&context).to_string()?;
    let body = engine.template("body").render_from(&context).to_string()?;

    Ok((head, body))
}

fn layout_feature_names<T>(
    layout_table: &LayoutTable<T>,
    tag_names: &TagNames,
    seen: &mut Seen,
) -> LayoutInfo {
    let mut info = LayoutInfo::default();
    if let Some(script_list) = &layout_table.opt_script_list {
        for script_record in script_list.script_records() {
            let script_table = script_record.script_table();
            if seen.scripts.insert(script_record.script_tag) {
                let name = tag_names
                    .scripts
                    .get(&script_record.script_tag)
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| DisplayTag(script_record.script_tag).to_string());
                info.scripts.push(name);
            }

            if let Some(default_langsys) = script_table.default_langsys_record() {
                if seen.langs.insert(tag::DFLT) {
                    info.langs.push("Default".to_string());
                }
                add_features(
                    layout_table,
                    default_langsys,
                    &tag_names.features,
                    seen,
                    &mut info.features,
                );
            }
            for langsys in script_table.langsys_records() {
                add_langsys(langsys.langsys_tag, tag_names, seen, &mut info);
                add_features(
                    layout_table,
                    langsys.langsys_table(),
                    &tag_names.features,
                    seen,
                    &mut info.features,
                );
            }
        }
    }
    info
}

fn add_langsys(lang_sys_tag: u32, tag_names: &TagNames, seen: &mut Seen, info: &mut LayoutInfo) {
    if seen.langs.insert(lang_sys_tag) {
        let name = tag_names
            .langsys
            .get(&lang_sys_tag)
            .map(|name| name.to_string())
            .unwrap_or_else(|| DisplayTag(lang_sys_tag).to_string());
        info.langs.push(name);
    }
}

fn add_features<T>(
    layout_table: &LayoutTable<T>,
    langsys: &LangSys,
    registered_features: &HashMap<u32, &str>,
    seen: &mut Seen,
    features: &mut Vec<String>,
) {
    for feature_index in langsys.feature_indices_iter() {
        let Ok(feature_record) = layout_table.feature_by_index(*feature_index) else {
            continue;
        };
        let tag = feature_record.feature_tag;
        if seen.features.insert(tag) {
            let name = registered_features
                .get(&tag)
                .map(|name| name.to_string())
                .unwrap_or_else(|| match character_variant(tag) {
                    Some(num) => format!("Character Variants {num}"),
                    None => DisplayTag(tag).to_string(),
                });
            features.push(name);
        }
    }
}

/// See if the tag matches /cv\d\d/ and return the digits if it does
fn character_variant(tag: u32) -> Option<u8> {
    match tag.to_be_bytes() {
        [b'c', b'v', a, b] if a.is_ascii_digit() && b.is_ascii_digit() => {
            Some(((a - b'0') * 10) + (b - b'0'))
        }
        _ => None,
    }
}

fn font_type(font: &DynamicFontTableProvider<'_>) -> &'static str {
    match font.sfnt_version() {
        tables::TTF_MAGIC | tables::TRUE_MAGIC => "TrueType",
        tables::CFF_MAGIC => "OpenType",
        _ => "Unknown",
    }
}

fn font_has_glyphs_for_text(
    cmap_subtable: &CmapSubtable<'_>,
    encoding: Encoding,
    first_char: u16,
    text: &str,
) -> bool {
    let from_unicode = match encoding {
        Encoding::Unicode => |unicode, _| Some(unicode as u32),
        Encoding::Symbol => |unicode, first_char| {
            let char_code0 = if !('\u{F000}'..='\u{F0FF}').contains(&unicode) {
                unicode as u32
            } else {
                unicode as u32 - 0xF000
            };
            Some((char_code0 + u32::from(first_char)) - 0x20) // Perform subtraction last to avoid underflow.
        },
        Encoding::AppleRoman => |unicode, _| char_to_macroman(unicode).map(u32::from),
        Encoding::Big5 => |unicode, _| unicode_to_big5(unicode).map(u32::from),
    };

    text.chars().all(|unicode| {
        from_unicode(unicode, first_char)
            .and_then(|ch| cmap_subtable.map_glyph(ch).ok().flatten())
            .is_some()
    })
}

const SAMPLE_CHARS: usize = 26;

fn sample_text_from_available_glyphs(
    cmap_subtable: &CmapSubtable<'_>,
    encoding: Encoding,
    first_char: u16,
) -> Option<String> {
    let mappings = cmap_subtable.mappings().ok()?;
    // Fairly arbitrary threshold
    if mappings.len() < SAMPLE_CHARS * 4 {
        first_n_glyphs_sample_text(encoding, first_char, mappings)
    } else {
        semi_random_sample_text(encoding, first_char, mappings)
    }
}

fn first_n_glyphs_sample_text(
    encoding: Encoding,
    first_char: u16,
    mappings: HashMap<u16, u32>,
) -> Option<String> {
    let to_unicode = to_unicode_fn(encoding);
    let mappings = mappings.into_iter().collect::<BTreeMap<_, _>>();
    let text = mappings
        .iter()
        .filter_map(|(&gid, &ch)| {
            if gid != 0 {
                to_unicode(ch, first_char).filter(|&ch| ch != '\0')
            } else {
                None
            }
        })
        .take(SAMPLE_CHARS)
        .collect::<String>();

    Some(text)
}

fn semi_random_sample_text(
    encoding: Encoding,
    first_char: u16,
    mappings: HashMap<u16, u32>,
) -> Option<String> {
    let num_mappings = mappings.len() as u16; // FIXME
    let to_unicode = to_unicode_fn(encoding);

    let mut gid = num_mappings;
    let gids = std::iter::from_fn(|| {
        gid = rand(gid);
        Some(gid)
    });

    let text = gids
        .take(SAMPLE_CHARS)
        .filter_map(|gid| {
            let gid = gid % num_mappings;
            if gid != 0 {
                mappings
                    .get(&gid)
                    .and_then(|&ch| to_unicode(ch, first_char).filter(|&ch| ch != '\0'))
            } else {
                None
            }
        })
        .collect::<String>();

    Some(text)
}

fn to_unicode_fn(encoding: Encoding) -> fn(u32, u16) -> Option<char> {
    match encoding {
        Encoding::Unicode => |ch, _| std::char::from_u32(ch),
        Encoding::Symbol => |ch, first_char| {
            // The symbol encoding was created to support fonts with arbitrary ornaments or symbols not
            // supported in Unicode or other standard encodings. A format 4 subtable would be used,
            // typically with up to 224 graphic characters assigned at code positions beginning with
            // 0xF020. This corresponds to a sub-range within the Unicode Private-Use Area (PUA), though
            // this is not a Unicode encoding. In legacy usage, some applications would represent the
            // symbol characters in text using a single-byte encoding, and then map 0x20 to the
            // OS/2.usFirstCharIndex value in the font.
            // — https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#encoding-records-and-encodings

            // fn legacy_symbol_char_code_to_unicode(ch: u32, first_char: u16) -> Option<char> {
            let char_code0 = if (0xF000..=0xF0FF).contains(&ch) {
                ch
            } else {
                ch + 0xF000
            };
            std::char::from_u32((char_code0 + 0x20) - u32::from(first_char)) // Perform subtraction last to avoid underflow.
                                                                             // }
        },
        Encoding::AppleRoman => |char_code, _| macroman_to_char(char_code as u8),
        Encoding::Big5 => |char_code, _| u16::try_from(char_code).ok().and_then(big5_to_unicode),
    }
}

/// Try to find a name table entry for the given id in any language.
fn any_string_for_id(name: &NameTable<'_>, name_id: u16) -> Option<String> {
    name.name_records
        .iter()
        .find_map(|record| {
            if record.name_id != name_id {
                return None;
            }
            match (record.platform_id, record.encoding_id) {
                // Windows Unicode BMP, English language ids
                // https://learn.microsoft.com/en-us/typography/opentype/spec/name#windows-language-ids
                (3, 1) => Some(record),
                // Windows Unicode full, English language ids
                // https://learn.microsoft.com/en-us/typography/opentype/spec/name#windows-language-ids
                (3, 10) => Some(record),
                _ => None,
            }
        })
        .and_then(|record| {
            let offset = usize::from(record.offset);
            let length = usize::from(record.length);
            let name_data = name
                .string_storage
                .offset_length(offset, length)
                .ok()?
                .data();
            Some(tables::decode(encoding_rs::UTF_16BE, name_data))
        })
}

impl LayoutInfo {
    fn append(&mut self, other: &mut LayoutInfo) {
        self.features.append(&mut other.features);
        self.scripts.append(&mut other.scripts);
        self.langs.append(&mut other.langs);
    }
}

impl From<LayoutInfo> for upon::Value {
    fn from(info: LayoutInfo) -> Self {
        upon::Value::Map(
            IntoIterator::into_iter([
                ("features", upon::Value::from(info.features)),
                ("scripts", upon::Value::from(info.scripts)),
                ("langs", upon::Value::from(info.langs)),
            ])
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
        )
    }
}

impl From<ParseError> for SpecimenError {
    fn from(err: ParseError) -> Self {
        SpecimenError::Read(err)
    }
}

impl From<ReadWriteError> for SpecimenError {
    fn from(err: ReadWriteError) -> Self {
        match err {
            ReadWriteError::Read(err) => SpecimenError::Read(err),
            ReadWriteError::Write(err) => SpecimenError::Write(err),
        }
    }
}

impl From<upon::Error> for SpecimenError {
    fn from(err: upon::Error) -> Self {
        SpecimenError::Template(err.to_string())
    }
}

impl fmt::Display for SpecimenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecimenError::Read(err) => write!(f, "read error: {}", err),
            SpecimenError::Write(err) => write!(f, "write error: {}", err),
            SpecimenError::Template(err) => write!(f, "template error: {}", err),
        }
    }
}

impl std::error::Error for SpecimenError {}

/// Custom implementation copied from [rustdoc]: https://github.com/rust-lang/rust/blob/4596f4f8b565bdd02d3b99d1ab12ff09146a93de/src/librustdoc/html/escape.rs
fn escape_html(f: &mut upon::fmt::Formatter<'_>, value: &upon::Value) -> upon::fmt::Result {
    let s = match value {
        upon::Value::String(s) => s,
        value => {
            // Fallback to default formatter
            return upon::fmt::default(f, value);
        }
    };

    let mut last = 0;
    for (i, byte) in s.bytes().enumerate() {
        match byte {
            b'<' | b'>' | b'&' | b'\'' | b'"' => {
                f.write_str(&s[last..i])?;
                let s = match byte {
                    b'>' => "&gt;",
                    b'<' => "&lt;",
                    b'&' => "&amp;",
                    b'\'' => "&#39;",
                    b'"' => "&quot;",
                    _ => unreachable!(),
                };
                f.write_str(s)?;
                last = i + 1;
            }
            _ => {}
        }
    }
    if last < s.len() {
        f.write_str(&s[last..])?;
    }
    Ok(())
}

// http://www.retroprogramming.com/2017/07/xorshift-pseudorandom-numbers-in-z80.html
fn rand(mut xs: u16) -> u16 {
    xs ^= xs << 7;
    xs ^= xs >> 9;
    xs ^= xs << 8;
    xs
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::path::Path;

    use crate::error::ReadWriteError;
    use crate::font_specimen;
    use crate::tables::cmap::{EncodingId, PlatformId};
    use crate::tests::{fixture_path, read_fixture};

    use super::*;

    fn load_font<'a>(scope: ReadScope<'a>) -> Result<DynamicFontTableProvider<'a>, ReadWriteError> {
        let font_file = scope.read::<FontData<'_>>()?;
        font_file.table_provider(0)
    }

    fn with_cmap<P: AsRef<Path>>(
        path: P,
        mut callback: impl FnMut(&CmapSubtable<'_>, Encoding, &Os2),
    ) -> Result<(), Box<dyn Error>> {
        let buffer = std::fs::read(path)?;
        let scope = ReadScope::new(&buffer);
        let provider = load_font(scope)?;

        let cmap_data = provider.read_table_data(tag::CMAP)?;
        let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>()?;

        let os2_data = provider.read_table_data(tag::OS_2)?;
        let os2 = ReadScope::new(&os2_data).read_dep::<Os2>(os2_data.len())?;

        let (cmap_subtable_encoding, cmap_subtable_offset) = find_good_cmap_subtable(&cmap)
            .map(|(encoding, encoding_record)| (encoding, encoding_record.offset))
            .ok_or(ParseError::UnsuitableCmap)?;
        let cmap_subtable = ReadScope::new(&cmap_data[usize::safe_from(cmap_subtable_offset)..])
            .read::<CmapSubtable<'_>>()?;

        callback(&cmap_subtable, cmap_subtable_encoding, &os2);

        Ok(())
    }

    #[test]
    fn test_character_variant() {
        assert_eq!(character_variant(tag!(b"cv99")), Some(99));
        assert_eq!(character_variant(tag!(b"cv01")), Some(1));
        assert_eq!(character_variant(tag!(b"othr")), None);
    }

    #[test]
    fn font_has_glyphs_unicode() {
        let path = fixture_path("tests/font_specimen/fonts/SourceSans3.abc.otf");
        with_cmap(path, |cmap, encoding, os2| {
            assert_eq!(encoding, Encoding::Unicode);
            assert!(font_has_glyphs_for_text(
                cmap,
                encoding,
                os2.us_first_char_index,
                "abc"
            ));
            assert!(!font_has_glyphs_for_text(
                cmap,
                encoding,
                os2.us_first_char_index,
                "abcd"
            )); // only has abc glyphs
        })
        .unwrap();
    }

    #[test]
    fn font_has_glyphs_macroman() -> Result<(), ParseError> {
        let cmap_data = read_fixture("tests/font_specimen/fonts/macroman.cmap");
        let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>()?;

        let encoding_record = cmap
            .find_subtable(PlatformId::MACINTOSH, EncodingId::MACINTOSH_APPLE_ROMAN)
            .unwrap();
        let cmap_subtable = cmap
            .scope
            .offset(usize::try_from(encoding_record.offset).unwrap())
            .read::<CmapSubtable<'_>>()?;

        let encoding = Encoding::AppleRoman;
        assert!(font_has_glyphs_for_text(
            &cmap_subtable,
            encoding,
            0x20,
            "abc"
        ));
        assert!(!font_has_glyphs_for_text(
            &cmap_subtable,
            encoding,
            0x20,
            "þ"
        ));
        Ok(())
    }

    #[test]
    fn font_has_glyphs_big5() -> Result<(), ParseError> {
        let cmap_data = read_fixture("tests/font_specimen/fonts/big5.cmap");
        let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>()?;

        let encoding_record = cmap
            .find_subtable(PlatformId::WINDOWS, EncodingId::WINDOWS_BIG5)
            .unwrap();
        let cmap_subtable = cmap
            .scope
            .offset(usize::try_from(encoding_record.offset).unwrap())
            .read::<CmapSubtable<'_>>()?;

        let encoding = Encoding::Big5;
        assert!(font_has_glyphs_for_text(
            &cmap_subtable,
            encoding,
            0x20,
            "丕乾"
        ));
        assert!(!font_has_glyphs_for_text(
            &cmap_subtable,
            encoding,
            0x20,
            "þ"
        ));
        Ok(())
    }

    #[test]
    fn font_has_glyphs_symbol() {
        let path = fixture_path("tests/font_specimen/fonts/SymbolTest-Regular.ttf");
        with_cmap(path, |cmap, encoding, os2| {
            assert_eq!(encoding, Encoding::Symbol);
            assert!(font_has_glyphs_for_text(
                cmap,
                encoding,
                os2.us_first_char_index,
                "\u{f061}\u{f064}\u{f020}"
            ));
            assert!(!font_has_glyphs_for_text(
                cmap,
                encoding,
                os2.us_first_char_index,
                "xyx"
            ));
        })
        .unwrap();
    }

    #[test]
    fn sample_text_unicode() {
        let path = fixture_path("tests/font_specimen/fonts/SourceSans3.abc.otf");
        with_cmap(path, |cmap, encoding, os2| {
            assert_eq!(encoding, Encoding::Unicode);
            let sample_text =
                sample_text_from_available_glyphs(cmap, encoding, os2.us_first_char_index);
            assert_eq!(sample_text.as_deref(), Some("abc"));
        })
        .unwrap();
    }

    #[test]
    fn sample_text_symbol() {
        let path = fixture_path("tests/font_specimen/fonts/SymbolTest-Regular.ttf");
        with_cmap(path, |cmap, encoding, os2| {
            assert_eq!(encoding, Encoding::Symbol);
            let sample_text =
                sample_text_from_available_glyphs(cmap, encoding, os2.us_first_char_index);
            assert_eq!(sample_text.as_deref(), Some("abcd "));
        })
        .unwrap();
    }

    #[test]
    fn source_sans_3_specimen() {
        let data = read_fixture("tests/font_specimen/fonts/SourceSans3.abc.otf");
        let options = SpecimenOptions::default();

        let expected_head =
            fs::read_to_string("tests/font_specimen/SourceSans3.abc.otf.head.html").unwrap();
        let expected_body =
            fs::read_to_string("tests/font_specimen/SourceSans3.abc.otf.body.html").unwrap();

        let (head, body) = font_specimen::specimen(
            "tests/font_specimen/fonts/SourceSans3.abc.otf",
            &data,
            options,
        )
        .expect("error generating specimen");

        assert_eq!(head.trim(), expected_head.trim());
        assert_eq!(body.trim(), expected_body.trim());
    }
}
