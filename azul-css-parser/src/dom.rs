use azul_style::NodeTypePath;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NodeTypePathParseError<'a> {
    Invalid(&'a str),
}

impl_display!{ NodeTypePathParseError<'a>, {
    Invalid(e) => format!("Invalid node type: {}", e),
}}

/// Parses the node type from a CSS string such as `"div"` => `NodeTypePath::Div`
pub fn node_type_path_from_str(data: &str) -> Result<NodeTypePath, NodeTypePathParseError> {
    use azul_style::NodeTypePath::*;
    match data {
        "div" => Ok(Div),
        "p" => Ok(P),
        "img" => Ok(Img),
        "texture" => Ok(Texture),
        "iframe" => Ok(IFrame),
        other => Err(NodeTypePathParseError::Invalid(other)),
    }
}
