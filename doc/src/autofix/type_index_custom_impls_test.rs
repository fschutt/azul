//! What autofix reads of the hand-written `impl Trait for Type` blocks of a source: the traits that
//! change what the bindings generate, among them `Send` and `Sync` (`unsafe impl`).

use super::extract_custom_impls_from_items;

fn impls_of(source: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let file = syn::parse_file(source).expect("the source parses");
    extract_custom_impls_from_items(&file.items)
}

#[test]
fn an_unsafe_impl_of_send_and_sync_is_a_custom_impl_of_the_type() {
    let found = impls_of(
        "pub struct Handle { ptr: *mut u8 }\n\
         // SAFETY: the state behind the pointer is shared on purpose.\n\
         unsafe impl Send for Handle {}\n\
         unsafe impl Sync for Handle {}\n\
         impl Clone for Handle { fn clone(&self) -> Self { Handle { ptr: self.ptr } } }\n",
    );
    assert_eq!(found["Handle"], ["Send", "Sync", "Clone"]);
}

#[test]
fn a_type_without_them_has_none_and_the_other_traits_are_still_read() {
    let found = impls_of("pub struct Plain(u8);\nimpl Default for Plain { fn default() -> Self { Plain(0) } }\n");
    assert_eq!(found["Plain"], ["Default"]);
    assert!(!found["Plain"].iter().any(|t| t == "Send" || t == "Sync"));
    // An impl of a trait that does not matter to the bindings is not recorded.
    let other = impls_of("pub struct T;\nimpl std::fmt::Write for T { fn write_str(&mut self, _: &str) -> std::fmt::Result { Ok(()) } }\n");
    assert!(other.get("T").is_none());
}
