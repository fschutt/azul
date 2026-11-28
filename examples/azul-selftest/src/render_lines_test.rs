//! The engine's [render] lines and the verdict on them. No device.

use super::*;

fn lines(of: &[&str]) -> Vec<String> {
    of.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn the_backend_line_gives_renderer_request_and_driver_wherever_the_prefix_puts_it() {
    let b = parse_backend("[    120us][Info][Rendering] [render] backend=gpu requested=auto device=Mali-G715 MC7 (OpenGL ES 3.2)").unwrap();
    assert_eq!((b.ran.as_str(), b.requested.as_str(), b.device.as_str()), ("gpu", "auto", "Mali-G715 MC7 (OpenGL ES 3.2)"));
    let cpu = parse_backend("[render] backend=cpu requested=cpu device=software").unwrap();
    assert_eq!((cpu.ran.as_str(), cpu.device.as_str()), ("cpu", "software"));
    assert_eq!(parse_backend("[render] first GPU frame: 812 ms"), None);
    assert_eq!(parse_backend("nothing about it"), None);
}

#[test]
fn the_frames_line_gives_the_interval_and_the_draw_time() {
    let f = parse_frames("[render] frames n=57 over_ms=1000 interval_ms median=16.7 p95=18.0 max=33.1 draw_ms median=4.2 p95=7.0").unwrap();
    assert_eq!(f.n, 57);
    assert_eq!((f.interval_median, f.interval_p95, f.interval_max), (16.7, 18.0, 33.1));
    assert_eq!((f.draw_median, f.draw_p95), (4.2, 7.0));
    assert_eq!(parse_frames("[render] frames n=57 with no times"), None);
}

/// The row answers 'which backend ran, which driver, ms per frame': a backend line is a PASS and the text holds all three.
#[test]
fn a_gpu_that_ran_is_a_pass_with_the_driver_and_the_frame_times() {
    let log = lines(&[
        "[render] backend=gpu requested=gpu device=Mali-G715",
        "[render] first GPU frame: 812 ms",
        "[render] frames n=58 over_ms=1000 interval_ms median=16.7 p95=17.5 max=20.1 draw_ms median=3.1 p95=5.0",
        "[render] frames n=60 over_ms=1000 interval_ms median=16.6 p95=17.0 max=18.0 draw_ms median=3.0 p95=4.5",
    ]);
    let Judged::Pass(text) = judge(&log, "app: median 16.6 ms") else { panic!("not a pass") };
    assert!(text.starts_with("gpu (requested gpu) on Mali-G715"), "{text}");
    assert!(text.contains("first GPU frame 812 ms"), "{text}");
    assert!(text.contains("60 frames in the last second: interval median 16.6"), "{text}: the LAST frames line");
    assert!(text.ends_with("app: median 16.6 ms"), "{text}");
}

/// Asking for the GPU and getting the CPU is the failure of this row, with the engine's own reason.
///
/// Mutation: `requested == "gpu" && ran == "cpu"` judged a pass (a phone that silently drew on the CPU looked fine).
#[test]
fn a_gpu_that_was_asked_for_and_a_cpu_that_ran_is_a_fail_with_the_engines_reason() {
    let log = lines(&[
        "[render] gpu unavailable at egl_create_context: EGL_BAD_ATTRIBUTE; drawing on the CPU",
        "[render] backend=cpu requested=gpu device=software",
    ]);
    let Judged::Fail(text) = judge(&log, "") else { panic!("not a fail") };
    assert!(text.contains("requested gpu, ran cpu: gpu unavailable at egl_create_context: EGL_BAD_ATTRIBUTE; drawing on the CPU"), "{text}");
    let no_reason = lines(&["[render] backend=cpu requested=gpu device=software"]);
    let Judged::Fail(text) = judge(&no_reason, "") else { panic!("not a fail") };
    assert!(text.contains("no 'gpu unavailable at' line before it"), "{text}");
}

/// A CPU that was asked for, or that `auto` chose, is not a failure.
#[test]
fn a_cpu_that_was_asked_for_or_chosen_by_auto_is_a_pass() {
    for line in ["[render] backend=cpu requested=cpu device=software", "[render] backend=cpu requested=auto device=software"] {
        assert!(matches!(judge(&lines(&[line]), ""), Judged::Pass(_)), "{line}");
    }
}

#[test]
fn no_backend_line_is_no_verdict() {
    assert_eq!(judge(&lines(&["[render] frames n=3"]), "x"), Judged::NoLines);
    assert_eq!(judge(&[], ""), Judged::NoLines);
}

#[test]
fn the_spread_of_timings_is_median_p95_and_worst_in_milliseconds() {
    assert_eq!(spread_ms(&[]), None);
    let (median, p95, max) = spread_ms(&[16_000, 17_000, 16_500, 40_000, 16_000]).unwrap();
    assert_eq!((median, max), (16_500.0 / 1000.0, 40.0));
    assert!(p95 >= median && p95 <= max);
}
