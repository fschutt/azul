//! Types and methods used to describe the style of an application
use style_properties::StyleProperty;

/// Wrapper for a `Vec<StyleRuleSet>` - the style is immutable at runtime, it can only be
/// created once. Animations / conditional styling is implemented using dynamic fields.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AppStyle {
    /// The style rules making up the document - for example, de-duplicated CSS rules
    pub rules: Vec<StyleRuleSet>,
}

impl std::convert::From<Vec<StyleRuleSet>> for AppStyle {
    fn from(rules: Vec<StyleRuleSet>) -> Self {
        Self { rules }
    }
}

/// Contains one parsed `key: value` pair, static or dynamic
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StyleDeclaration {
    /// Static key-value pair, such as `width: 500px`
    Static(StyleProperty),
    /// Dynamic key-value pair with default value, such as `width: [[ my_id | 500px ]]`
    Dynamic(DynamicStyleProperty),
}

impl StyleDeclaration {
    /// Determines if the property will be inherited (applied to the children)
    /// during the recursive application of the style on the DOM tree
    pub fn is_inheritable(&self) -> bool {
        use self::StyleDeclaration::*;
        match self {
            Static(s) => s.is_inheritable(),
            Dynamic(d) => d.is_inheritable(),
        }
    }
}

/// A `DynamicStyleProperty` is a type of style property that can be changed on possibly
/// every frame by the Rust code - for example to implement an `On::Hover` behaviour.
///
/// The syntax for such a property looks like this:
///
/// ```no_run,ignore
/// #my_div {
///    padding: [[ my_dynamic_property_id | 400px ]];
/// }
/// ```
///
/// Azul will register a dynamic property with the key "my_dynamic_property_id"
/// and the default value of 400px. If the property gets overridden during one frame,
/// the overridden property takes precedence.
///
/// At runtime the style is immutable (which is a performance optimization - if we
/// can assume that the property never changes at runtime), we can do some optimizations on it.
/// Dynamic style properties can also be used for animations and conditional styles
/// (i.e. `hover`, `focus`, etc.), thereby leading to cleaner code, since all of these
/// special cases now use one single API.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DynamicStyleProperty {
    /// The stringified ID of this property, i.e. the `"my_id"` in `width: [[ my_id | 500px ]]`.
    pub dynamic_id: String,
    /// Default value, used if the style property isn't overridden in this frame
    /// i.e. the `500px` in `width: [[ my_id | 500px ]]`.
    pub default: DynamicStylePropertyDefault,
}

/// If this value is set to default, the style property will not exist if it isn't overriden.
/// An example where this is useful is when you want to say something like this:
///
/// `width: [[ 400px | auto ]];`
///
/// "If I set this property to width: 400px, then use exactly 400px. Otherwise use whatever the default width is."
/// If this property wouldn't exist, you could only set the default to "0px" or something like
/// that, meaning that if you don't override the property, then you'd set it to 0px - which is
/// different from `auto`, since `auto` has its width determined by how much space there is
/// available in the parent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DynamicStylePropertyDefault  {
    Exact(StyleProperty),
    Auto,
}

impl DynamicStyleProperty {
    pub fn is_inheritable(&self) -> bool {
        // Dynamic style properties should not be inheritable,
        // since that could lead to bugs - you set a property in Rust, suddenly
        // the wrong UI component starts to react because it was inherited.
        false
    }
}

/// One block of rules that applies a bunch of rules to an "xpath" in the style, i.e.
/// `div#myid.myclass -> { ("justify-content", "center") }`
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRuleSet {
    /// The xpath (full selector) of the style ruleset
    pub path: XPath,
    /// `"justify-content: center"` =>
    /// `StyleDeclaration::Static(StyleProperty::JustifyContent(LayoutJustifyContent::Center))`
    pub declarations: Vec<StyleDeclaration>,
}

pub type CssContentGroup<'a> = Vec<&'a XPathSelector>;

/// Signifies the type (i.e. the discriminant value) of a DOM node without any of its associated
/// data
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

/// Represents a full style xpath:
/// `#div > .my_class:focus` =>
/// `[XPathSelector::Type(NodeTypePath::Div), LimitChildren, XPathSelector::Class("my_class"), XPathSelector::PseudoSelector]`
#[derive(Debug, Clone, Hash, Default, PartialEq)]
pub struct XPath {
    pub selectors: Vec<XPathSelector>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum XPathSelector {
    /// Represents the `*` selector
    Global,
    /// `div`, `p`, etc.
    Type(NodeTypePath),
    /// `.something`
    Class(String),
    /// `#something`
    Id(String),
    /// `:something`
    PseudoSelector(XPathPseudoSelector),
    /// Represents the `>` selector
    DirectChildren,
    /// Represents the ` ` selector
    Children
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum XPathPseudoSelector {
    /// `:first`
    First,
    /// `:last`
    Last,
    /// `:nth-child`
    NthChild(usize),
    /// `:hover` - mouse is over element
    Hover,
    /// `:active` - mouse is pressed and over element
    Active,
    /// `:focus` - element has received focus
    Focus,
}

impl AppStyle {
    /// Creates a new AppStyle with no style rules.
    pub fn new() -> Self {
        Default::default()
    }

    /// Combines two parsed stylesheets into one, appending the rules of
    /// `other` after the rules of `self`.
    pub fn merge(&mut self, mut other: Self) {
        self.rules.append(&mut other.rules);
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct StyleConstraintList {
    pub list: Vec<StyleDeclaration>
}
