    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Result<*>` wrappers
    use crate::dll::*;
    use std::ffi::c_void;


    /// `ResultSvgSvgParseError` struct
    #[doc(inline)] pub use crate::dll::AzResultSvgSvgParseError as ResultSvgSvgParseError;

    impl Clone for ResultSvgSvgParseError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_result_svg_svg_parse_error_deep_copy)(self) } }
    impl Drop for ResultSvgSvgParseError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_result_svg_svg_parse_error_delete)(self); } }
