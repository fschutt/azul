//! PDF standards such as PDF/A and PDF/X, and what they require.
//!
//! Besides the general PDF format (ISO 32000), there are subset standards for specific
//! jobs: PDF/A for long-term archiving (ISO 19005), PDF/X for print production
//! (ISO 15930), PDF/UA for accessibility (ISO 14289), PDF/E for engineering (ISO 24517) and
//! PDF/VT for variable data printing (ISO 16612). A conforming file must follow extra
//! rules, for example embed all fonts, carry XMP metadata (an XML description of the
//! document stored in the file) or name the color profile it was prepared for (an output intent).
//!
//! [`PdfConformance`] is stored in [`PdfDocumentInfo::conformance`](crate::PdfDocumentInfo)
//! and chooses which of these are added when the document is saved:
//!
//! * The identifier of the standard (for example `"PDF/X-3:2002"`) is written to the
//!   `/GTS_PDFXVersion` entry of the document info dictionary and to the XMP metadata.
//! * If [`must_have_icc_profile`](PdfConformance::must_have_icc_profile) is true, an output
//!   intent with the embedded CMYK profile "Coated FOGRA39" is added to the catalog.
//! * If [`must_have_xmp_metadata`](PdfConformance::must_have_xmp_metadata) is true, an XMP
//!   metadata stream is added to the catalog.
//!
//! The crate does not validate a document against a standard. Choosing a variant does not
//! embed fonts, convert colors or reject content that the standard forbids; the
//! `is_*_allowed` methods only report what the standard permits.
//!
//! The default is [`PdfConformance::Custom`] with the default [`CustomPdfConformance`]: no
//! identifier, no XMP metadata and no output intent, which gives the smallest files.

use serde_derive::{Deserialize, Serialize};

/// A PDF standard that a document claims to follow.
///
/// Variant names read `<standard>_<year>_PDF_<version>`, with the PDF version of that
/// edition of the standard.
///
/// The enum is `#[serde(untagged)]`, so in JSON every variant except `Custom` is written
/// as `null`.
///
/// # Examples
///
/// ```
/// use printpdf::PdfConformance;
///
/// let c = PdfConformance::from_identifier_string("PDF/X-3:2002");
/// assert_eq!(c, PdfConformance::X3_2002_PDF_1_3);
/// assert!(c.must_have_xmp_metadata() && c.must_have_icc_profile());
/// assert_eq!(serde_json::to_string(&c).unwrap(), "null");
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
#[serde(untagged, rename_all = "kebab-case")]
pub enum PdfConformance {
    /// `PDF/A-1b`: basic archival conformance, many features restricted (ISO 19005-1).
    A1B_2005_PDF_1_4,
    /// `PDF/A-1a`: like 1b, plus document structure and Unicode text (ISO 19005-1).
    A1A_2005_PDF_1_4,
    /// `PDF/A-2`: adds JPEG 2000, transparency, layers and OpenType fonts (ISO 19005-2).
    A2_2011_PDF_1_7,
    /// `PDF/A-2a`: PDF/A-2 with document structure and Unicode text.
    A2A_2011_PDF_1_7,
    /// `PDF/A-2b`: PDF/A-2 with basic conformance.
    A2B_2011_PDF_1_7,
    /// `PDF/A-2u`: PDF/A-2 where all text maps to Unicode.
    A2U_2011_PDF_1_7,
    /// `PDF/A-3`: PDF/A-2 that may embed arbitrary files such as XML or CAD data (ISO 19005-3).
    A3_2012_PDF_1_7,
    /// `PDF/UA-1`: accessibility for screen readers and other assistive software (ISO 14289-1).
    UA_2014_PDF_1_6,
    /// `PDF/X-1a:2001`: print exchange with CMYK and spot colors, no ICC profiles (ISO 15930-1).
    X1A_2001_PDF_1_3,
    /// `PDF/X-3:2002`: like X-1a, and also allows calibrated color and ICC profiles
    /// (ISO 15930-3).
    X3_2002_PDF_1_3,
    /// `PDF/X-1a:2003`: revision of `PDF/X-1a:2001` based on PDF 1.4 (ISO 15930-4).
    X1A_2003_PDF_1_4,
    /// `PDF/X-3:2003`: revision of `PDF/X-3:2002` based on PDF 1.4 (ISO 15930-6).
    X3_2003_PDF_1_4,
    /// `PDF/X-4:2010`: colour-managed print exchange with transparency and layers
    /// (ISO 15930-7).
    X4_2010_PDF_1_4,
    /// `PDF/X-4p:2010`: X-4 whose ICC profile may be an external file, based on PDF 1.6.
    X4P_2010_PDF_1_6,
    /// `PDF/X-5g:2010`: X-4 that may reference graphics outside the file (ISO 15930-8).
    X5G_2010_PDF_1_6,
    /// `PDF/X-5pg`: X-5g combined with an external ICC profile for the output intent.
    X5PG_2010_PDF_1_6,
    /// `PDF/X-5n`: X-4p where the external output intent profile may use any color space.
    X5N_2010_PDF_1_6,
    /// `PDF/E-1:2008`: engineering documents with 3D objects and geospatial data
    /// (ISO 24517-1).
    E1_2008_PDF_1_6,
    /// `PDF/VT-1:2010`: variable and transactional printing, such as personalized mailings
    /// (ISO 16612-2).
    VT_2010_PDF_1_4,
    /// No standard in particular; the settings come from the [`CustomPdfConformance`].
    Custom(CustomPdfConformance),
}

/// The default is [`PdfConformance::Custom`] with the default [`CustomPdfConformance`],
/// which keeps the file small.
impl Default for PdfConformance {
    fn default() -> Self {
        Self::Custom(CustomPdfConformance::default())
    }
}

/// Settings of a [`PdfConformance::Custom`] profile, for documents that do not need to follow a
/// PDF standard, for example when the file should be as small as possible.
///
/// Serialized to JSON with camelCase field names; missing fields take the values of
/// [`CustomPdfConformance::default`], except where noted. The `PdfConformance` methods read
/// every field except `allows_default_fonts`, which is stored but not used.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPdfConformance {
    /// Text written as the standard's identifier. Default: empty.
    #[serde(default)]
    pub identifier: String,
    /// Whether 3D content is allowed. Default: `false`.
    #[serde(default)]
    pub allows_3d_content: bool,
    /// Whether video content is allowed. Default: `false`.
    #[serde(default)]
    pub allows_video_content: bool,
    /// Whether audio content is allowed. Default: `false`.
    #[serde(default)]
    pub allows_audio_content: bool,
    /// Whether embedded JavaScript is allowed. Default: `false`.
    #[serde(default)]
    pub allows_embedded_javascript: bool,
    /// Whether JPEG-compressed images are allowed. `true` from [`Default`], but `false` when
    /// the field is missing in deserialized data.
    #[serde(default = "default_false")]
    pub allows_jpeg_content: bool,
    /// Whether XMP metadata must be written. `false` from [`Default`] and when missing.
    #[serde(default = "default_false")]
    pub requires_xmp_metadata: bool,
    /// Whether the 14 built-in PDF fonts (Helvetica, ...) are allowed. They are not embedded,
    /// so printed output depends on the viewer's substitute. Default: `false`.
    #[serde(default)]
    pub allows_default_fonts: bool,
    /// Whether an output intent with an ICC color profile must be written. `false` from
    /// [`Default`] and when missing.
    #[serde(default = "default_false")]
    pub requires_icc_profile: bool,
    /// Whether layers (optional content) are allowed. `true` from [`Default`], but `false` when
    /// the field is missing in deserialized data.
    #[serde(default = "default_false")]
    pub allows_pdf_layers: bool,
}

fn default_false() -> bool {
    false
}

impl Default for CustomPdfConformance {
    fn default() -> Self {
        CustomPdfConformance {
            identifier: "".into(),
            allows_3d_content: false,
            allows_video_content: false,
            allows_audio_content: false,
            allows_embedded_javascript: false,
            allows_jpeg_content: true,
            requires_xmp_metadata: false,
            allows_default_fonts: false,
            requires_icc_profile: false,
            allows_pdf_layers: true,
        }
    }
}

impl PdfConformance {
    /// Parses an identifier such as `"PDF/A-2b:2011"` (see
    /// [`get_identifier_string`](Self::get_identifier_string)).
    ///
    /// An unknown string gives a [`PdfConformance::Custom`] with that identifier. This is how
    /// the `/GTS_PDFXVersion` entry of a parsed PDF is read.
    pub fn from_identifier_string(i: &str) -> Self {
        // the identifiers might not match the official names in all cases
        match i {
            "PDF/A-1b:2005" => PdfConformance::A1B_2005_PDF_1_4,
            "PDF/A-1a:2005" => PdfConformance::A1A_2005_PDF_1_4,
            "PDF/A-2:2011" => PdfConformance::A2_2011_PDF_1_7,
            "PDF/A-2a:2011" => PdfConformance::A2A_2011_PDF_1_7,
            "PDF/A-2b:2011" => PdfConformance::A2B_2011_PDF_1_7,
            "PDF/A-2u:2011" => PdfConformance::A2U_2011_PDF_1_7,
            "PDF/A-3:2012" => PdfConformance::A3_2012_PDF_1_7,
            "PDF/UA" => PdfConformance::UA_2014_PDF_1_6,
            "PDF/X-1a:2001" => PdfConformance::X1A_2001_PDF_1_3,
            "PDF/X-3:2002" => PdfConformance::X3_2002_PDF_1_3,
            "PDF/X-1a:2003" => PdfConformance::X1A_2003_PDF_1_4,
            "PDF/X-3:2003" => PdfConformance::X3_2003_PDF_1_4,
            "PDF/X-4" => PdfConformance::X4_2010_PDF_1_4,
            "PDF/X-4P" => PdfConformance::X4P_2010_PDF_1_6,
            "PDF/X-5G" => PdfConformance::X5G_2010_PDF_1_6,
            "PDF/X-5PG" => PdfConformance::X5PG_2010_PDF_1_6,
            "PDF/X-5N" => PdfConformance::X5N_2010_PDF_1_6,
            "PDF/E-1" => PdfConformance::E1_2008_PDF_1_6,
            "PDF/VT" => PdfConformance::VT_2010_PDF_1_4,
            i => PdfConformance::Custom(CustomPdfConformance {
                identifier: i.to_string(),
                ..Default::default()
            }),
        }
    }

    /// Identifier of the standard, such as `"PDF/X-3:2002"`. For `Custom` it is the
    /// `identifier` field.
    ///
    /// This is the string written to `/GTS_PDFXVersion` and to the XMP metadata.
    pub fn get_identifier_string(&self) -> String {
        let identifier = match *self {
            PdfConformance::A1B_2005_PDF_1_4 => "PDF/A-1b:2005",
            PdfConformance::A1A_2005_PDF_1_4 => "PDF/A-1a:2005",
            PdfConformance::A2_2011_PDF_1_7 => "PDF/A-2:2011",
            PdfConformance::A2A_2011_PDF_1_7 => "PDF/A-2a:2011",
            PdfConformance::A2B_2011_PDF_1_7 => "PDF/A-2b:2011",
            PdfConformance::A2U_2011_PDF_1_7 => "PDF/A-2u:2011",
            PdfConformance::A3_2012_PDF_1_7 => "PDF/A-3:2012",
            PdfConformance::UA_2014_PDF_1_6 => "PDF/UA",
            PdfConformance::X1A_2001_PDF_1_3 => "PDF/X-1a:2001",
            PdfConformance::X3_2002_PDF_1_3 => "PDF/X-3:2002",
            PdfConformance::X1A_2003_PDF_1_4 => "PDF/X-1a:2003",
            PdfConformance::X3_2003_PDF_1_4 => "PDF/X-3:2003",
            PdfConformance::X4_2010_PDF_1_4 => "PDF/X-4",
            PdfConformance::X4P_2010_PDF_1_6 => "PDF/X-4P",
            PdfConformance::X5G_2010_PDF_1_6 => "PDF/X-5G",
            PdfConformance::X5PG_2010_PDF_1_6 => "PDF/X-5PG",
            PdfConformance::X5N_2010_PDF_1_6 => "PDF/X-5N",
            PdfConformance::E1_2008_PDF_1_6 => "PDF/E-1",
            PdfConformance::VT_2010_PDF_1_4 => "PDF/VT",
            PdfConformance::Custom(ref c) => &c.identifier,
        };

        identifier.to_string()
    }

    /// Whether the standard permits 3D content: only PDF/E-1, or a `Custom` profile that
    /// allows it. The crate does not check document content against this.
    pub fn is_3d_content_allowed(&self) -> bool {
        match *self {
            PdfConformance::E1_2008_PDF_1_6 => true,
            PdfConformance::Custom(ref c) => c.allows_3d_content,
            _ => false,
        }
    }

    /// Whether video content is permitted: only for a `Custom` profile that allows it. The
    /// crate does not check document content against this.
    pub fn is_video_content_allowed(&self) -> bool {
        match *self {
            PdfConformance::Custom(ref c) => c.allows_video_content,
            _ => false,
        }
    }

    /// Whether audio content is permitted: only for a `Custom` profile that allows it. The
    /// crate does not check document content against this.
    pub fn is_audio_content_allowed(&self) -> bool {
        match *self {
            PdfConformance::Custom(ref c) => c.allows_audio_content,
            _ => false,
        }
    }

    /// Whether embedded JavaScript is permitted: only for a `Custom` profile that allows it.
    /// The crate does not check document content against this.
    pub fn is_javascript_content_allowed(&self) -> bool {
        match *self {
            PdfConformance::Custom(ref c) => c.allows_embedded_javascript,
            _ => false,
        }
    }

    /// Whether JPEG images are permitted: always, except for a `Custom` profile that
    /// disallows them. The crate does not check document content against this.
    pub fn is_jpeg_content_allowed(&self) -> bool {
        match *self {
            PdfConformance::Custom(ref c) => c.allows_jpeg_content,
            _ => true,
        }
    }

    /// Whether saving writes an XMP metadata stream: true for PDF/X-1a, X-3, X-4, X-4p, X-5g
    /// and X-5pg, and for a `Custom` profile with `requires_xmp_metadata`. The PDF/A, PDF/UA,
    /// PDF/E, PDF/VT and PDF/X-5n variants return false.
    pub fn must_have_xmp_metadata(&self) -> bool {
        match *self {
            PdfConformance::X1A_2001_PDF_1_3 => true,
            PdfConformance::X3_2002_PDF_1_3 => true,
            PdfConformance::X1A_2003_PDF_1_4 => true,
            PdfConformance::X3_2003_PDF_1_4 => true,
            PdfConformance::X4_2010_PDF_1_4 => true,
            PdfConformance::X4P_2010_PDF_1_6 => true,
            PdfConformance::X5G_2010_PDF_1_6 => true,
            PdfConformance::X5PG_2010_PDF_1_6 => true,
            PdfConformance::Custom(ref c) => c.requires_xmp_metadata,
            _ => false,
        }
    }

    /// Whether saving writes an output intent with an embedded ICC profile: true for every
    /// standard except PDF/X-1a:2001, and for a `Custom` profile with `requires_icc_profile`.
    pub fn must_have_icc_profile(&self) -> bool {
        match *self {
            PdfConformance::X1A_2001_PDF_1_3 => false,
            PdfConformance::Custom(ref c) => c.requires_icc_profile,
            _ => true,
        }
    }

    /// Whether the standard permits layers (optional content groups): false for PDF/X-1a and
    /// PDF/X-3, true for the other standards, and the `allows_pdf_layers` field for `Custom`.
    /// The crate does not check document content against this.
    pub fn is_layering_allowed(&self) -> bool {
        match self {
            PdfConformance::X1A_2001_PDF_1_3 => false,
            PdfConformance::X3_2002_PDF_1_3 => false,
            PdfConformance::X1A_2003_PDF_1_4 => false,
            PdfConformance::X3_2003_PDF_1_4 => false,
            PdfConformance::Custom(c) => c.allows_pdf_layers,
            _ => true,
        }
    }
}
