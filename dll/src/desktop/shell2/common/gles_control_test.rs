//! The ladder of `gles_control` against Mesa's OpenGL ES 3 on a host (`scripts/ci/gles_host.sh`'s conditions: `AZ_GLES_HOST_TESTS=1`,
//! `EGL_PLATFORM=surfaceless`): on a driver that works every phase reads back what it says, so a wrong row on a phone is the phone's.

use super::*;
use crate::desktop::shell2::common::gles_host_test::{gles_or_skip, Framebuffer};

#[test]
fn a_row_is_wrong_when_a_pixel_is_off_or_an_error_is_left() {
    let good = row("t", &[("a", RED, [254, 1, 0, 255])], 0, "");
    assert!(good.ok, "{}", good.detail);
    let off = row("t", &[("a", RED, [0, 0, 0, 0])], 0, "");
    assert!(!off.ok);
    assert!(off.detail.contains("expect #ff0000ff got #00000000"), "{}", off.detail);
    let errored = row("t", &[("a", RED, RED)], 0x502, "");
    assert!(!errored.ok);
    assert!(errored.detail.contains("glGetError 0x502"), "{}", errored.detail);
}

#[test]
fn the_phases_have_the_names_the_log_and_the_report_use() {
    assert_eq!(
        PHASES,
        [
            "clear",
            "shader",
            "dummy_texture",
            "fbo_sample",
            "instanced",
            "flat_lowp",
            "quad_u8norm",
            "external_text",
            "big_texture",
            "texture_size",
            "data_textures",
            "pbo_upload",
            "vertex_fetch",
            "scatter",
            "depth",
            "depth_fine",
            "shared_depth"
        ]
    );
}

#[test]
fn the_ladder_reads_back_what_it_draws_on_a_working_driver() {
    let name = "the_ladder_reads_back_what_it_draws_on_a_working_driver";
    let Some(gles) = gles_or_skip(name) else {
        return;
    };
    let gl = gles.gl();
    let framebuffer = Framebuffer::new(gl.clone(), 400, 560).expect("an offscreen window framebuffer");
    // The ladder binds the framebuffers twice over: as `GL_FRAMEBUFFER`, and the way WebRender's device does (draw and read apart).
    for (split, external_unbind) in [(false, false), (true, false), (true, true)] {
        let target = Target {
            size: (400, 560),
            window: framebuffer.id(),
            split,
            external_unbind,
        };
        let mut shown: Vec<&'static str> = Vec::new();
        let ladder = run_ladder(&gl, target, &mut |phase| shown.push(phase));
        for row in &ladder.rows {
            eprintln!(
                "control (split {split}) {}: {} ({})",
                row.name,
                if row.ok { "ok" } else { "WRONG" },
                row.detail
            );
        }
        assert!(ladder.program_problems.is_empty(), "programs that did not build: {:?}", ladder.program_problems);
        for row in &ladder.rows {
            assert!(row.ok, "split {split}, {}: {}", row.name, row.detail);
        }
        assert_eq!(ladder.rows.len(), PHASES.len());
        // Every phase is shown twice (drawn, swapped, drawn again, swapped), in the order of the ladder.
        let expected: Vec<&str> = PHASES.iter().flat_map(|p| [*p, *p]).collect();
        assert_eq!(shown, expected);
    }
}
