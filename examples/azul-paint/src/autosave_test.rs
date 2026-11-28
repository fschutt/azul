//! The drawing written when the app is paused is the drawing read back, and a file that is not one is no drawing.
//! No window.

use crate::{
    autosave::{decode, encode, MAX_POINTS_PER_STROKE},
    ColorU, Stroke, StrokePoint,
};

fn point(x: f32, y: f32) -> StrokePoint {
    StrokePoint { x, y, pressure: 0.5, tilt_x: -0.25, tilt_y: 0.125, barrel_roll_rad: 1.5 }
}

fn stroke(points: Vec<StrokePoint>, eraser: bool) -> Stroke {
    Stroke { points, color: ColorU { r: 30, g: 40, b: 250, a: 255 }, is_eraser: eraser }
}

fn same(a: &[Stroke], b: &[Stroke]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.is_eraser == y.is_eraser
                && (x.color.r, x.color.g, x.color.b, x.color.a) == (y.color.r, y.color.g, y.color.b, y.color.a)
                && x.points.len() == y.points.len()
                && x.points.iter().zip(&y.points).all(|(p, q)| {
                    (p.x, p.y, p.pressure, p.tilt_x, p.tilt_y, p.barrel_roll_rad)
                        == (q.x, q.y, q.pressure, q.tilt_x, q.tilt_y, q.barrel_roll_rad)
                })
        })
}

#[test]
fn a_drawing_comes_back_as_it_was_written() {
    let drawing = vec![
        stroke(vec![point(1.0, 2.0), point(3.5, 4.25), point(1e-3, 123456.75)], false),
        stroke(vec![point(0.1, 0.2)], true),
        stroke(Vec::new(), false),
    ];
    let back = decode(&encode(&drawing)).expect("the file it wrote parses");
    assert!(same(&drawing, &back), "every stroke, its colour, its eraser flag and every number of every point");
}

#[test]
fn an_empty_canvas_is_a_file_too() {
    // a cleared drawing must stay cleared at the next start
    let back = decode(&encode(&[])).expect("an empty drawing parses");
    assert!(back.is_empty());
}

#[test]
fn a_file_that_is_not_a_drawing_is_no_drawing() {
    assert!(decode("").is_none(), "empty");
    assert!(decode("some other file\n").is_none(), "another header");
    assert!(decode("azpaint-autosave 2\n").is_none(), "another version");
    assert!(decode("azpaint-autosave 1\nline 1 2 3\n").is_none(), "a line that is no stroke");
    assert!(decode("azpaint-autosave 1\nstroke 1 2 3 4 0 x\n").is_none(), "a count that is no number");
    assert!(decode("azpaint-autosave 1\nstroke 300 2 3 4 0 0\n").is_none(), "a colour out of the byte range");
}

#[test]
fn a_file_that_is_cut_off_is_no_drawing() {
    let whole = encode(&[stroke(vec![point(1.0, 2.0), point(3.0, 4.0)], false)]);
    // the process was ended while the file was written: the last point is gone, and so is half of the one before
    let cut = &whole[..whole.rfind('\n').expect("a last line") - 10];
    assert!(decode(cut).is_none());
    assert!(decode("azpaint-autosave 1\nstroke 1 2 3 4 0 2\n1 2 3 4 5 6\n").is_none(), "one point of two");
    assert!(decode("azpaint-autosave 1\nstroke 1 2 3 4 0 1\n1 2 3 4 5\n").is_none(), "a point of five numbers");
    assert!(decode("azpaint-autosave 1\nstroke 1 2 3 4 0 1\n1 2 3 NaN 5 6\n").is_none(), "a number that is not finite");
}

#[test]
fn a_count_that_would_fill_the_memory_is_refused_before_anything_is_allocated() {
    let hostile = format!("azpaint-autosave 1\nstroke 1 2 3 4 0 {}\n", MAX_POINTS_PER_STROKE + 1);
    assert!(decode(&hostile).is_none());
    let huge = format!("azpaint-autosave 1\nstroke 1 2 3 4 0 {}\n", usize::MAX);
    assert!(decode(&huge).is_none());
}

#[test]
fn blank_lines_between_strokes_are_ignored() {
    let one = encode(&[stroke(vec![point(1.0, 2.0)], false)]);
    let body = one.strip_prefix("azpaint-autosave 1\n").expect("the header");
    let spaced = format!("azpaint-autosave 1\n\n{body}\n\n{body}");
    let back = decode(&spaced).expect("blank lines are fine");
    assert_eq!(back.len(), 2);
}
