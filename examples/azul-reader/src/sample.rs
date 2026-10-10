//! The sample book of `--sample` (an empty library only): an EPUB 3 made here with azul's
//! `Zip` - two chapters of Lewis Carroll's "Alice's Adventures in Wonderland" (1865, in the
//! public domain), a navigation document and a style sheet - so the library, the pages, the
//! table of contents and the bookmarks have something to show.

/// The sample's file name (its title in the library comes from its package).
pub const FILE_NAME: &str = "alice-sample.epub";

const CONTAINER: &str = "<?xml version=\"1.0\"?>\n<container version=\"1.0\" \
xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile \
full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles>\
</container>";

const OPF: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package \
xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\"><metadata \
xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"uid\">azreader-sample-alice\
</dc:identifier><dc:title>Alice's Adventures in Wonderland (sample)</dc:title><dc:creator>\
Lewis Carroll</dc:creator><dc:language>en</dc:language></metadata><manifest><item id=\"nav\" \
href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/><item id=\"css\" \
href=\"style.css\" media-type=\"text/css\"/><item id=\"c1\" href=\"chapter-1.xhtml\" \
media-type=\"application/xhtml+xml\"/><item id=\"c2\" href=\"chapter-2.xhtml\" \
media-type=\"application/xhtml+xml\"/></manifest><spine><itemref idref=\"c1\"/><itemref \
idref=\"c2\"/></spine></package>";

const NAV: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html \
xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head>\
<title>Contents</title></head><body><nav epub:type=\"toc\"><ol><li><a \
href=\"chapter-1.xhtml\">Chapter I. Down the Rabbit-Hole</a><ol><li><a \
href=\"chapter-1.xhtml#well\">The well</a></li></ol></li><li><a href=\"chapter-2.xhtml\">\
Chapter II. The Pool of Tears</a></li></ol></nav></body></html>";

const CSS: &str = "h1 { text-align: center; font-size: 1.4em; margin: 1em 0 1.5em 0; }\n\
p { margin: 0; text-indent: 1.5em; }\nh1 + p { text-indent: 0; }\n";

const CHAPTER_1: &[&str] = &[
    "Alice was beginning to get very tired of sitting by her sister on the bank, and of having \
     nothing to do: once or twice she had peeped into the book her sister was reading, but it \
     had no pictures or conversations in it, \u{2018}and what is the use of a book,\u{2019} \
     thought Alice \u{2018}without pictures or conversations?\u{2019}",
    "So she was considering in her own mind (as well as she could, for the hot day made her \
     feel very sleepy and stupid), whether the pleasure of making a daisy-chain would be worth \
     the trouble of getting up and picking the daisies, when suddenly a White Rabbit with pink \
     eyes ran close by her.",
    "There was nothing so very remarkable in that; nor did Alice think it so very much out of \
     the way to hear the Rabbit say to itself, \u{2018}Oh dear! Oh dear! I shall be \
     late!\u{2019} (when she thought it over afterwards, it occurred to her that she ought to \
     have wondered at this, but at the time it all seemed quite natural); but when the Rabbit \
     actually took a watch out of its waistcoat-pocket, and looked at it, and then hurried on, \
     Alice started to her feet, for it flashed across her mind that she had never before seen a \
     rabbit with either a waistcoat-pocket, or a watch to take out of it, and burning with \
     curiosity, she ran across the field after it, and fortunately was just in time to see it \
     pop down a large rabbit-hole under the hedge.",
    "In another moment down went Alice after it, never once considering how in the world she \
     was to get out again.",
];

const CHAPTER_1_WELL: &[&str] = &[
    "The rabbit-hole went straight on like a tunnel for some way, and then dipped suddenly \
     down, so suddenly that Alice had not a moment to think about stopping herself before she \
     found herself falling down a very deep well.",
];

const CHAPTER_2: &[&str] = &[
    "\u{2018}Curiouser and curiouser!\u{2019} cried Alice (she was so much surprised, that for \
     the moment she quite forgot how to speak good English); \u{2018}now I\u{2019}m opening out \
     like the largest telescope that ever was! Good-bye, feet!\u{2019}",
];

/// One chapter's XHTML: the heading, the paragraphs, then (with an id) more paragraphs.
fn chapter(title: &str, paragraphs: &[&str], more: Option<(&str, &[&str])>) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\">\
         <head><title>{title}</title><link rel=\"stylesheet\" type=\"text/css\" \
         href=\"style.css\"/></head><body><h1>{title}</h1>"
    );
    for p in paragraphs {
        out.push_str("<p>");
        out.push_str(azul::xml::Xml::encode_text(*p).as_str());
        out.push_str("</p>");
    }
    if let Some((id, paragraphs)) = more {
        for (i, p) in paragraphs.iter().enumerate() {
            if i == 0 {
                out.push_str(&format!("<p id=\"{id}\">"));
            } else {
                out.push_str("<p>");
            }
            out.push_str(azul::xml::Xml::encode_text(*p).as_str());
            out.push_str("</p>");
        }
    }
    out.push_str("</body></html>");
    out
}

/// The sample book's bytes (an EPUB).
#[must_use]
pub fn sample_epub() -> Vec<u8> {
    let mut zip = azul::zip::Zip::create();
    zip.add_file("mimetype", b"application/epub+zip".to_vec());
    zip.add_file("META-INF/container.xml", CONTAINER.as_bytes().to_vec());
    zip.add_file("OEBPS/content.opf", OPF.as_bytes().to_vec());
    zip.add_file("OEBPS/nav.xhtml", NAV.as_bytes().to_vec());
    zip.add_file("OEBPS/style.css", CSS.as_bytes().to_vec());
    zip.add_file(
        "OEBPS/chapter-1.xhtml",
        chapter(
            "Chapter I. Down the Rabbit-Hole",
            CHAPTER_1,
            Some(("well", CHAPTER_1_WELL)),
        )
        .into_bytes(),
    );
    zip.add_file(
        "OEBPS/chapter-2.xhtml",
        chapter("Chapter II. The Pool of Tears", CHAPTER_2, None).into_bytes(),
    );
    zip.to_bytes().as_slice().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::{parse_book, Container};

    #[test]
    fn the_sample_is_an_epub_with_two_chapters_a_table_of_contents_and_an_anchor() {
        let container = Container::from_zip_bytes(&sample_epub()).expect("a zip");
        let book = parse_book(&container).expect("a book");
        assert_eq!(
            book.metadata.title,
            "Alice's Adventures in Wonderland (sample)"
        );
        assert_eq!(book.metadata.authors, vec!["Lewis Carroll".to_string()]);
        assert_eq!(book.spine.len(), 2);
        let labels: Vec<&str> = book.toc.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Chapter I. Down the Rabbit-Hole",
                "The well",
                "Chapter II. The Pool of Tears"
            ]
        );
        assert_eq!(book.toc[1].fragment, "well");
        let options = crate::content::ReadOptions {
            book: "s".to_string(),
            page_width: 400.0,
            page_height: 600.0,
        };
        let ch1 = crate::content::read_chapter(
            &container,
            &book.spine[0].path,
            false,
            &options,
            &mut |_| None,
        );
        assert!(ch1.anchor_fraction("well").is_some_and(|f| f > 0.5));
        assert!(ch1
            .text
            .starts_with("Chapter I. Down the Rabbit-Hole Alice was beginning"));
    }
}
