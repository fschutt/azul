    #![allow(dead_code, unused_imports)]
    //! Struct definition for image / font / text IDs
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::vec::U8Vec;


    /// `TextId` struct
    pub use crate::dll::AzTextId as TextId;

    impl TextId {
        /// Creates a new, unique `TextId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_text_id_new)() }
    }

    impl std::fmt::Debug for TextId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_text_id_fmt_debug)(self)) } }
    impl Clone for TextId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_text_id_deep_copy)(self) } }
    impl Drop for TextId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_text_id_delete)(self); } }


    /// `ImageId` struct
    pub use crate::dll::AzImageId as ImageId;

    impl ImageId {
        /// Creates a new, unique `ImageId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_image_id_new)() }
    }

    impl std::fmt::Debug for ImageId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_image_id_fmt_debug)(self)) } }
    impl Clone for ImageId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_image_id_deep_copy)(self) } }
    impl Drop for ImageId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_image_id_delete)(self); } }


    /// `FontId` struct
    pub use crate::dll::AzFontId as FontId;

    impl FontId {
        /// Creates a new, unique `FontId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_font_id_new)() }
    }

    impl std::fmt::Debug for FontId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_font_id_fmt_debug)(self)) } }
    impl Clone for FontId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_font_id_deep_copy)(self) } }
    impl Drop for FontId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_font_id_delete)(self); } }


    /// `ImageSource` struct
    pub use crate::dll::AzImageSource as ImageSource;

    impl std::fmt::Debug for ImageSource { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_image_source_fmt_debug)(self)) } }
    impl Clone for ImageSource { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_image_source_deep_copy)(self) } }
    impl Drop for ImageSource { fn drop(&mut self) { (crate::dll::get_azul_dll().az_image_source_delete)(self); } }


    /// `FontSource` struct
    pub use crate::dll::AzFontSource as FontSource;

    impl std::fmt::Debug for FontSource { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_font_source_fmt_debug)(self)) } }
    impl Clone for FontSource { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_font_source_deep_copy)(self) } }
    impl Drop for FontSource { fn drop(&mut self) { (crate::dll::get_azul_dll().az_font_source_delete)(self); } }


    /// `RawImage` struct
    pub use crate::dll::AzRawImage as RawImage;

    impl RawImage {
        /// Creates a new `RawImage` by loading the decoded bytes
        pub fn new(decoded_pixels: U8Vec, width: usize, height: usize, data_format: RawImageFormat) -> Self { (crate::dll::get_azul_dll().az_raw_image_new)(decoded_pixels, width, height, data_format) }
    }

    impl std::fmt::Debug for RawImage { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_raw_image_fmt_debug)(self)) } }
    impl Clone for RawImage { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_raw_image_deep_copy)(self) } }
    impl Drop for RawImage { fn drop(&mut self) { (crate::dll::get_azul_dll().az_raw_image_delete)(self); } }


    /// `RawImageFormat` struct
    pub use crate::dll::AzRawImageFormat as RawImageFormat;

    impl std::fmt::Debug for RawImageFormat { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_raw_image_format_fmt_debug)(self)) } }
    impl Clone for RawImageFormat { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_raw_image_format_deep_copy)(self) } }
    impl Drop for RawImageFormat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_raw_image_format_delete)(self); } }
