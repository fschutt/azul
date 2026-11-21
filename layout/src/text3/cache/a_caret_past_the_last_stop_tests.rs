    use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};

    use super::UnifiedLayout;

    fn stop(byte: u32) -> GraphemeClusterId {
        GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        }
    }

    /// The end of "krug" written as the typed insertion leaves it - `(4, Leading)`, past the
    /// last stop - is offset 4, the same as `(3, Trailing)`: one step left is "g"'s start.
    #[test]
    fn the_end_of_a_typed_text_is_the_offset_after_its_last_stop() {
        let stops = [stop(0), stop(1), stop(2), stop(3)];
        let typed_end = TextCursor {
            cluster_id: stop(4),
            affinity: CursorAffinity::Leading,
        };
        let canonical_end = TextCursor {
            cluster_id: stop(3),
            affinity: CursorAffinity::Trailing,
        };
        let is_cluster = |id: &GraphemeClusterId| id.start_byte_in_run < 4;
        assert_eq!(
            UnifiedLayout::grapheme_caret_offset_in(&stops, &typed_end, &is_cluster),
            Some(4)
        );
        assert_eq!(
            UnifiedLayout::grapheme_caret_offset_in(&stops, &canonical_end, &is_cluster),
            Some(4)
        );
        // A caret on a folded mark (a cluster, no stop) still snaps back to its grapheme.
        let mark_stops = [stop(0), stop(1), stop(3)];
        let on_a_mark = TextCursor {
            cluster_id: stop(2),
            affinity: CursorAffinity::Leading,
        };
        assert_eq!(
            UnifiedLayout::grapheme_caret_offset_in(&mark_stops, &on_a_mark, &is_cluster),
            Some(1)
        );
        let left = UnifiedLayout::cursor_from_grapheme_offset(&stops, 3);
        assert_eq!(left.cluster_id, stop(3));
        assert_eq!(left.affinity, CursorAffinity::Leading);
    }
