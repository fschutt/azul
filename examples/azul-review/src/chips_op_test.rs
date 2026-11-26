//! The debug op that fills the bottom bar with mock conversations.

use super::asked;

#[test]
fn the_count_is_read_from_the_arguments_and_held_to_the_cap() {
    assert_eq!(asked("{\"count\":50}"), 50);
    assert_eq!(asked("{\"count\":99999}"), crate::chips::MAX_CHIPS);
    assert_eq!(asked("{}"), 0);
    assert_eq!(asked("not json"), 0);
    assert_eq!(asked("{\"count\":-3}"), 0);
}
