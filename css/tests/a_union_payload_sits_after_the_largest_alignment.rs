//! Rust's layout of a `#[repr(C, u8)]` enum, the rule every binding's
//! tagged-union layout mirrors (`doc/src/codegen/v2/c_layout.rs`,
//! `union_payload_layout`).
//!
//! The Rust reference ("Primitive representation of enums with fields",
//! combined with `repr(C)`) lays such an enum out as
//! `struct { u8 tag; union { one repr(C) struct per variant } }`: EVERY
//! variant's payload starts at the tag's size rounded up to the largest
//! alignment of ANY variant, not at the payload's own alignment. azul.h
//! used to declare `{ uint8_t tag; AzColorU payload; }` per variant, which
//! put a 1-aligned payload at offset 1 while Rust keeps it at 8.
//!
//! These values are read byte by byte (through raw pointers, never through
//! the padding) so the test pins what the DLL really hands a C caller.

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::color::ColorU, layout::display::LayoutDisplay, property::CssProperty,
        style::background::StyleBackgroundContent,
    },
};

/// Reads `N` initialized bytes of `value` at byte offset `at`.
fn bytes_at<T, const N: usize>(value: &T, at: usize) -> [u8; N] {
    assert!(at + N <= core::mem::size_of::<T>());
    // SAFETY: in bounds (asserted above), and the callers only read bytes
    // Rust initialized (the tag or a payload field, never padding).
    unsafe { core::ptr::read_unaligned((value as *const T as *const u8).add(at) as *const [u8; N]) }
}

#[test]
fn a_union_payload_sits_after_the_largest_alignment() {
    // The gradient variants hold Vecs (pointers), so the union is 8-aligned
    // and every payload - even ColorU's four 1-aligned bytes - starts at 8.
    assert_eq!(core::mem::align_of::<StyleBackgroundContent>(), 8);
    let value = StyleBackgroundContent::Color(ColorU { r: 1, g: 2, b: 3, a: 4 });

    // Tag: `Color` is the fifth variant.
    assert_eq!(bytes_at::<_, 1>(&value, 0), [4]);
    assert_eq!(
        bytes_at::<_, 4>(&value, 8),
        [1, 2, 3, 4],
        "the ColorU payload of StyleBackgroundContent::Color starts at offset 8, after the \
         largest alignment of any variant, not at offset 1 right after the u8 tag"
    );
}

#[test]
fn a_nested_union_payload_sits_after_the_largest_alignment_at_every_level() {
    // `CssProperty` is 8-aligned (many variants hold Vecs), so the
    // `CssPropertyValue<LayoutDisplay>` payload of `Display` starts at 8
    // although its own alignment is 4 (the int-sized `repr(C)` LayoutDisplay).
    assert_eq!(core::mem::align_of::<CssProperty>(), 8);
    assert_eq!(core::mem::align_of::<CssPropertyValue<LayoutDisplay>>(), 4);
    let value = CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Flex));

    // The inner union's tag at 8: `Exact` is its seventh variant.
    assert_eq!(
        bytes_at::<_, 1>(&value, 8),
        [6],
        "the CssPropertyValue payload of CssProperty::Display starts at offset 8, not 4"
    );
    // The inner payload at 8 + 4: the inner union is 4-aligned.
    assert_eq!(
        u32::from_ne_bytes(bytes_at::<_, 4>(&value, 12)),
        LayoutDisplay::Flex as u32
    );
}
