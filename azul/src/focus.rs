use azul_css::CssPath;
#[cfg(feature = "css-parser")]
use azul_css_parser::CssPathParseError;
use {
    window::CallbackInfo,
    traits::Layout,
    id_tree::NodeId,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FocusTarget {
    Id(NodeId),
    Path(CssPath),
    NoFocus,
}

impl<'a, T: 'a + Layout> CallbackInfo<'a, T> {

    /// Set the focus to a certain div by parsing a string.
    /// Note that the parsing of the string can fail, therefore the Result
    #[cfg(feature = "css-parser")]
    pub fn set_focus<'b>(&mut self, input: &'b str) -> Result<(), CssPathParseError<'b>> {
        use azul_css_parser::parse_css_path;
        let path = parse_css_path(input)?;
        self.focus = Some(FocusTarget::Path(path));
        Ok(())
    }

    /// Sets the focus by using an already-parsed `CssPath`.
    pub fn set_focus_by_path(&mut self, path: CssPath) {
        self.focus = Some(FocusTarget::Path(path))
    }

    /// Set the focus of the window to a specific div using a `NodeId`.
    ///
    /// Note that this ID will be dependent on the position in the DOM and therefore
    /// the next frames UI must be the exact same as the current one, otherwise
    /// the focus will be cleared or shifted (depending on apps setting).
    pub fn set_focus_by_node_id(&mut self, id: NodeId) {
        self.focus = Some(FocusTarget::Id(id));
    }

    /// Clears the focus for the next frame.
    pub fn clear_focus(&mut self) {
        self.focus = Some(FocusTarget::NoFocus);
    }
}