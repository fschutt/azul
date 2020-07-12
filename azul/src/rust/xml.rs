    #![allow(dead_code, unused_imports)]
    //! XML parsing + XML type definitions
    use crate::dll::*;
    use std::ffi::c_void;


    /// `XmlTextPos` struct
    pub use crate::dll::AzXmlTextPos as XmlTextPos;

    impl std::fmt::Debug for XmlTextPos { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_xml_text_pos_fmt_debug)(self)) } }
    impl Clone for XmlTextPos { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_xml_text_pos_deep_copy)(self) } }
    impl Drop for XmlTextPos { fn drop(&mut self) { (crate::dll::get_azul_dll().az_xml_text_pos_delete)(self); } }
