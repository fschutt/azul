//! AzPdf's model: zoom, the page strip, size labels, render sizes, the page
//! cache, the render plan, search, the recent documents, the page field.
//! Plain Rust - no window, no libazul calls.

use azul_appkit::find::TextMatch;

use crate::model::*;

const LETTER: PageSize = PageSize {
    width_pt: 612.0,
    height_pt: 792.0,
};
const A4: PageSize = PageSize {
    width_pt: 595.2756,
    height_pt: 841.8898,
};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

// ==== zoom ====

#[test]
fn a_percent_zoom_is_that_scale() {
    assert!(close(
        Zoom::Percent(100).scale(1000.0, 800.0, &[LETTER]),
        1.0
    ));
    assert!(close(
        Zoom::Percent(150).scale(1000.0, 800.0, &[LETTER]),
        1.5
    ));
}

#[test]
fn fit_width_makes_the_widest_page_fill_the_view_inside_its_padding() {
    // Letter at 100 % is 816 css px wide; the view leaves VIEW_PAD each side.
    let scale = Zoom::FitWidth.scale(1000.0, 800.0, &[LETTER]);
    assert!(close(scale, (1000.0 - 2.0 * VIEW_PAD) / 816.0), "{scale}");
    let wide = PageSize {
        width_pt: 1224.0,
        height_pt: 792.0,
    };
    let scale = Zoom::FitWidth.scale(1000.0, 800.0, &[LETTER, wide]);
    assert!(close(scale, (1000.0 - 2.0 * VIEW_PAD) / 1632.0), "{scale}");
}

#[test]
fn fit_page_makes_a_whole_page_fit_the_view() {
    // Letter is 816 x 1056 css px; an 1000 x 800 view is height-bound.
    let scale = Zoom::FitPage.scale(1000.0, 800.0, &[LETTER]);
    assert!(close(scale, (800.0 - 2.0 * VIEW_PAD) / 1056.0), "{scale}");
}

#[test]
fn a_zoom_scale_stays_within_its_bounds() {
    assert!(close(
        Zoom::FitWidth.scale(10.0, 10.0, &[LETTER]),
        MIN_SCALE
    ));
    assert!(close(
        Zoom::Percent(100_000).scale(10.0, 10.0, &[LETTER]),
        MAX_SCALE
    ));
    assert!(
        close(Zoom::FitWidth.scale(1000.0, 800.0, &[]), 1.0),
        "no pages: 100 %"
    );
}

#[test]
fn zooming_in_and_out_steps_to_the_next_stop() {
    let fit = Zoom::FitWidth.scale(1000.0, 800.0, &[LETTER]); // ~1.167
    assert_eq!(Zoom::zoom_in(fit), Zoom::Percent(125));
    assert_eq!(Zoom::zoom_out(fit), Zoom::Percent(100));
    assert_eq!(Zoom::zoom_in(1.0), Zoom::Percent(125));
    assert_eq!(Zoom::zoom_out(1.0), Zoom::Percent(75));
    let top = *ZOOM_STEPS.last().unwrap();
    let bottom = ZOOM_STEPS[0];
    assert_eq!(Zoom::zoom_in(top as f32 / 100.0), Zoom::Percent(top));
    assert_eq!(Zoom::zoom_out(bottom as f32 / 100.0), Zoom::Percent(bottom));
}

#[test]
fn a_zoom_has_a_label() {
    assert_eq!(Zoom::FitWidth.label(), "Fit width");
    assert_eq!(Zoom::FitPage.label(), "Fit page");
    assert_eq!(Zoom::Percent(125).label(), "125 %");
}

// ==== the page strip ====

fn three_letters() -> Strip {
    Strip::new(&[LETTER, LETTER, LETTER], 1.0)
}

#[test]
fn the_strip_stacks_the_pages_with_a_gap_inside_the_padding() {
    let strip = three_letters();
    assert!(close(strip.sizes[0].0, 816.0) && close(strip.sizes[0].1, 1056.0));
    assert!(close(strip.tops[0], VIEW_PAD));
    assert!(close(strip.tops[1], VIEW_PAD + 1056.0 + PAGE_GAP));
    assert!(close(strip.tops[2], VIEW_PAD + 2.0 * (1056.0 + PAGE_GAP)));
    assert!(close(
        strip.height,
        2.0 * VIEW_PAD + 3.0 * 1056.0 + 2.0 * PAGE_GAP
    ));
    assert!(close(strip.width, 816.0 + 2.0 * VIEW_PAD));
}

#[test]
fn the_strip_scales_its_pages() {
    let strip = Strip::new(&[A4], 0.5);
    assert!(close(strip.sizes[0].0, 595.2756 * PX_PER_PT * 0.5));
    assert!(close(strip.sizes[0].1, 841.8898 * PX_PER_PT * 0.5));
}

#[test]
fn the_visible_pages_are_those_that_meet_the_view() {
    let strip = three_letters();
    assert_eq!(strip.visible(0.0, 800.0), 0..1);
    // Page 0 ends at 1080, page 1 starts at 1096: a view over both.
    assert_eq!(strip.visible(1000.0, 200.0), 0..2);
    assert_eq!(strip.visible(0.0, 100_000.0), 0..3);
    assert_eq!(
        strip.visible(1_000_000.0, 100.0),
        3..3,
        "past the end: none"
    );
    let empty = Strip::new(&[], 1.0);
    assert_eq!(empty.visible(0.0, 800.0), 0..0);
}

#[test]
fn the_page_at_an_offset_owns_half_of_each_gap() {
    let strip = three_letters();
    assert_eq!(strip.page_at(0.0), 0);
    let boundary = strip.tops[1] - PAGE_GAP / 2.0;
    assert_eq!(strip.page_at(boundary - 0.1), 0);
    assert_eq!(strip.page_at(boundary), 1);
    assert_eq!(strip.page_at(1.0e9), 2);
    assert_eq!(Strip::new(&[], 1.0).page_at(10.0), 0);
}

#[test]
fn jumping_to_a_page_scrolls_to_its_top_and_lands_on_it() {
    let strip = three_letters();
    assert!(close(strip.top_of(0), 0.0));
    for page in 0..3 {
        assert_eq!(strip.page_at(strip.top_of(page)), page, "page {page}");
    }
    assert!(
        close(strip.top_of(99), strip.top_of(2)),
        "past the end: the last page"
    );
}

// ==== labels and sizes ====

#[test]
fn a_page_size_is_named_when_it_is_a_paper_size() {
    assert_eq!(size_label(A4), "A4 \u{b7} 210 \u{d7} 297 mm");
    let landscape = PageSize {
        width_pt: A4.height_pt,
        height_pt: A4.width_pt,
    };
    assert_eq!(
        size_label(landscape),
        "A4 landscape \u{b7} 297 \u{d7} 210 mm"
    );
    assert_eq!(size_label(LETTER), "Letter \u{b7} 8.5 \u{d7} 11 in");
    let odd = PageSize {
        width_pt: 500.0,
        height_pt: 500.0,
    };
    assert_eq!(size_label(odd), "176 \u{d7} 176 mm");
}

#[test]
fn a_page_is_rendered_in_width_buckets_at_the_displays_density() {
    assert_eq!(
        render_width(300.0, 2.0),
        640,
        "600 px rounds up to the bucket"
    );
    assert_eq!(render_width(816.0, 1.0), 832);
    assert_eq!(render_width(640.0, 1.0), 640, "a bucket boundary stays");
    assert_eq!(render_width(10_000.0, 2.0), MAX_RENDER_WIDTH);
    assert_eq!(render_width(0.0, 1.0), RENDER_BUCKET);
    assert_eq!(render_width(f32::NAN, 1.0), RENDER_BUCKET);
}

// ==== the page cache ====

#[test]
fn the_page_cache_forgets_the_least_recently_used_page() {
    let mut cache = PageCache::new(3);
    cache.insert(0, 640, "a");
    cache.insert(1, 640, "b");
    cache.insert(2, 640, "c");
    assert_eq!(cache.get(0, 640), Some("a"), "page 0 is used again");
    cache.insert(3, 640, "d");
    assert_eq!(
        cache.get(1, 640),
        None,
        "page 1 was the least recently used"
    );
    assert_eq!(cache.get(0, 640), Some("a"));
    assert_eq!(cache.get(3, 640), Some("d"));
    assert_eq!(cache.len(), 3);
}

#[test]
fn the_page_cache_keeps_one_entry_per_page_and_width() {
    let mut cache = PageCache::new(4);
    cache.insert(0, 640, "old");
    cache.insert(0, 640, "new");
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.get(0, 640), Some("new"));
    assert!(cache.has(0, 640));
    assert!(!cache.has(0, 1280));
}

#[test]
fn a_page_without_its_width_shows_the_nearest_render_meanwhile() {
    let mut cache = PageCache::new(4);
    cache.insert(5, 320, "small");
    cache.insert(5, 1280, "big");
    cache.insert(6, 640, "other page");
    assert_eq!(cache.nearest(5, 1100), Some("big"));
    assert_eq!(cache.nearest(5, 400), Some("small"));
    assert_eq!(cache.nearest(7, 640), None);
    cache.clear();
    assert_eq!(cache.len(), 0);
}

// ==== what to render next ====

#[test]
fn the_render_plan_takes_the_wanted_pages_in_order_minus_the_cached_and_running() {
    let cached = |page: usize| page == 2;
    let running = [(3, 640)];
    let plan = plan_renders(&[1, 2, 3, 4, 5], 640, cached, &running, 2);
    assert_eq!(plan, vec![(1, 640), (4, 640)]);
    // A page running at ANOTHER width is wanted again at this one.
    let plan = plan_renders(&[3], 1280, |_| false, &running, 4);
    assert_eq!(plan, vec![(3, 1280)]);
    assert!(plan_renders(&[], 640, |_| false, &[], 4).is_empty());
}

// ==== search ====

fn pages() -> Vec<String> {
    vec![
        "Hello world\r\nThe rent is due".to_string(),
        "No match here".to_string(),
        "rent rent".to_string(),
    ]
}

#[test]
fn search_finds_every_match_with_its_page() {
    let hits = search(&pages(), "rent", TextMatch::default());
    let on: Vec<usize> = hits.iter().map(|h| h.page).collect();
    assert_eq!(on, vec![0, 2, 2]);
    assert!(search(&pages(), "", TextMatch::default()).is_empty());
    assert!(search(&pages(), "absent", TextMatch::default()).is_empty());
    let case = TextMatch {
        match_case: true,
        ..TextMatch::default()
    };
    assert!(search(&pages(), "RENT", case).is_empty());
}

#[test]
fn a_search_hit_shows_its_context_on_one_line() {
    let hits = search(&pages(), "rent", TextMatch::default());
    assert!(
        hits[0].snippet.contains("The rent is due"),
        "{}",
        hits[0].snippet
    );
    assert!(!hits[0].snippet.contains('\r') && !hits[0].snippet.contains('\n'));
}

#[test]
fn a_search_hit_in_a_long_text_is_cut_around_the_match() {
    let long = format!("{} needle {}", "a".repeat(500), "b".repeat(500));
    let hits = search(&[long], "needle", TextMatch::default());
    assert_eq!(hits.len(), 1);
    let snippet = &hits[0].snippet;
    assert!(snippet.contains("needle"));
    assert!(snippet.starts_with('\u{2026}') && snippet.ends_with('\u{2026}'));
    assert!(
        snippet.chars().count() <= 2 * SNIPPET_CONTEXT + "needle".len() + 2,
        "{snippet}"
    );
}

// ==== the recent documents ====

fn doc(path: &str, page: usize) -> RecentDoc {
    RecentDoc {
        path: path.to_string(),
        title: file_title(path),
        pages: 10,
        last_page: page,
        opened: 1,
    }
}

#[test]
fn opening_a_document_puts_it_first_in_the_recent_list_once() {
    let mut recent = Recent::default();
    recent.touch(doc("/a/One.pdf", 0));
    recent.touch(doc("/a/Two.pdf", 0));
    recent.touch(doc("/a/One.pdf", 3));
    let paths: Vec<&str> = recent.docs.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(paths, vec!["/a/One.pdf", "/a/Two.pdf"]);
    assert_eq!(recent.docs[0].last_page, 3);
}

#[test]
fn the_recent_list_is_capped() {
    let mut recent = Recent::default();
    for i in 0..(MAX_RECENT + 5) {
        recent.touch(doc(&format!("/d/{i}.pdf"), 0));
    }
    assert_eq!(recent.docs.len(), MAX_RECENT);
    assert_eq!(recent.docs[0].path, format!("/d/{}.pdf", MAX_RECENT + 4));
}

#[test]
fn the_recent_list_round_trips_through_its_json_file() {
    let mut recent = Recent::default();
    recent.touch(doc("/a/One.pdf", 2));
    recent.set_page("/a/One.pdf", 7);
    let back = Recent::parse(&recent.to_json());
    assert_eq!(back, recent);
    assert_eq!(back.docs[0].last_page, 7);
    assert_eq!(Recent::parse(""), Recent::default());
    assert_eq!(Recent::parse("{ not json"), Recent::default());
}

// ==== small parsers ====

#[test]
fn the_page_field_reads_a_page_number_within_the_document() {
    assert_eq!(parse_page_field("12", 42), Some(11));
    assert_eq!(parse_page_field(" 7 ", 42), Some(6));
    assert_eq!(
        parse_page_field("0", 42),
        Some(0),
        "below the first: the first"
    );
    assert_eq!(
        parse_page_field("99", 42),
        Some(41),
        "past the last: the last"
    );
    assert_eq!(parse_page_field("", 42), None);
    assert_eq!(parse_page_field("abc", 42), None);
    assert_eq!(parse_page_field("5", 0), None, "no pages");
}

#[test]
fn a_documents_title_is_its_file_name_without_the_extension() {
    assert_eq!(file_title("/a/b/Manual.pdf"), "Manual");
    assert_eq!(file_title("Scan.PDF"), "Scan");
    assert_eq!(file_title("/a/notes"), "notes");
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_string()).collect()
}

#[test]
fn the_export_switches_name_the_output_the_page_the_width_and_the_file() {
    assert_eq!(
        parse_export(&strings(&["file.pdf"])),
        None,
        "no export: the window"
    );
    let request = parse_export(&strings(&[
        "--export-png",
        "/tmp/p1.png",
        "--page",
        "3",
        "--width",
        "1200",
        "in.pdf",
    ]));
    assert_eq!(
        request,
        Some(Ok(ExportRequest {
            format: ExportFormat::Png,
            out: "/tmp/p1.png".to_string(),
            page: 2,
            width: 1200,
            file: "in.pdf".to_string(),
        }))
    );
    let svg = parse_export(&strings(&["--export-svg", "o.svg", "in.pdf"]));
    assert_eq!(
        svg,
        Some(Ok(ExportRequest {
            format: ExportFormat::Svg,
            out: "o.svg".to_string(),
            page: 0,
            width: DEFAULT_EXPORT_WIDTH,
            file: "in.pdf".to_string(),
        }))
    );
    assert!(matches!(
        parse_export(&strings(&["--export-png", "o.png"])),
        Some(Err(_))
    ));
    assert!(matches!(
        parse_export(&strings(&[
            "--export-png",
            "o.png",
            "--page",
            "x",
            "in.pdf"
        ])),
        Some(Err(_))
    ));
}

#[test]
fn a_pdf_is_known_by_its_header_or_its_name() {
    assert!(is_pdf_bytes(b"%PDF-1.7\n..."));
    assert!(
        is_pdf_bytes(b"\xef\xbb\xbf junk %PDF-1.4"),
        "junk before the header"
    );
    assert!(!is_pdf_bytes(b"PK\x03\x04"));
    assert!(is_pdf_path("/a/B.Pdf"));
    assert!(!is_pdf_path("/a/b.txt"));
}
