//! One-off size probe. Run with: cargo test -p azul-core --test sizes -- --nocapture

#![allow(dead_code)]

use azul_core::dom::*;
use azul_core::callbacks::*;
use azul_core::refany::RefAny;
use azul_core::styled_dom::*;

#[test]
fn print_layout_sizes() {
    println!();
    println!("=== Layout-phase struct sizes (bytes) ===");
    macro_rules! s { ($t:ty) => {
        println!("  {:>5}  {}", core::mem::size_of::<$t>(), stringify!($t));
    }}
    println!();
    println!("Tree shape:");
    s!(Dom);
    s!(NodeData);
    s!(NodeDataExt);
    s!(DomVec);
    s!(NodeDataVec);
    s!(NodeHierarchyItem);
    s!(NodeHierarchyItemVec);
    println!();
    println!("Node payload pieces:");
    s!(NodeType);
    s!(IdOrClass);
    s!(IdOrClassVec);
    s!(AttributeType);
    s!(AttributeTypeVec);
    s!(CoreCallbackData);
    s!(CoreCallbackDataVec);
    s!(NodeFlags);
    s!(AccessibilityInfo);
    s!(SmallAriaInfo);
    s!(ProgressAriaInfo);
    s!(MeterAriaInfo);
    s!(DialogAriaInfo);
    println!();
    println!("Aria roles / state:");
    s!(AccessibilityRole);
    s!(AccessibilityState);
    s!(AccessibilityAction);
    println!();
    println!("Refany / virtual view:");
    s!(RefAny);
    s!(VirtualViewCallback);
    s!(VirtualViewCallbackInfo);
    s!(VirtualViewReturn);
    println!();
    println!("Styled DOM (post-cascade):");
    s!(StyledDom);
    s!(StyledNodeState);
}
