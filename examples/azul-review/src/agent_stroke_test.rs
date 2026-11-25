//! A stroke as the agent's SVG: cleaned, thinned, simplified, with its widths and times; no window.

use super::*;

fn sample(x: f32, y: f32, pressure: f32, dt_ms: u32) -> Sample {
    Sample {
        x,
        y,
        pressure,
        dt_ms,
        tilt: None,
    }
}

fn stroke(id: u64, samples: Vec<Sample>) -> StrokeInput {
    StrokeInput {
        id,
        page: 0,
        meaning: Some(Meaning::Issue),
        pinned: true,
        nib: "pen".to_string(),
        nib_width: 2.0,
        t0_ms: 1_000,
        samples,
    }
}

fn shape_of(s: &StrokeInput) -> (Shape, Vec<String>) {
    let mut notes = Notes::new();
    let shape = shape(s, &Limits::default(), &mut notes);
    (shape, notes.finish())
}

#[test]
fn numbers_are_written_with_one_decimal_at_most() {
    assert_eq!(fmt_num(120.5), "120.5");
    assert_eq!(fmt_num(131.0), "131");
    assert_eq!(fmt_num(0.0), "0");
    assert_eq!(fmt_num(-0.04), "0", "no negative zero");
    assert_eq!(fmt_num(-3.26), "-3.3");
    assert_eq!(fmt_num(99.949), "99.9");
    assert_eq!(fmt_num(99.96), "100");
    assert_eq!(fmt_num(1234.0), "1234");
}

#[test]
fn collinear_points_simplify_to_the_ends() {
    let pts: Vec<(f32, f32)> = (0..50).map(|i| (i as f32 * 2.0, 10.0)).collect();
    assert_eq!(simplify_indices(&pts, 0.5), vec![0, 49]);
}

#[test]
fn a_corner_stays_when_it_is_further_than_the_tolerance() {
    let pts = vec![
        (0.0, 0.0),
        (5.0, 0.0),
        (10.0, 0.0),
        (10.0, 5.0),
        (10.0, 10.0),
    ];
    assert_eq!(simplify_indices(&pts, 0.5), vec![0, 2, 4]);
}

#[test]
fn jitter_under_the_tolerance_is_smoothed_away() {
    let pts: Vec<(f32, f32)> = (0..40)
        .map(|i| (i as f32 * 3.0, if i % 2 == 0 { 0.2 } else { -0.2 }))
        .collect();
    assert_eq!(simplify_indices(&pts, 0.8), vec![0, 39]);
}

#[test]
fn two_points_and_one_point_are_kept_whole() {
    assert_eq!(simplify_indices(&[(0.0, 0.0), (1.0, 1.0)], 1.0), vec![0, 1]);
    assert_eq!(simplify_indices(&[(3.0, 3.0)], 1.0), vec![0]);
    assert!(simplify_indices(&[], 1.0).is_empty());
}

#[test]
fn a_circle_keeps_enough_points_to_stay_round() {
    let pts: Vec<(f32, f32)> = (0..=100)
        .map(|i| {
            let a = i as f32 / 100.0 * std::f32::consts::TAU;
            (100.0 + 40.0 * a.cos(), 100.0 + 40.0 * a.sin())
        })
        .collect();
    let kept = simplify_indices(&pts, 0.8);
    assert!(
        kept.len() >= 12 && kept.len() < 60,
        "{} points kept",
        kept.len()
    );
    assert_eq!(kept.first(), Some(&0));
    assert_eq!(kept.last(), Some(&100));
}

#[test]
fn a_straight_stroke_becomes_two_points_with_their_times_and_widths() {
    let s = stroke(
        7,
        (0..20)
            .map(|i| sample(10.0 + i as f32 * 5.0, 50.0, 0.5, i * 10))
            .collect(),
    );
    let (shape, notes) = shape_of(&s);
    assert_eq!(shape.points.len(), 2);
    assert_eq!((shape.points[0].x, shape.points[0].t_ms), (10.0, 0));
    assert_eq!((shape.points[1].x, shape.points[1].t_ms), (105.0, 190));
    assert_eq!(shape.duration_ms, 190);
    assert_eq!(shape.samples_in, 20);
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn the_width_follows_the_pressure_between_a_third_and_the_whole_nib() {
    let s = stroke(
        1,
        vec![sample(0.0, 0.0, 0.0, 0), sample(100.0, 0.0, 1.0, 100)],
    );
    let (shape, _) = shape_of(&s);
    assert!(
        (shape.points[0].w - 0.7).abs() < 1e-4,
        "{}",
        shape.points[0].w
    );
    assert!(
        (shape.points[1].w - 2.0).abs() < 1e-4,
        "{}",
        shape.points[1].w
    );
    assert!((shape.mean_pressure - 0.5).abs() < 1e-4);
    assert!((shape.peak_pressure - 1.0).abs() < 1e-4);
}

#[test]
fn a_single_sample_is_a_dot_with_a_zero_length_path() {
    let s = stroke(2, vec![sample(30.0, 40.0, 0.6, 0)]);
    let (shape, _) = shape_of(&s);
    assert_eq!(shape.points.len(), 1);
    let doc = svg_document(&[(&s, shape)]);
    assert!(doc.contains("d=\"M30 40 L30 40\""), "{doc}");
}

#[test]
fn a_stroke_without_a_readable_sample_has_no_points_and_says_so() {
    let s = stroke(
        3,
        vec![
            sample(f32::NAN, 1.0, 0.5, 0),
            sample(1.0, f32::INFINITY, 0.5, 5),
        ],
    );
    let (shape, notes) = shape_of(&s);
    assert!(shape.points.is_empty());
    assert!(notes.iter().any(|n| n.contains("stroke 3")), "{notes:?}");
    assert_eq!(svg_document(&[(&s, shape)]), "");
}

#[test]
fn unreadable_samples_are_dropped_the_rest_kept_and_it_is_said_once() {
    let s = stroke(
        4,
        vec![
            sample(0.0, 0.0, 0.5, 0),
            sample(f32::NAN, 0.0, 0.5, 10),
            sample(20.0, 20.0, 0.5, 20),
            sample(f32::NEG_INFINITY, 0.0, 0.5, 30),
            sample(40.0, 0.0, 0.5, 40),
        ],
    );
    let (shape, notes) = shape_of(&s);
    assert_eq!(shape.samples_in, 3);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("2 unreadable"), "{notes:?}");
}

#[test]
fn a_coordinate_beyond_any_page_is_held_to_the_limit() {
    let s = stroke(
        5,
        vec![sample(1.0e30, -1.0e30, 0.5, 0), sample(10.0, 10.0, 0.5, 5)],
    );
    let (shape, notes) = shape_of(&s);
    assert!(shape
        .points
        .iter()
        .all(|p| p.x.abs() <= MAX_COORD && p.y.abs() <= MAX_COORD));
    assert!(notes.iter().any(|n| n.contains("beyond")), "{notes:?}");
}

#[test]
fn pressure_is_held_to_zero_to_one_and_a_missing_one_is_half() {
    let s = stroke(
        6,
        vec![
            sample(0.0, 0.0, -3.0, 0),
            sample(50.0, 50.0, 7.0, 10),
            sample(100.0, 0.0, f32::NAN, 20),
        ],
    );
    let (shape, _) = shape_of(&s);
    assert_eq!(shape.points.len(), 3);
    assert!((shape.peak_pressure - 1.0).abs() < 1e-4);
    assert!(
        (shape.points[0].w - 0.7).abs() < 1e-4,
        "pressure -3 is 0: {}",
        shape.points[0].w
    );
    assert!(
        (shape.points[2].w - 2.0 * (0.35 + 0.65 * 0.5)).abs() < 1e-4,
        "NaN is 0.5: {}",
        shape.points[2].w
    );
}

#[test]
fn a_time_that_runs_backwards_stands_still() {
    let s = stroke(
        8,
        vec![
            sample(0.0, 0.0, 0.5, 100),
            sample(50.0, 40.0, 0.5, 40),
            sample(100.0, 0.0, 0.5, 300),
        ],
    );
    let (shape, _) = shape_of(&s);
    let times: Vec<u32> = shape.points.iter().map(|p| p.t_ms).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
}

#[test]
fn a_huge_stroke_is_thinned_then_simplified_within_the_point_cap() {
    let n = 200_000;
    let samples: Vec<Sample> = (0..n)
        .map(|i| {
            let a = i as f32 * 0.01;
            sample(
                300.0 + a.cos() * (50.0 + a),
                300.0 + a.sin() * (50.0 + a),
                0.5,
                i as u32,
            )
        })
        .collect();
    let s = stroke(9, samples);
    let limits = Limits::default();
    let mut notes = Notes::new();
    let shape = shape(&s, &limits, &mut notes);
    assert!(
        shape.points.len() <= limits.max_svg_points,
        "{}",
        shape.points.len()
    );
    assert!(shape.points.len() >= 8);
    let notes = notes.finish();
    assert!(notes.iter().any(|n| n.contains("thinned")), "{notes:?}");
    assert_eq!(shape.points.first().map(|p| p.t_ms), Some(0));
    assert_eq!(
        shape.points.last().map(|p| p.t_ms),
        Some(u32::try_from(n - 1).unwrap_or(u32::MAX))
    );
}

#[test]
fn the_override_of_the_limits_keeps_more_points() {
    let samples: Vec<Sample> = (0..500)
        .map(|i| {
            let a = i as f32 * 0.07;
            sample(200.0 + a.cos() * 80.0, 200.0 + a.sin() * 80.0, 0.5, i * 4)
        })
        .collect();
    let s = stroke(10, samples);
    let tight = Limits {
        max_svg_points: 20,
        ..Limits::default()
    };
    let mut notes = Notes::new();
    let few = shape(&s, &tight, &mut notes);
    assert!(few.points.len() <= 20);
    let mut notes = Notes::new();
    let more = shape(&s, &Limits::default(), &mut notes);
    assert!(more.points.len() > few.points.len());
}

#[test]
fn the_bounding_box_covers_the_kept_points() {
    let s = stroke(
        11,
        vec![
            sample(10.0, 40.0, 0.5, 0),
            sample(60.0, 5.0, 0.5, 10),
            sample(90.0, 70.0, 0.5, 20),
        ],
    );
    let (shape, _) = shape_of(&s);
    assert_eq!(shape.bbox, [10.0, 5.0, 90.0, 70.0]);
}

#[test]
fn the_document_has_a_path_per_stroke_with_what_the_agent_reads() {
    let a = stroke(
        12,
        vec![
            sample(10.0, 10.0, 0.4, 0),
            sample(60.0, 10.0, 0.8, 100),
            sample(60.0, 50.0, 0.8, 200),
        ],
    );
    let mut b = stroke(
        13,
        vec![sample(5.0, 90.0, 0.5, 0), sample(80.0, 95.0, 0.5, 50)],
    );
    b.meaning = None;
    b.pinned = false;
    b.nib = "mark\"er <b>".to_string();
    let (sa, _) = shape_of(&a);
    let (sb, _) = shape_of(&b);
    let doc = svg_document(&[(&a, sa), (&b, sb)]);
    assert!(
        doc.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\""),
        "{doc}"
    );
    assert!(doc.ends_with("</svg>"), "{doc}");
    assert_eq!(doc.matches("<path ").count(), 2);
    assert!(doc.contains("id=\"s12\""));
    assert!(doc.contains("d=\"M10 10 L60 10 L60 50\""), "{doc}");
    assert!(doc.contains("data-t=\"0 100 200\""), "{doc}");
    assert!(
        doc.contains("data-meaning=\"issue\"") && doc.contains("data-pinned=\"true\""),
        "{doc}"
    );
    assert!(doc.contains("data-t0-ms=\"1000\""), "{doc}");
    assert!(
        doc.contains("data-meaning=\"unknown\"") && doc.contains("data-pinned=\"false\""),
        "{doc}"
    );
    assert!(
        doc.contains("data-nib=\"mark&quot;er &lt;b&gt;\""),
        "the nib name is escaped: {doc}"
    );
    assert!(doc.contains("data-pressure-mean=\"0.7\""), "{doc}");
    assert!(doc.contains("data-pressure-peak=\"0.8\""), "{doc}");
    // the view box covers both strokes with a margin
    assert!(doc.contains("viewBox=\"-3 2 91 101\""), "{doc}");
}

#[test]
fn xml_escaping_covers_the_five_characters() {
    assert_eq!(xml_attr("a&b<c>d\"e'f"), "a&amp;b&lt;c&gt;d&quot;e&apos;f");
    assert_eq!(xml_attr("plain"), "plain");
}

#[test]
fn the_meanings_read_back_by_their_words() {
    for m in Meaning::ALL {
        assert_eq!(Meaning::from_key(m.key()), Some(m));
        assert!(!m.gloss().is_empty());
    }
    assert_eq!(Meaning::from_key(" Issue "), Some(Meaning::Issue));
    assert_eq!(Meaning::from_key("other"), None);
}
