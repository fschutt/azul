/// Like the node type, but only signifies the type (i.e. the discriminant value)
/// of the `NodeType`, without the actual data
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NodeTypePath {
    Div,
    P,
    Img,
    Texture,
    IFrame,
}

impl std::fmt::Display for NodeTypePath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        use self::NodeTypePath::*;
        let path = match self {
            Div => "div",
            P => "p",
            Img => "img",
            Texture => "texture",
            IFrame => "iframe",
        };
        write!(f, "{}", path)?;
        Ok(())
    }
}
