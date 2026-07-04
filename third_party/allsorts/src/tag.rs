//! Utilities and constants for OpenType tags.
//!
//! See also the [`tag!`](../macro.tag.html) macro for creating tags from a byte string.

use crate::error::ParseError;
use std::{fmt, iter, str};

/// Generate a 4-byte OpenType tag from byte string
///
/// Example:
///
/// ```
/// use allsorts::tag;
/// assert_eq!(tag!(b"glyf"), 0x676C7966);
/// ```
#[macro_export]
macro_rules! tag {
    ($w:expr) => {
        u32::from_be_bytes(*$w)
    };
}

/// Wrapper type for a tag that implements `Display`
///
/// Example:
///
/// ```
/// use allsorts::tag::{self, DisplayTag};
///
/// // ASCII tag comes out as a string
/// assert_eq!(&DisplayTag(tag::NAME).to_string(), "name");
/// // Non-ASCII tag comes out as hex
/// assert_eq!(&DisplayTag(0x12345678).to_string(), "0x12345678");
///
/// println!(
///     "DisplayTag is handy for printing a tag: '{}'",
///     DisplayTag(tag::CFF)
/// );
/// ```
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct DisplayTag(pub u32);

/// Build a `u32` tag from a string.
///
/// If the supplied string is less than 4 characters it will be padded with spaces.
pub fn from_string(s: &str) -> Result<u32, ParseError> {
    if s.len() > 4 {
        return Err(ParseError::BadValue);
    }

    let mut tag: u32 = 0;
    for c in s.chars().chain(iter::repeat(' ')).take(4) {
        if !c.is_ascii() || c.is_ascii_control() {
            return Err(ParseError::BadValue);
        }

        tag = (tag << 8) | (c as u32);
    }

    Ok(tag)
}

impl fmt::Display for DisplayTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = self.0;
        let bytes = tag.to_be_bytes();
        if bytes.iter().all(|c| c.is_ascii() && !c.is_ascii_control()) {
            let s = str::from_utf8(&bytes).unwrap(); // unwrap safe due to above check
            s.fmt(f)
        } else {
            write!(f, "0x{:08x}", tag)
        }
    }
}

impl fmt::Debug for DisplayTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// `abvf`
pub const ABVF: u32 = tag!(b"abvf");
/// `abvm`
pub const ABVM: u32 = tag!(b"abvm");
/// `abvs`
pub const ABVS: u32 = tag!(b"abvs");
/// `acnt`
pub const ACNT: u32 = tag!(b"acnt");
/// `afrc`
pub const AFRC: u32 = tag!(b"afrc");
/// `akhn`
pub const AKHN: u32 = tag!(b"akhn");
/// `arab`
pub const ARAB: u32 = tag!(b"arab");
/// `avar`
pub const AVAR: u32 = tag!(b"avar");
/// `BASE`
pub const BASE: u32 = tag!(b"BASE");
/// `bdat`
pub const BDAT: u32 = tag!(b"bdat");
/// `beng`
pub const BENG: u32 = tag!(b"beng");
/// `bloc`
pub const BLOC: u32 = tag!(b"bloc");
/// `blwf`
pub const BLWF: u32 = tag!(b"blwf");
/// `blwm`
pub const BLWM: u32 = tag!(b"blwm");
/// `blws`
pub const BLWS: u32 = tag!(b"blws");
/// `bng2`
pub const BNG2: u32 = tag!(b"bng2");
/// `bsln`
pub const BSLN: u32 = tag!(b"bsln");
/// `c2sc`
pub const C2SC: u32 = tag!(b"c2sc");
/// `calt`
pub const CALT: u32 = tag!(b"calt");
/// `CBDT`
pub const CBDT: u32 = tag!(b"CBDT");
/// `CBLC`
pub const CBLC: u32 = tag!(b"CBLC");
/// `ccmp`
pub const CCMP: u32 = tag!(b"ccmp");
/// `cfar`
pub const CFAR: u32 = tag!(b"cfar");
/// `CFF `
pub const CFF: u32 = tag!(b"CFF ");
/// `CFF2`
pub const CFF2: u32 = tag!(b"CFF2");
/// `cjct`
pub const CJCT: u32 = tag!(b"cjct");
/// `clig`
pub const CLIG: u32 = tag!(b"clig");
/// `cmap`
pub const CMAP: u32 = tag!(b"cmap");
/// `COLR`
pub const COLR: u32 = tag!(b"COLR");
/// `CPAL`
pub const CPAL: u32 = tag!(b"CPAL");
/// `curs`
pub const CURS: u32 = tag!(b"curs");
/// `cvar`
pub const CVAR: u32 = tag!(b"cvar");
/// `cvt `
pub const CVT: u32 = tag!(b"cvt ");
/// `cyrl`
pub const CYRL: u32 = tag!(b"cyrl");
/// `dev2`
pub const DEV2: u32 = tag!(b"dev2");
/// `deva`
pub const DEVA: u32 = tag!(b"deva");
/// `DFLT`
pub const DFLT: u32 = tag!(b"DFLT");
/// `dist`
pub const DIST: u32 = tag!(b"dist");
/// `dlig`
pub const DLIG: u32 = tag!(b"dlig");
/// `dupe`
pub const DUPE: u32 = tag!(b"dupe");
/// `EBDT`
pub const EBDT: u32 = tag!(b"EBDT");
/// `EBLC`
pub const EBLC: u32 = tag!(b"EBLC");
/// `EBSC`
pub const EBSC: u32 = tag!(b"EBSC");
/// `FAR `
pub const FAR: u32 = tag!(b"FAR ");
/// `fdsc`
pub const FDSC: u32 = tag!(b"fdsc");
/// `Feat`
pub const FEAT2: u32 = tag!(b"Feat");
/// `feat`
pub const FEAT: u32 = tag!(b"feat");
/// `fin2`
pub const FIN2: u32 = tag!(b"fin2");
/// `fin3`
pub const FIN3: u32 = tag!(b"fin3");
/// `fina`
pub const FINA: u32 = tag!(b"fina");
/// `flip`
pub const FLIP: u32 = tag!(b"flip");
/// `fmtx`
pub const FMTX: u32 = tag!(b"fmtx");
/// `fpgm`
pub const FPGM: u32 = tag!(b"fpgm");
/// `frac`
pub const FRAC: u32 = tag!(b"frac");
/// `fvar`
pub const FVAR: u32 = tag!(b"fvar");
/// `gasp`
pub const GASP: u32 = tag!(b"gasp");
/// `GDEF`
pub const GDEF: u32 = tag!(b"GDEF");
/// `gjr2`
pub const GJR2: u32 = tag!(b"gjr2");
/// `Glat`
pub const GLAT: u32 = tag!(b"Glat");
/// `Gloc`
pub const GLOC: u32 = tag!(b"Gloc");
/// `glyf`
pub const GLYF: u32 = tag!(b"glyf");
/// `GPOS`
pub const GPOS: u32 = tag!(b"GPOS");
/// `grek`
pub const GREK: u32 = tag!(b"grek");
/// `GSUB`
pub const GSUB: u32 = tag!(b"GSUB");
/// `gujr`
pub const GUJR: u32 = tag!(b"gujr");
/// `gur2`
pub const GUR2: u32 = tag!(b"gur2");
/// `guru`
pub const GURU: u32 = tag!(b"guru");
/// `gvar`
pub const GVAR: u32 = tag!(b"gvar");
/// `half`
pub const HALF: u32 = tag!(b"half");
/// `haln`
pub const HALN: u32 = tag!(b"haln");
/// `hdmx`
pub const HDMX: u32 = tag!(b"hdmx");
/// `head`
pub const HEAD: u32 = tag!(b"head");
/// `hhea`
pub const HHEA: u32 = tag!(b"hhea");
/// `hlig`
pub const HLIG: u32 = tag!(b"hlig");
/// `hmtx`
pub const HMTX: u32 = tag!(b"hmtx");
/// `hsty`
pub const HSTY: u32 = tag!(b"hsty");
/// `HVAR`
pub const HVAR: u32 = tag!(b"HVAR");
/// `init`
pub const INIT: u32 = tag!(b"init");
/// `isol`
pub const ISOL: u32 = tag!(b"isol");
/// `jpg `
pub const JPG: u32 = tag!(b"jpg ");
/// `JSTF`
pub const JSTF: u32 = tag!(b"JSTF");
/// `just`
pub const JUST: u32 = tag!(b"just");
/// `kern`
pub const KERN: u32 = tag!(b"kern");
/// `khmr`
pub const KHMR: u32 = tag!(b"khmr");
/// `knd2`
pub const KND2: u32 = tag!(b"knd2");
/// `knda`
pub const KNDA: u32 = tag!(b"knda");
/// `lao `
pub const LAO: u32 = tag!(b"lao ");
/// `latn`
pub const LATN: u32 = tag!(b"latn");
/// `lcar`
pub const LCAR: u32 = tag!(b"lcar");
/// `liga`
pub const LIGA: u32 = tag!(b"liga");
/// `lnum`
pub const LNUM: u32 = tag!(b"lnum");
/// `loca`
pub const LOCA: u32 = tag!(b"loca");
/// `locl`
pub const LOCL: u32 = tag!(b"locl");
/// `LTSH`
pub const LTSH: u32 = tag!(b"LTSH");
/// `mark`
pub const MARK: u32 = tag!(b"mark");
/// `MATH`
pub const MATH: u32 = tag!(b"MATH");
/// `maxp`
pub const MAXP: u32 = tag!(b"maxp");
/// `med2`
pub const MED2: u32 = tag!(b"med2");
/// `medi`
pub const MEDI: u32 = tag!(b"medi");
/// `mkmk`
pub const MKMK: u32 = tag!(b"mkmk");
/// `mlm2`
pub const MLM2: u32 = tag!(b"mlm2");
/// `mlym`
pub const MLYM: u32 = tag!(b"mlym");
/// `mort`
pub const MORT: u32 = tag!(b"mort");
/// `morx`
pub const MORX: u32 = tag!(b"morx");
/// `mset`
pub const MSET: u32 = tag!(b"mset");
/// `MVAR`
pub const MVAR: u32 = tag!(b"MVAR");
/// `mymr`
pub const MYMR: u32 = tag!(b"mymr");
/// `mym2`
pub const MYM2: u32 = tag!(b"mym2");
/// `name`
pub const NAME: u32 = tag!(b"name");
/// `nukt`
pub const NUKT: u32 = tag!(b"nukt");
/// `onum`
pub const ONUM: u32 = tag!(b"onum");
/// `opbd`
pub const OPBD: u32 = tag!(b"opbd");
/// `ordn`
pub const ORDN: u32 = tag!(b"ordn");
/// `ory2`
pub const ORY2: u32 = tag!(b"ory2");
/// `orya`
pub const ORYA: u32 = tag!(b"orya");
/// `OS/2`
pub const OS_2: u32 = tag!(b"OS/2");
/// `OTTO`
pub const OTTO: u32 = tag!(b"OTTO");
/// `PCLT`
pub const PCLT: u32 = tag!(b"PCLT");
/// `pnum`
pub const PNUM: u32 = tag!(b"pnum");
/// `png `
pub const PNG: u32 = tag!(b"png ");
/// `post`
pub const POST: u32 = tag!(b"post");
/// `pref`
pub const PREF: u32 = tag!(b"pref");
/// `prep`
pub const PREP: u32 = tag!(b"prep");
/// `pres`
pub const PRES: u32 = tag!(b"pres");
/// `prop`
pub const PROP: u32 = tag!(b"prop");
/// `pstf`
pub const PSTF: u32 = tag!(b"pstf");
/// `psts`
pub const PSTS: u32 = tag!(b"psts");
/// `rclt`
pub const RCLT: u32 = tag!(b"rclt");
/// `rkrf`
pub const RKRF: u32 = tag!(b"rkrf");
/// `rlig`
pub const RLIG: u32 = tag!(b"rlig");
/// `rphf`
pub const RPHF: u32 = tag!(b"rphf");
/// `rvrn`
pub const RVRN: u32 = tag!(b"rvrn");
/// `sbix`
pub const SBIX: u32 = tag!(b"sbix");
/// `Silf`
pub const SILF: u32 = tag!(b"Silf");
/// `Sill`
pub const SILL: u32 = tag!(b"Sill");
/// `sinh`
pub const SINH: u32 = tag!(b"sinh");
/// `smcp`
pub const SMCP: u32 = tag!(b"smcp");
/// `SND `
pub const SND: u32 = tag!(b"SND ");
/// `STAT`
pub const STAT: u32 = tag!(b"STAT");
/// `SVG `
pub const SVG: u32 = tag!(b"SVG ");
/// `syrc`
pub const SYRC: u32 = tag!(b"syrc");
/// `taml`
pub const TAML: u32 = tag!(b"taml");
/// `tel2`
pub const TEL2: u32 = tag!(b"tel2");
/// `telu`
pub const TELU: u32 = tag!(b"telu");
/// `thai`
pub const THAI: u32 = tag!(b"thai");
/// `tiff`
pub const TIFF: u32 = tag!(b"tiff");
/// `tml2`
pub const TML2: u32 = tag!(b"tml2");
/// `tnum`
pub const TNUM: u32 = tag!(b"tnum");
/// `trak`
pub const TRAK: u32 = tag!(b"trak");
/// `true`
pub const TRUE: u32 = tag!(b"true");
/// `ttcf`
pub const TTCF: u32 = tag!(b"ttcf");
/// `URD `
pub const URD: u32 = tag!(b"URD ");
/// `vatu`
pub const VATU: u32 = tag!(b"vatu");
/// `VDMX`
pub const VDMX: u32 = tag!(b"VDMX");
/// `vert`
pub const VERT: u32 = tag!(b"vert");
/// `vhea`
pub const VHEA: u32 = tag!(b"vhea");
/// `vmtx`
pub const VMTX: u32 = tag!(b"vmtx");
/// `VORG`
pub const VORG: u32 = tag!(b"VORG");
/// `vrt2`
pub const VRT2: u32 = tag!(b"vrt2");
/// `Zapf`
pub const ZAPF: u32 = tag!(b"Zapf");
/// `zero`
pub const ZERO: u32 = tag!(b"zero");

// MVAR [value tags](https://learn.microsoft.com/en-us/typography/opentype/spec/mvar#value-tags)

/// `hasc`: horizontal ascender - `OS/2.sTypoAscender`
pub const HASC: u32 = tag!(b"hasc");
/// `hdsc`: horizontal descender - `OS/2.sTypoDescender`
pub const HDSC: u32 = tag!(b"hdsc");
/// `hlgp`: horizontal line gap - `OS/2.sTypoLineGap`
pub const HLGP: u32 = tag!(b"hlgp");
/// `hcla`: horizontal clipping ascent - `OS/2.usWinAscent`
pub const HCLA: u32 = tag!(b"hcla");
/// `hcld`: horizontal clipping descent - `OS/2.usWinDescent`
pub const HCLD: u32 = tag!(b"hcld");
/// `vasc`: vertical ascender - `vhea.ascent`
pub const VASC: u32 = tag!(b"vasc");
/// `vdsc`: vertical descender - `vhea.descent`
pub const VDSC: u32 = tag!(b"vdsc");
/// `vlgp`: vertical line gap - `vhea.lineGap`
pub const VLGP: u32 = tag!(b"vlgp");
/// `hcrs`: horizontal caret rise - `hhea.caretSlopeRise`
pub const HCRS: u32 = tag!(b"hcrs");
/// `hcrn`: horizontal caret run - `hhea.caretSlopeRun`
pub const HCRN: u32 = tag!(b"hcrn");
/// `hcof`: horizontal caret offset - `hhea.caretOffset`
pub const HCOF: u32 = tag!(b"hcof");
/// `vcrs`: vertical caret rise - `vhea.caretSlopeRise`
pub const VCRS: u32 = tag!(b"vcrs");
/// `vcrn`: vertical caret run - `vhea.caretSlopeRun`
pub const VCRN: u32 = tag!(b"vcrn");
/// `vcof`: vertical caret offset - `vhea.caretOffset`
pub const VCOF: u32 = tag!(b"vcof");
/// `xhgt`: x height - `OS/2.sxHeight`
pub const XHGT: u32 = tag!(b"xhgt");
/// `cpht`: cap height - `OS/2.sCapHeight`
pub const CPHT: u32 = tag!(b"cpht");
/// `sbxs`: subscript em x size - `OS/2.ySubscriptXSize`
pub const SBXS: u32 = tag!(b"sbxs");
/// `sbys`: subscript em y size - `OS/2.ySubscriptYSize`
pub const SBYS: u32 = tag!(b"sbys");
/// `sbxo`: subscript em x offset - `OS/2.ySubscriptXOffset`
pub const SBXO: u32 = tag!(b"sbxo");
/// `sbyo`: subscript em y offset - `OS/2.ySubscriptYOffset`
pub const SBYO: u32 = tag!(b"sbyo");
/// `spxs`: superscript em x size - `OS/2.ySuperscriptXSize`
pub const SPXS: u32 = tag!(b"spxs");
/// `spys`: superscript em y size - `OS/2.ySuperscriptYSize`
pub const SPYS: u32 = tag!(b"spys");
/// `spxo`: superscript em x offset - `OS/2.ySuperscriptXOffset`
pub const SPXO: u32 = tag!(b"spxo");
/// `spyo`: superscript em y offset - `OS/2.ySuperscriptYOffset`
pub const SPYO: u32 = tag!(b"spyo");
/// `strs`: strikeout size - `OS/2.yStrikeoutSize`
pub const STRS: u32 = tag!(b"strs");
/// `stro`: strikeout offset - `OS/2.yStrikeoutPosition`
pub const STRO: u32 = tag!(b"stro");
/// `unds`: underline size - `post.underlineThickness`
pub const UNDS: u32 = tag!(b"unds");
/// `undo`: underline offset - `post.underlinePosition`
pub const UNDO: u32 = tag!(b"undo");
/// `gsp0`: `gaspRange[0]` - `gasp.gaspRange[0].rangeMaxPPEM`
pub const GSP0: u32 = tag!(b"gsp0");
/// `gsp1`: `gaspRange[1]` - `gasp.gaspRange[1].rangeMaxPPEM`
pub const GSP1: u32 = tag!(b"gsp1");
/// `gsp2`: `gaspRange[2]` - `gasp.gaspRange[2].rangeMaxPPEM`
pub const GSP2: u32 = tag!(b"gsp2");
/// `gsp3`: `gaspRange[3]` - `gasp.gaspRange[3].rangeMaxPPEM`
pub const GSP3: u32 = tag!(b"gsp3");
/// `gsp4`: `gaspRange[4]` - `gasp.gaspRange[4].rangeMaxPPEM`
pub const GSP4: u32 = tag!(b"gsp4");
/// `gsp5`: `gaspRange[5]` - `gasp.gaspRange[5].rangeMaxPPEM`
pub const GSP5: u32 = tag!(b"gsp5");
/// `gsp6`: `gaspRange[6]` - `gasp.gaspRange[6].rangeMaxPPEM`
pub const GSP6: u32 = tag!(b"gsp6");
/// `gsp7`: `gaspRange[7]` - `gasp.gaspRange[7].rangeMaxPPEM`
pub const GSP7: u32 = tag!(b"gsp7");
/// `gsp8`: `gaspRange[8]` - `gasp.gaspRange[8].rangeMaxPPEM`
pub const GSP8: u32 = tag!(b"gsp8");
/// `gsp9`: `gaspRange[9]` - `gasp.gaspRange[9].rangeMaxPPEM`
pub const GSP9: u32 = tag!(b"gsp9");

// Registered variation axis tags
// https://learn.microsoft.com/en-us/typography/opentype/spec/dvaraxisreg#registered-axis-tags

/// `ital`: Italic
pub const ITAL: u32 = tag!(b"ital");
/// `opsz`: Optical size
pub const OPSZ: u32 = tag!(b"opsz");
/// `slnt`: Slant
pub const SLNT: u32 = tag!(b"slnt");
/// `wdth`: Width
pub const WDTH: u32 = tag!(b"wdth");
/// `wght`: Weight
pub const WGHT: u32 = tag!(b"wght");

#[cfg(test)]
mod tests {
    use super::*;

    mod from_string {
        use super::*;

        #[test]
        fn test_five_chars() {
            assert!(from_string("12345").is_err());
        }

        #[test]
        fn test_four_chars() {
            let tag = from_string("beng").expect("invalid tag");

            assert_eq!(tag, 1650814567);
        }

        #[test]
        fn test_three_chars() {
            let tag = from_string("BEN").expect("invalid tag");

            assert_eq!(tag, 1111838240);
        }
    }

    mod display_tag {
        use crate::tag::{DisplayTag, NAME};

        #[test]
        fn test_simple_ascii() {
            assert_eq!(DisplayTag(NAME).to_string(), "name".to_string());
        }

        #[test]
        fn test_non_ascii() {
            assert_eq!(DisplayTag(0x12345678).to_string(), "0x12345678".to_string());
        }

        #[test]
        fn test_debug() {
            assert_eq!(format!("{:?}", DisplayTag(NAME)), "name".to_string());
        }
    }
}
