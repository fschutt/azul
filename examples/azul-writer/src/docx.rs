//! Word (.docx) import: `docx-parser`'s JSON wire format into the shared
//! `RichTextDoc` - paragraphs with their runs (bold, italic, underline,
//! strike), alignment, outline levels as headings, numbered and bulleted
//! items (with their level), tables (cell text), page breaks. Anything else
//! in the wire is skipped; a wire the deserializer refuses falls back to
//! `docx-parser`'s Markdown.

use azul::widgets::{
    RichAlign, RichBlock, RichBlockKind, RichFormats, RichRun, RichTable, RichTableRow, RichTextDoc,
};
use serde::Deserialize;

mod wire {
    use super::Deserialize;

    #[derive(Deserialize, Debug, Default)]
    pub struct Doc {
        #[serde(default)]
        pub body: Vec<Body>,
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum Body {
        Paragraph(Para),
        Table(Table),
        PageBreak {},
        ColumnBreak,
        SectionBreak {},
        #[serde(other)]
        Unknown,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct Para {
        pub alignment: String,
        pub outline_level: Option<u32>,
        pub numbering: Option<Numbering>,
        pub runs: Vec<Run>,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct Numbering {
        pub format: String,
        pub level: u32,
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum Run {
        Text(TextRun),
        Break {},
        #[serde(other)]
        Unknown,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct TextRun {
        pub text: String,
        pub bold: bool,
        pub italic: bool,
        pub underline: bool,
        pub strikethrough: bool,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct Table {
        pub rows: Vec<TableRow>,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct TableRow {
        pub cells: Vec<TableCell>,
    }

    #[derive(Deserialize, Debug, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct TableCell {
        pub content: Vec<CellElement>,
    }

    #[derive(Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum CellElement {
        Paragraph(Para),
        #[serde(other)]
        Unknown,
    }
}

/// The text of a paragraph's runs (a line break is a space).
fn wire_text(runs: &[wire::Run]) -> String {
    let mut s = String::new();
    for run in runs {
        match run {
            wire::Run::Text(t) => s.push_str(&t.text),
            wire::Run::Break {} => s.push(' '),
            wire::Run::Unknown => {}
        }
    }
    s
}

/// A paragraph's runs as the model's.
fn wire_runs(runs: &[wire::Run]) -> Vec<RichRun> {
    let mut out: Vec<RichRun> = Vec::new();
    for run in runs {
        let (text, formats) = match run {
            wire::Run::Text(t) if !t.text.is_empty() => (
                t.text.clone(),
                RichFormats {
                    bold: t.bold,
                    italic: t.italic,
                    underline: t.underline,
                    strike: t.strikethrough,
                    code: false,
                },
            ),
            wire::Run::Break {} => (" ".to_string(), RichFormats::create()),
            _ => continue,
        };
        out.push(RichRun::create(text.as_str()).with_formats(formats));
    }
    out
}

/// One paragraph of the wire as a block.
fn wire_block(p: &wire::Para) -> RichBlock {
    let align = match p.alignment.as_str() {
        "center" => RichAlign::Center,
        "right" => RichAlign::Right,
        "both" | "justify" => RichAlign::Justify,
        _ => RichAlign::Left,
    };
    let level = |n: u32| u8::try_from(n.min(8)).unwrap_or(0);
    let kind = match (&p.numbering, p.outline_level) {
        (Some(n), _) if n.format == "bullet" => RichBlockKind::Bullet(level(n.level)),
        (Some(n), _) => RichBlockKind::Numbered(level(n.level)),
        (None, Some(outline)) => RichBlockKind::Heading(level(outline.saturating_add(1)).clamp(1, 6)),
        (None, None) => RichBlockKind::Paragraph,
    };
    RichBlock::create(kind, wire_runs(&p.runs)).with_align(align)
}

/// The wire JSON of `docx-parser` as a document.
pub fn from_docx_wire(json: &str) -> Result<RichTextDoc, String> {
    let doc: wire::Doc = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut blocks: Vec<RichBlock> = Vec::new();
    for element in doc.body {
        match element {
            wire::Body::Paragraph(p) => blocks.push(wire_block(&p)),
            wire::Body::Table(t) => {
                let rows: Vec<RichTableRow> = t
                    .rows
                    .iter()
                    .map(|row| {
                        let cells: Vec<azul::str::String> = row
                            .cells
                            .iter()
                            .map(|cell| {
                                cell.content
                                    .iter()
                                    .filter_map(|el| match el {
                                        wire::CellElement::Paragraph(p) => Some(wire_text(&p.runs)),
                                        wire::CellElement::Unknown => None,
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ")
                                    .into()
                            })
                            .collect();
                        RichTableRow::create(cells)
                    })
                    .collect();
                if !rows.is_empty() {
                    blocks.push(RichBlock::create(
                        RichBlockKind::Table(RichTable::create(rows, false)),
                        Vec::<RichRun>::new(),
                    ));
                }
            }
            wire::Body::PageBreak {} => {
                blocks.push(RichBlock::create(RichBlockKind::PageBreak, Vec::<RichRun>::new()));
            }
            wire::Body::ColumnBreak | wire::Body::SectionBreak {} | wire::Body::Unknown => {}
        }
    }
    if blocks.is_empty() {
        blocks.push(RichBlock::create_paragraph(""));
    }
    Ok(RichTextDoc::create_from_blocks(blocks))
}

/// A .docx file's bytes as a document: the wire format, else the Markdown
/// `docx-parser` makes of it.
pub fn from_docx_bytes(data: &[u8]) -> Result<RichTextDoc, String> {
    let json = docx_parser::parse_docx_native(data)?;
    match from_docx_wire(&json) {
        Ok(doc) => Ok(doc),
        Err(wire_err) => docx_parser::to_markdown_native(data)
            .map(|md| RichTextDoc::create_from_markdown(md.as_str()))
            .map_err(|md_err| format!("wire: {wire_err}; markdown: {md_err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(doc: &RichTextDoc) -> Vec<String> {
        doc.blocks
            .as_slice()
            .iter()
            .map(|b| b.get_text().as_str().to_string())
            .collect()
    }

    #[test]
    fn the_docx_wire_subset_becomes_blocks_and_skips_the_unknown() {
        let json = r#"{
            "section": {},
            "body": [
                {"type":"paragraph","alignment":"left","outlineLevel":0,
                 "runs":[{"type":"text","text":"Heading","bold":false,"italic":false,
                          "underline":false,"strikethrough":false,"fontSize":16.0}]},
                {"type":"paragraph","alignment":"both",
                 "runs":[{"type":"text","text":"Body ","bold":false,"italic":false,
                          "underline":false,"strikethrough":false,"fontSize":11.0},
                         {"type":"text","text":"bold","bold":true,"italic":false,
                          "underline":false,"strikethrough":false,"fontSize":11.0},
                         {"type":"shape","whatever":123}]},
                {"type":"paragraph","alignment":"left",
                 "numbering":{"numId":1,"level":1,"format":"bullet","text":"x"},
                 "runs":[{"type":"text","text":"a bullet","bold":false,"italic":false,
                          "underline":false,"strikethrough":false,"fontSize":11.0}]},
                {"type":"pageBreak"},
                {"type":"someFutureThing","payload":{}}
            ]
        }"#;
        let doc = from_docx_wire(json).expect("the subset deserializes");
        let blocks = doc.blocks.as_slice();
        assert_eq!(blocks.len(), 4, "the unknown element is dropped");
        assert_eq!(blocks[0].kind, RichBlockKind::Heading(1), "outline level 0 is heading 1");
        assert_eq!(blocks[1].align, RichAlign::Justify);
        assert_eq!(blocks[1].runs.as_slice().len(), 2);
        assert!(blocks[1].runs.as_slice()[1].formats.bold);
        assert_eq!(blocks[2].kind, RichBlockKind::Bullet(1), "the item keeps its level");
        assert_eq!(blocks[3].kind, RichBlockKind::PageBreak);
        assert_eq!(texts(&doc)[..3], ["Heading", "Body bold", "a bullet"]);
    }

    #[test]
    fn a_real_docx_lands_in_the_document() {
        let bytes = include_bytes!("../testdata/sample.docx");
        let doc = from_docx_bytes(bytes).expect("the docx loads");
        let blocks = doc.blocks.as_slice();
        assert_eq!(blocks[0].kind, RichBlockKind::Heading(1), "{:?}", blocks.first());
        assert_eq!(blocks[0].get_text().as_str(), "A Real Heading");
        assert_eq!(blocks[1].get_text().as_str(), "Plain then bold italic");
        let runs = blocks[1].runs.as_slice();
        assert!(runs.iter().any(|r| r.formats.bold), "a bold run survives");
        assert!(runs.iter().any(|r| r.formats.italic), "an italic run survives");
        assert!(
            blocks.iter().any(|b| b.kind == RichBlockKind::PageBreak),
            "the page break survives"
        );
        assert_eq!(crate::model::title_of(&doc), "A Real Heading");
    }
}
