    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Option<*>` wrappers
    use crate::dll::*;
    use std::ffi::c_void;


    /// `ResultSvgSvgParseError` struct
    pub use crate::dll::AzResultSvgSvgParseError as ResultSvgSvgParseError;

    impl std::fmt::Debug for ResultSvgSvgParseError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_result_svg_svg_parse_error_fmt_debug)(self)) } }
    impl Clone for ResultSvgSvgParseError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_result_svg_svg_parse_error_deep_copy)(self) } }
    impl Drop for ResultSvgSvgParseError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_result_svg_svg_parse_error_delete)(self); } }


    /// `ResultRefAnyBlockError` struct
    pub use crate::dll::AzResultRefAnyBlockError as ResultRefAnyBlockError;

    impl std::fmt::Debug for ResultRefAnyBlockError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_result_ref_any_block_error_fmt_debug)(self)) } }
    impl Clone for ResultRefAnyBlockError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_result_ref_any_block_error_deep_copy)(self) } }
    impl Drop for ResultRefAnyBlockError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_result_ref_any_block_error_delete)(self); } }
