//! The whole of the whiteboard without a window: a loop is drawn round a word, armed, lifted by a hold and
//! dropped on the bar, drawn as a cutout, joined to another by an arrow, kept in a file, read back
//! into a board that is the same, and jumped from to the lines it was made of - also after the file
//! changed. Every part is the real one; the picture's layers, the PNG codec and the files are stand-ins.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use crate::{
    stroke_fixtures as fx,
    whiteboard::{
        kept_lines, lines_hash, Board, BoardEvent, BoardObserver, CutoutDraft, End, ImageSlot, Item, Source, LIMITS,
    },
    whiteboard_cutout::{compose, plan, Raster},
    whiteboard_drag::{Armed, ArmedRegions, Bar, Drag, Motion, Pointer, Press, Release, Tick, HOLD_MS},
    whiteboard_geom::{Pt, Rect, Size},
    whiteboard_jump::{find_block_in, jump, FileView, Freshness},
    whiteboard_lasso::{covered_lines, recognize, PageLines},
    whiteboard_store::{board_name, read, write, DecodedPng, ImageCodec, MANIFEST},
};

const PAGE: Size = Size { w: 1000.0, h: 730.0 };

struct Codec;

impl ImageCodec for Codec {
    fn decode_png(&self, png: &[u8]) -> Result<DecodedPng, String> {
        let (w, h) = (u32::from_le_bytes(png[0..4].try_into().map_err(|_| "short")?), u32::from_le_bytes(png[4..8].try_into().map_err(|_| "short")?));
        Ok(DecodedPng { width: w, height: h, rgba: png[8..].to_vec() })
    }
}

/// A stand-in PNG: the size and the straight pixels.
fn fake_png(r: &Raster) -> Vec<u8> {
    let mut v = r.w.to_le_bytes().to_vec();
    v.extend_from_slice(&r.h.to_le_bytes());
    v.extend_from_slice(&r.to_straight());
    v
}

struct Files(BTreeMap<String, Vec<String>>);

impl FileView for Files {
    fn file_hash(&self, file: &str) -> Option<u64> {
        self.0.get(file).map(|l| lines_hash(l))
    }

    fn line_count(&self, file: &str) -> Option<u32> {
        self.0.get(file).map(|l| l.len() as u32)
    }

    fn lines(&self, file: &str, first: u32, last: u32) -> Option<Vec<String>> {
        self.0.get(file)?.get((first as usize - 1)..(last as usize))
            .map(<[String]>::to_vec)
    }

    fn find_block(&self, file: &str, block: &[String], near: u32) -> Option<u32> {
        find_block_in(self.0.get(file)?, block, near)
    }
}

struct Log(Rc<RefCell<Vec<BoardEvent>>>);

impl BoardObserver for Log {
    fn on_event(&mut self, event: &BoardEvent) {
        self.0.borrow_mut().push(event.clone());
    }
}

/// A file of 120 lines, each 40 characters or so.
fn the_file() -> Vec<String> {
    (1..=120).map(|i| format!("pub fn step_{i:03}(x: u32) -> u32 {{ x + {i} }}")).collect()
}

/// The loop `stroke` on page 0 (lines from 1, the page starts at line 1), armed at `now`: what the app does when a pen lifts.
fn arm(stroke: &fx::Fixture, regions: &mut ArmedRegions, file: &[String], id: u64, now: u64) -> (crate::whiteboard_lasso::Lasso, u32, u32) {
    let pts: Vec<Pt> = stroke.samples.iter().map(|s| Pt::new(s.x, s.y)).collect();
    let lasso = recognize(&pts).expect("a lasso");
    let chars: Vec<usize> = file.iter().take(46).map(|l| l.chars().count()).collect();
    let page = PageLines { first_number: 1, chars: &chars, top: 32.0, line_h: 15.0, text_x: 84.0, char_w: 6.6, max_w: 600.0 };
    let range = covered_lines(&lasso.polygon, &page).expect("lines");
    regions.arm(Armed { page: 0, polygon: lasso.polygon.clone(), first_line: range.first, last_line: range.last, stroke_id: id }, PAGE, now);
    (lasso, range.first, range.last)
}

#[test]
fn a_loop_becomes_a_cutout_two_cutouts_get_an_arrow_the_board_is_kept_and_a_jump_finds_the_lines() {
    let file = the_file();
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut board = Board::new();
    board.set_observer(Box::new(Log(events.clone())));
    let mut regions = ArmedRegions::default();
    let mut drag = Drag::default();
    let bar = Bar { rect: Rect::new(980.0, 30.0, 300.0, 760.0), open: true, window_w: 1280.0 };
    let mut made = Vec::new();

    // Two loops, each lifted by pressing and holding inside it, carried to the bar and dropped.
    for (i, (stroke, drop)) in [(fx::closed_loop(), Pt::new(1_040.0, 120.0)), (fx::loop_around_a_line(), Pt::new(1_040.0, 360.0))].into_iter().enumerate() {
        let now = 10_000 * (i as u64 + 1);
        let (lasso, first, last) = arm(&stroke, &mut regions, &file, i as u64 + 1, now);
        assert!(first >= 11 && last <= 13, "the loops are round line 12: {first} to {last}");

        // Press inside the loop, hold, then move off: the region lifts and follows the pointer.
        let inside = Pt::new(fx::WORD_CENTRE.0, fx::WORD_CENTRE.1);
        let at = Pointer { page: 0, in_page: inside, in_window: Pt::new(inside.x + 100.0, inside.y + 50.0) };
        let region = match drag.press(&regions, at, now + 500) {
            Press::Inside(id) => id,
            other => panic!("a press inside the loop: {other:?}"),
        };
        assert_eq!(drag.tick(now + 500 + HOLD_MS), Some(Tick::Held(region)));
        assert!(matches!(drag.motion(Pt::new(at.in_window.x + 40.0, at.in_window.y), now + 900, &bar), Motion::Lifted(r) if r == region));
        assert!(matches!(drag.motion(drop, now + 1_000, &bar), Motion::Dragging { over_bar: true, .. }));
        let at_bar = match drag.release(drop, &bar) {
            Release::Dropped { region: r, at_bar } if r == region => at_bar,
            other => panic!("dropped on the bar: {other:?}"),
        };
        let taken = regions.take(region).expect("armed");

        // The picture: the plan at 2x, layers (the code is a block of ink), composed and cut along the loop.
        let p = plan(&lasso.polygon, 2.0, PAGE, &LIMITS).expect("a plan");
        let code = Raster::filled(p.px_w, p.px_h, [20, 20, 20, 255]);
        let mut ink = Raster::new(p.px_w, p.px_h);
        ink.over(&Raster::filled(8, 8, [214, 45, 32, 255]), 10, 10);
        let picture = compose(&p, [255, 255, 255, 255], None, &code, Some(&ink)).expect("composed");
        assert_eq!(picture.get(0, 0)[3], 0, "outside the loop: clear");
        let png = fake_png(&picture);
        let image = picture.into_shared_image(p.scale, png);

        let source = Source {
            rev: String::new(),
            sha256: String::new(),
            file: "src/lib.rs".to_string(),
            page: 0,
            first_line: taken.first_line,
            last_line: taken.last_line,
            shape: taken.polygon.clone(),
            file_hash: lines_hash(&file),
            range_hash: lines_hash(&kept_lines(&file[(taken.first_line as usize - 1)..taken.last_line as usize], &LIMITS)),
            lines: kept_lines(&file[(taken.first_line as usize - 1)..taken.last_line as usize], &LIMITS),
        };
        let board_at = board.view().to_board(at_bar);
        let id = board.add_cutout(CutoutDraft::ready(board_at, image, source)).expect("room on the board");
        made.push(id);
    }
    assert!(regions.is_empty());
    assert_eq!(board.len(), 2);
    assert!(board.picture_pixels() > 0);

    // An arrow from the first to the second, bent as a hand bends one.
    let arrow = board.add_arrow(End::item(made[0]), End::item(made[1]), 0.2, "calls").expect("an arrow");
    board.set_label(made[0], "the step").expect("a label");
    let placed = board.place_arrow(arrow).expect("placed");
    assert!(board.cutout(made[0]).is_some_and(|c| c.rect.contains(placed.from) || c.rect.inflate(1.0).contains(placed.from)));

    // Kept in a file and read back: the same board.
    let name = board_name("/work/azul");
    assert!(name.starts_with("boards/azul-"));
    let meta = crate::whiteboard_azboard::Meta { workspace_name: "azul".to_string(), modified_ms: 1_700_000_000_000, ..Default::default() };
    let entries = write(&board.snapshot(), &meta);
    assert_eq!(entries.iter().filter(|e| e.name != MANIFEST).count(), 2, "a picture for each cutout");
    let by_name: BTreeMap<String, Vec<u8>> = entries.iter().map(|e| (e.name.clone(), e.bytes.clone())).collect();
    let loaded = read(&by_name[MANIFEST], &|n| by_name.get(n).cloned(), &Codec, &LIMITS).expect("read");
    assert_eq!((loaded.left_out, loaded.unreadable_pictures), (0, 0));
    let (again, notices) = Board::from_data(loaded.data, LIMITS);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(again.snapshot().items.len(), board.snapshot().items.len());
    for (a, b) in again.items().iter().zip(board.items()) {
        match (a, b) {
            (Item::Cutout(x), Item::Cutout(y)) => {
                assert_eq!((x.id, x.rect, &x.label, &x.source), (y.id, y.rect, &y.label, &y.source));
                let (ImageSlot::Ready(xi), ImageSlot::Ready(yi)) = (&x.image, &y.image) else { panic!("pictures") };
                assert_eq!((xi.width, xi.height, xi.scale), (yi.width, yi.height, yi.scale));
                for (p, q) in xi.rgba.iter().zip(&yi.rgba) {
                    assert!((i32::from(*p) - i32::from(*q)).abs() <= 3, "a pixel changed by the trip");
                }
            }
            (x, y) => assert_eq!(x, y),
        }
    }
    let mut again = again;

    // The jump: the file as it was, then with five lines added at the top, then with the lines rewritten.
    let mut files = Files(BTreeMap::from([("src/lib.rs".to_string(), file.clone())]));
    let t = jump(&mut again, made[0], &files).expect("a jump");
    assert_eq!((t.freshness, t.page(46)), (Freshness::Fresh, 0));
    let first = t.first_line;
    let mut shifted: Vec<String> = (0..5).map(|i| format!("// header {i}")).collect();
    shifted.extend(file.clone());
    files.0.insert("src/lib.rs".to_string(), shifted);
    let t = jump(&mut again, made[0], &files).expect("a jump");
    assert_eq!(t.freshness, Freshness::LinesMoved);
    assert_eq!(t.first_line, first + 5, "five lines further down");
    let mut rewritten = file.clone();
    for l in &mut rewritten[10..14] {
        *l = "// gone".to_string();
    }
    files.0.insert("src/lib.rs".to_string(), rewritten);
    assert_eq!(jump(&mut again, made[0], &files).expect("a jump").freshness, Freshness::LinesChanged);
    files.0.clear();
    assert_eq!(jump(&mut again, made[0], &files).expect("a jump").freshness, Freshness::FileMissing);

    // What the board's observer (the recorder) heard of the first board, in order.
    let kinds: Vec<&str> = events
        .borrow()
        .iter()
        .map(|e| match e {
            BoardEvent::CutoutCreated { .. } => "cutout",
            BoardEvent::ArrowDrawn { .. } => "arrow",
            BoardEvent::Labelled { .. } => "label",
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(kinds, vec!["cutout", "cutout", "arrow", "label"]);

    // And a step of the board can be taken back and brought again after all of this.
    assert!(board.undo() && board.undo(), "the label and the arrow");
    assert_eq!(board.len(), 2);
    assert!(board.redo() && board.redo());
    assert_eq!(board.len(), 3);
}
