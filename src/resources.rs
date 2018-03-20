use webrender::api::{ImageKey, FontKey};
use FastHashMap;

/// Font and image keys
/// 
/// The idea is that azul doesn't know where the resources come from,
/// whether they are loaded from the network or a disk.
/// Fonts and images must be added and removed dynamically. If you have a 
/// fonts that should be always accessible, then simply add them before the app
/// starts up. 
///
/// Images and fonts can be references across window contexts 
/// (not yet tested, but should work).
#[derive(Debug, Default, Clone)]
pub(crate) struct AppResources {
    pub(crate) images: FastHashMap<usize, ImageKey>,
    pub(crate) fonts: FastHashMap<usize, FontKey>,
}

/// An `ImageId` is a wrapper around webrenders `ImageKey`. 
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(usize);

/// A Font ID is a wrapper around webrenders `FontKey`. 
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(usize);

