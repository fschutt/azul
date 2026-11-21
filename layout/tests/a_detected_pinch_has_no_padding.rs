//! `DetectedPinch` crosses the FFI as `repr(C)`; its fields sit in decreasing
//! alignment so the layout holds no padding: a u64, a `LogicalPosition`
//! (2 x f32), three f32 and a bool = 29 bytes, rounded up to the 8-byte
//! alignment = 32. With the u64 after the four-byte fields and the bool last
//! it was 40 bytes: `azul-doc autofix` reported it as a critical FFI-safety
//! error on 2026-09-30 ("field order wastes ~8 bytes of padding") and the
//! error went unread through four codegen runs.

use azul_layout::managers::gesture::DetectedPinch;

#[test]
fn a_detected_pinch_has_no_padding() {
    assert_eq!(core::mem::size_of::<DetectedPinch>(), 32);
    assert_eq!(core::mem::align_of::<DetectedPinch>(), 8);
}
