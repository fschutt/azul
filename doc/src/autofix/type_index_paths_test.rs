//! Where autofix puts a source file in the crate: the module path of `layout/src/widgets/sidebar.rs`
//! is `widgets::sidebar` however deep the checkout itself is, even in a folder named `src`.

use std::path::Path;

use super::infer_module_path;

#[test]
fn a_file_of_the_crate_is_named_by_the_path_after_its_own_src() {
    assert_eq!(
        infer_module_path("azul_layout", Path::new("/work/azul/layout/src/widgets/sidebar.rs")),
        "widgets::sidebar"
    );
    assert_eq!(infer_module_path("azul_layout", Path::new("/work/azul/layout/src/lib.rs")), "");
    assert_eq!(
        infer_module_path("azul_layout", Path::new("/work/azul/layout/src/widgets/mod.rs")),
        "widgets"
    );
}

/// The box checks out into `.../slot2/src/`: the first `/src/` of the path is the checkout's, and
/// every widget type became `layout::src::widgets::x::Type` and landed in `misc`.
#[test]
fn a_checkout_inside_a_folder_named_src_does_not_change_the_module_path() {
    assert_eq!(
        infer_module_path("azul_layout", Path::new("/home/fs/azul-box/slot2/src/layout/src/widgets/sidebar.rs")),
        "widgets::sidebar"
    );
    assert_eq!(
        infer_module_path("azul_core", Path::new("/home/fs/azul-box/slot2/src/core/src/dom.rs")),
        "dom"
    );
}

#[test]
fn a_path_with_no_src_has_no_module_path() {
    assert_eq!(infer_module_path("azul_layout", Path::new("/tmp/loose.rs")), "");
}
