    #![allow(dead_code, unused_imports)]
    //! Definition of error types
    use crate::dll::*;
    use std::ffi::c_void;


    /// `SvgParseError` struct
    pub use crate::dll::AzSvgParseError as SvgParseError;

    impl std::fmt::Debug for SvgParseError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_parse_error_fmt_debug)(self)) } }
    impl Clone for SvgParseError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_parse_error_deep_copy)(self) } }
    impl Drop for SvgParseError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_parse_error_delete)(self); } }


    /// `XmlError` struct
    pub use crate::dll::AzXmlError as XmlError;

    impl std::fmt::Debug for XmlError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_xml_error_fmt_debug)(self)) } }
    impl Clone for XmlError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_xml_error_deep_copy)(self) } }
    impl Drop for XmlError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_xml_error_delete)(self); } }


    /// `DuplicatedNamespaceError` struct
    pub use crate::dll::AzDuplicatedNamespaceError as DuplicatedNamespaceError;

    impl std::fmt::Debug for DuplicatedNamespaceError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_duplicated_namespace_error_fmt_debug)(self)) } }
    impl Clone for DuplicatedNamespaceError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_duplicated_namespace_error_deep_copy)(self) } }
    impl Drop for DuplicatedNamespaceError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_duplicated_namespace_error_delete)(self); } }


    /// `UnknownNamespaceError` struct
    pub use crate::dll::AzUnknownNamespaceError as UnknownNamespaceError;

    impl std::fmt::Debug for UnknownNamespaceError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_unknown_namespace_error_fmt_debug)(self)) } }
    impl Clone for UnknownNamespaceError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_unknown_namespace_error_deep_copy)(self) } }
    impl Drop for UnknownNamespaceError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_unknown_namespace_error_delete)(self); } }


    /// `UnexpectedCloseTagError` struct
    pub use crate::dll::AzUnexpectedCloseTagError as UnexpectedCloseTagError;

    impl std::fmt::Debug for UnexpectedCloseTagError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_unexpected_close_tag_error_fmt_debug)(self)) } }
    impl Clone for UnexpectedCloseTagError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_unexpected_close_tag_error_deep_copy)(self) } }
    impl Drop for UnexpectedCloseTagError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_unexpected_close_tag_error_delete)(self); } }


    /// `UnknownEntityReferenceError` struct
    pub use crate::dll::AzUnknownEntityReferenceError as UnknownEntityReferenceError;

    impl std::fmt::Debug for UnknownEntityReferenceError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_unknown_entity_reference_error_fmt_debug)(self)) } }
    impl Clone for UnknownEntityReferenceError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_unknown_entity_reference_error_deep_copy)(self) } }
    impl Drop for UnknownEntityReferenceError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_unknown_entity_reference_error_delete)(self); } }


    /// `DuplicatedAttributeError` struct
    pub use crate::dll::AzDuplicatedAttributeError as DuplicatedAttributeError;

    impl std::fmt::Debug for DuplicatedAttributeError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_duplicated_attribute_error_fmt_debug)(self)) } }
    impl Clone for DuplicatedAttributeError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_duplicated_attribute_error_deep_copy)(self) } }
    impl Drop for DuplicatedAttributeError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_duplicated_attribute_error_delete)(self); } }


    /// `XmlParseError` struct
    pub use crate::dll::AzXmlParseError as XmlParseError;

    impl std::fmt::Debug for XmlParseError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_xml_parse_error_fmt_debug)(self)) } }
    impl Clone for XmlParseError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_xml_parse_error_deep_copy)(self) } }
    impl Drop for XmlParseError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_xml_parse_error_delete)(self); } }


    /// `XmlTextError` struct
    pub use crate::dll::AzXmlTextError as XmlTextError;

    impl std::fmt::Debug for XmlTextError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_xml_text_error_fmt_debug)(self)) } }
    impl Clone for XmlTextError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_xml_text_error_deep_copy)(self) } }
    impl Drop for XmlTextError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_xml_text_error_delete)(self); } }


    /// `XmlStreamError` struct
    pub use crate::dll::AzXmlStreamError as XmlStreamError;

    impl std::fmt::Debug for XmlStreamError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_xml_stream_error_fmt_debug)(self)) } }
    impl Clone for XmlStreamError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_xml_stream_error_deep_copy)(self) } }
    impl Drop for XmlStreamError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_xml_stream_error_delete)(self); } }


    /// `NonXmlCharError` struct
    pub use crate::dll::AzNonXmlCharError as NonXmlCharError;

    impl std::fmt::Debug for NonXmlCharError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_non_xml_char_error_fmt_debug)(self)) } }
    impl Clone for NonXmlCharError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_non_xml_char_error_deep_copy)(self) } }
    impl Drop for NonXmlCharError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_non_xml_char_error_delete)(self); } }


    /// `InvalidCharError` struct
    pub use crate::dll::AzInvalidCharError as InvalidCharError;

    impl std::fmt::Debug for InvalidCharError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_invalid_char_error_fmt_debug)(self)) } }
    impl Clone for InvalidCharError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_invalid_char_error_deep_copy)(self) } }
    impl Drop for InvalidCharError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_invalid_char_error_delete)(self); } }


    /// `InvalidCharMultipleError` struct
    pub use crate::dll::AzInvalidCharMultipleError as InvalidCharMultipleError;

    impl std::fmt::Debug for InvalidCharMultipleError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_invalid_char_multiple_error_fmt_debug)(self)) } }
    impl Clone for InvalidCharMultipleError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_invalid_char_multiple_error_deep_copy)(self) } }
    impl Drop for InvalidCharMultipleError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_invalid_char_multiple_error_delete)(self); } }


    /// `InvalidQuoteError` struct
    pub use crate::dll::AzInvalidQuoteError as InvalidQuoteError;

    impl std::fmt::Debug for InvalidQuoteError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_invalid_quote_error_fmt_debug)(self)) } }
    impl Clone for InvalidQuoteError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_invalid_quote_error_deep_copy)(self) } }
    impl Drop for InvalidQuoteError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_invalid_quote_error_delete)(self); } }


    /// `InvalidSpaceError` struct
    pub use crate::dll::AzInvalidSpaceError as InvalidSpaceError;

    impl std::fmt::Debug for InvalidSpaceError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_invalid_space_error_fmt_debug)(self)) } }
    impl Clone for InvalidSpaceError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_invalid_space_error_deep_copy)(self) } }
    impl Drop for InvalidSpaceError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_invalid_space_error_delete)(self); } }


    /// `InvalidStringError` struct
    pub use crate::dll::AzInvalidStringError as InvalidStringError;

    impl std::fmt::Debug for InvalidStringError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_invalid_string_error_fmt_debug)(self)) } }
    impl Clone for InvalidStringError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_invalid_string_error_deep_copy)(self) } }
    impl Drop for InvalidStringError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_invalid_string_error_delete)(self); } }
