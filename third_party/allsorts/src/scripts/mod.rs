pub mod arabic;
pub mod indic;
pub mod khmer;
pub mod myanmar;
mod syllable;
pub mod syriac;
pub mod thai_lao;

use crate::glyph_position::TextDirection;
use crate::gsub::{GlyphOrigin, RawGlyph};
use crate::scripts::syllable::SyllableChar;
use crate::tag;
use crate::unicode::mcc::sort_by_modified_combining_class;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptType {
    Arabic,
    Default,
    Indic,
    Khmer,
    Myanmar,
    Syriac,
    ThaiLao,
}

impl From<u32> for ScriptType {
    fn from(script_tag: u32) -> Self {
        match script_tag {
            tag::ARAB => ScriptType::Arabic,
            tag::LATN => ScriptType::Default,
            tag::CYRL => ScriptType::Default,
            tag::GREK => ScriptType::Default,
            tag::DEVA => ScriptType::Indic,
            tag::BENG => ScriptType::Indic,
            tag::GURU => ScriptType::Indic,
            tag::GUJR => ScriptType::Indic,
            tag::ORYA => ScriptType::Indic,
            tag::TAML => ScriptType::Indic,
            tag::TELU => ScriptType::Indic,
            tag::KNDA => ScriptType::Indic,
            tag::MLYM => ScriptType::Indic,
            tag::SINH => ScriptType::Indic,
            tag::KHMR => ScriptType::Khmer,
            tag::MYMR => ScriptType::Myanmar,
            tag::MYM2 => ScriptType::Myanmar,
            tag::SYRC => ScriptType::Syriac,
            tag::THAI => ScriptType::ThaiLao,
            tag::LAO => ScriptType::ThaiLao,
            _ => ScriptType::Default,
        }
    }
}

impl<T> SyllableChar for RawGlyph<T> {
    fn char(&self) -> char {
        match self.glyph_origin {
            GlyphOrigin::Char(ch) => ch,
            GlyphOrigin::Direct => panic!("unexpected glyph origin"),
        }
    }
}

pub fn preprocess_text(cs: &mut Vec<char>, script_tag: u32) {
    match ScriptType::from(script_tag) {
        ScriptType::Arabic => arabic::reorder_marks(cs),
        ScriptType::Default => sort_by_modified_combining_class(cs),
        ScriptType::Indic => indic::preprocess_indic(cs, script_tag),
        ScriptType::Khmer => khmer::preprocess_khmer(cs),
        ScriptType::Myanmar => {}
        ScriptType::Syriac => sort_by_modified_combining_class(cs),
        ScriptType::ThaiLao => thai_lao::reorder_marks(cs),
    }
}

mod rtl_tags {
    use crate::tag;

    // Unicode 1.1
    pub const ARAB: u32 = tag!(b"arab"); // Arabic
    pub const HEBR: u32 = tag!(b"hebr"); // Hebrew

    // Unicode 3.0
    pub const SYRC: u32 = tag!(b"syrc"); // Syriac
    pub const THAA: u32 = tag!(b"thaa"); // Thaana

    // Unicode 4.0
    pub const CPRT: u32 = tag!(b"cprt"); // Cypriot Syllabary

    // Unicode 4.1
    pub const KHAR: u32 = tag!(b"khar"); // Kharosthi

    // Unicode 5.0
    pub const PHNX: u32 = tag!(b"phnx"); // Phoenician
    pub const NKO: u32 = tag!(b"nko "); // N'Ko

    // Unicode 5.1
    pub const LYDI: u32 = tag!(b"lydi"); // Lydian

    // Unicode 5.2
    pub const AVST: u32 = tag!(b"avst"); // Avestan
    pub const ARMI: u32 = tag!(b"armi"); // Imperial Aramaic
    pub const PHLI: u32 = tag!(b"phli"); // Inscriptional Pahlavi
    pub const PRTI: u32 = tag!(b"prti"); // Inscriptional Parthian
    pub const SARB: u32 = tag!(b"sarb"); // Old South Arabian
    pub const ORKH: u32 = tag!(b"orkh"); // Old Turkic, Orkhon Runic
    pub const SAMR: u32 = tag!(b"samr"); // Samaritan

    // Unicode 6.0
    pub const MAND: u32 = tag!(b"mand"); // Mandaic, Mandaean

    // Unicode 6.1
    pub const MERC: u32 = tag!(b"merc"); // Meroitic Cursive
    pub const MERO: u32 = tag!(b"mero"); // Meroitic Hieroglyphs

    // Unicode 7.0
    pub const MANI: u32 = tag!(b"mani"); // Manichaean
    pub const MEND: u32 = tag!(b"mend"); // Mende Kikakui
    pub const NBAT: u32 = tag!(b"nbat"); // Nabataean
    pub const NARB: u32 = tag!(b"narb"); // Old North Arabian
    pub const PALM: u32 = tag!(b"palm"); // Palmyrene
    pub const PHLP: u32 = tag!(b"phlp"); // Psalter Pahlavi

    // Unicode 8.0
    pub const HATR: u32 = tag!(b"hatr"); // Hatran

    // Unicode 9.0
    pub const ADLM: u32 = tag!(b"adlm"); // Adlam

    // Unicode 11.0
    pub const ROHG: u32 = tag!(b"rohg"); // Hanifi Rohingya
    pub const SOGO: u32 = tag!(b"sogo"); // Old Sogdian
    pub const SOGD: u32 = tag!(b"sogd"); // Sogdian

    // Unicode 12.0
    pub const ELYM: u32 = tag!(b"elym"); // Elymaic

    // Unicode 13.0
    pub const CHRS: u32 = tag!(b"chrs"); // Chorasmian
    pub const YEZI: u32 = tag!(b"yezi"); // Yezidi

    // Unicode 14.0
    pub const OUGR: u32 = tag!(b"ougr"); // Old Uyghur

    // Unicode 16.0
    pub const GARA: u32 = tag!(b"gara"); // Garay
}

pub fn horizontal_text_direction(script: u32) -> TextDirection {
    use rtl_tags as rtl;

    // Derived from https://github.com/harfbuzz/harfbuzz/blob/bdee8658c68cf400e266c91039d741b5047c2519/src/hb-common.cc#L556-L644
    // License: MIT
    // Copyright (c) 2009, 2010 Red Hat, Inc.
    // Copyright (c) 2011, 2012 Google, Inc.
    match script {
        // Unicode 1.1
        | rtl::ARAB // Arabic
        | rtl::HEBR // Hebrew

        // Unicode 3.0
        | rtl::SYRC // Syriac
        | rtl::THAA // Thaana

        // Unicode 4.0
        | rtl::CPRT // Cypriot Syllabary

        // Unicode 4.1
        | rtl::KHAR // Kharosthi

        // Unicode 5.0
        | rtl::PHNX // Phoenician
        | rtl::NKO  // N'Ko

        // Unicode 5.1
        | rtl::LYDI // Lydian

        // Unicode 5.2
        | rtl::AVST // Avestan
        | rtl::ARMI // Imperial Aramaic
        | rtl::PHLI // Inscriptional Pahlavi
        | rtl::PRTI // Inscriptional Parthian
        | rtl::SARB // Old South Arabian
        | rtl::ORKH // Old Turkic, Orkhon Runic
        | rtl::SAMR // Samaritan

        // Unicode 6.0
        | rtl::MAND // Mandaic, Mandaean

        // Unicode 6.1
        | rtl::MERC // Meroitic Cursive
        | rtl::MERO // Meroitic Hieroglyphs

        // Unicode 7.0
        | rtl::MANI // Manichaean
        | rtl::MEND // Mende Kikakui
        | rtl::NBAT // Nabataean
        | rtl::NARB // Old North Arabian
        | rtl::PALM // Palmyrene
        | rtl::PHLP // Psalter Pahlavi

        // Unicode 8.0
        | rtl::HATR // Hatran

        // Unicode 9.0
        | rtl::ADLM // Adlam

        // Unicode 11.0
        | rtl::ROHG // Hanifi Rohingya
        | rtl::SOGO // Old Sogdian
        | rtl::SOGD // Sogdian

        // Unicode 12.0
        | rtl::ELYM // Elymaic

        // Unicode 13.0
        | rtl::CHRS // Chorasmian
        | rtl::YEZI // Yezidi

        // Unicode 14.0
        | rtl::OUGR // Old Uyghur

        // Unicode 16.0
        | rtl::GARA => TextDirection::RightToLeft, // Garay

        _ => TextDirection::LeftToRight,
    }
}
