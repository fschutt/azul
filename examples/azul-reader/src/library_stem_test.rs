use super::safe_file_stem;

#[test]
fn a_title_becomes_a_file_name_without_what_a_file_name_cannot_hold() {
    assert_eq!(safe_file_stem("Alice's Adventures"), "Alice's Adventures");
    assert_eq!(safe_file_stem("a/b\\c:d*e?f\"g<h>i|j"), "a_b_c_d_e_f_g_h_i_j");
    assert_eq!(safe_file_stem("  ..hidden.. "), "hidden");
    assert_eq!(safe_file_stem("tab\there"), "tab_here");
}

#[test]
fn a_title_with_nothing_left_is_called_book_and_a_long_one_is_cut() {
    assert_eq!(safe_file_stem(""), "book");
    assert_eq!(safe_file_stem(" . . "), "book", "the dots round a space go, and the space with them");
    assert_eq!(safe_file_stem("..."), "book");
    assert_eq!(safe_file_stem(&"x".repeat(300)).chars().count(), 80);
}
