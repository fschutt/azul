//! U3-a-i: an edit that changes the RUN COUNT still moves the peer and seat
//! carets to the text they were at. `run_text_diff` aligns the runs common
//! to both generations and maps the middle through its concatenated text.

use std::sync::Arc;

use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_layout::text3::cache::{InlineContent, StyleProperties, StyledRun};
use azul_layout::text3::edit::run_text_diff;

fn runs(texts: &[&str]) -> Vec<InlineContent> {
    let style = Arc::new(StyleProperties::default());
    let mut byte = 0;
    texts
        .iter()
        .map(|t| {
            let run = InlineContent::Text(StyledRun {
                text: Arc::from(*t),
                style: style.clone(),
                logical_start_byte: byte,
                source_node_id: None,
            });
            byte += t.len();
            run
        })
        .collect()
}

fn at(run: u32, byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: run,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

#[test]
fn a_delete_spanning_two_runs_merges_them_and_the_caret_follows_its_text() {
    // "hel|lo" + "wor|ld" -> "heorld" (the "lo" + "wor" between the bars gone).
    let old = runs(&["hello", "world"]);
    let new = runs(&["heorld"]);
    let diff = run_text_diff(&old, &new);
    let remap = diff.remap.as_ref().expect("a run-count change yields a remap");
    assert_eq!(remap.first, 0);
    assert_eq!(remap.old_lens, vec![5, 5]);
    assert_eq!(remap.new_lens, vec![6]);
    // A peer on the 'l' of "world" (run 1, byte 3) is on byte 4 of "heorld".
    assert_eq!(diff.map_cursor(at(1, 3)), at(0, 4));
    // A peer inside the deleted span lands where it starts.
    assert_eq!(diff.map_cursor(at(0, 4)), at(0, 2));
}

#[test]
fn a_split_and_a_merge_keep_carets_in_untouched_runs_by_shifting_their_index() {
    let old = runs(&["intro", "abcd", "outro"]);
    let new = runs(&["intro", "ab", "cd", "outro"]);
    let diff = run_text_diff(&old, &new);
    assert_eq!(diff.map_cursor(at(0, 2)), at(0, 2), "before the change: untouched");
    assert_eq!(diff.map_cursor(at(1, 3)), at(2, 1), "'d' moved into the second piece");
    assert_eq!(diff.map_cursor(at(2, 4)), at(3, 4), "after the change: index up by one");
    let back = run_text_diff(&new, &old);
    assert_eq!(back.map_cursor(at(2, 1)), at(1, 3));
    assert_eq!(back.map_cursor(at(3, 4)), at(2, 4));
}

#[test]
fn an_equal_run_count_still_yields_the_byte_changes_only() {
    let old = runs(&["hello", "world"]);
    let new = runs(&["hello", "wide world"]);
    let diff = run_text_diff(&old, &new);
    assert!(diff.remap.is_none());
    assert_eq!(diff.changes.len(), 1);
    assert_eq!(diff.map_cursor(at(1, 1)), at(1, 6), "'o' of world after the 5 inserted bytes");
}
