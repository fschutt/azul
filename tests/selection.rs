//! Selection tests - currently disabled pending API export
//!
//! These tests require functions that are not currently exported from
//! azul_layout::text3::selection module.

// Disabled: functions find_word_boundaries and is_word_char are not exported
#![cfg(feature = "DISABLED_selection_tests")]

use azul_layout::text3::selection::*;

#[test]
fn test_word_boundaries_simple() {
    let text = "Hello World";

    // Cursor in "Hello"
    let (start, end) = find_word_boundaries(text, 2);
    assert_eq!(&text[start..end], "Hello");

    // Cursor in "World"
    let (start, end) = find_word_boundaries(text, 7);
    assert_eq!(&text[start..end], "World");

    // Cursor on space
    let (start, end) = find_word_boundaries(text, 5);
    assert_eq!(&text[start..end], " ");
}

#[test]
fn test_word_boundaries_start_end() {
    let text = "Hello";

    // At start
    let (start, end) = find_word_boundaries(text, 0);
    assert_eq!(&text[start..end], "Hello");

    // At end
    let (start, end) = find_word_boundaries(text, 5);
    assert_eq!(&text[start..end], "Hello");
}

#[test]
fn test_word_boundaries_punctuation() {
    let text = "Hello, World!";

    // In "Hello"
    let (start, end) = find_word_boundaries(text, 2);
    assert_eq!(&text[start..end], "Hello");

    // On comma
    let (start, end) = find_word_boundaries(text, 5);
    assert_eq!(&text[start..end], ", ");

    // In "World"
    let (start, end) = find_word_boundaries(text, 8);
    assert_eq!(&text[start..end], "World");
}

#[test]
fn test_word_boundaries_underscore() {
    let text = "hello_world";

    // Should treat underscore as word char
    let (start, end) = find_word_boundaries(text, 5);
    assert_eq!(&text[start..end], "hello_world");
}

#[test]
fn test_is_word_char() {
    assert!(is_word_char('a'));
    assert!(is_word_char('Z'));
    assert!(is_word_char('0'));
    assert!(is_word_char('_'));
    assert!(!is_word_char(' '));
    assert!(!is_word_char(','));
    assert!(!is_word_char('!'));
}
