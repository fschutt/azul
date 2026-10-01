//! Layers and the layer tree.
//!
//! A document's layers are a list, bottom first; a group holds its own list.
//! Every layer has an id that stays the same for its life (moves, undo).

use super::{adjust::Adjustment, blend::BlendMode, tile::TileGrid};

/// A layer's id: unique within its document, never reused.
pub type LayerId = u64;

/// What a layer is.
#[derive(Clone, Debug)]
pub enum LayerContent {
    /// Pixels.
    Raster(TileGrid),
    /// A colour map over everything below it in its group.
    Adjustment(Adjustment),
    /// Layers composited on their own, then blended as one (bottom first).
    Group(Vec<Layer>),
}

/// One row of the Layers panel.
#[derive(Clone, Debug)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub visible: bool,
    /// A locked layer refuses edits of its pixels and its position.
    pub locked: bool,
    /// 0..1.
    pub opacity: f32,
    pub blend: BlendMode,
    pub content: LayerContent,
    /// A group shows its children in the panel.
    pub expanded: bool,
}

impl Layer {
    fn with(id: LayerId, name: String, content: LayerContent) -> Self {
        Self {
            id,
            name,
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: BlendMode::Normal,
            content,
            expanded: true,
        }
    }

    #[must_use]
    pub fn raster(id: LayerId, name: impl Into<String>, grid: TileGrid) -> Self {
        Self::with(id, name.into(), LayerContent::Raster(grid))
    }

    #[must_use]
    pub fn adjustment(id: LayerId, adjustment: Adjustment) -> Self {
        let name = adjustment.name().to_string();
        Self::with(id, name, LayerContent::Adjustment(adjustment))
    }

    #[must_use]
    pub fn group(id: LayerId, name: impl Into<String>, children: Vec<Layer>) -> Self {
        Self::with(id, name.into(), LayerContent::Group(children))
    }

    /// "Layer", "Adjustment" or "Group".
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self.content {
            LayerContent::Raster(_) => "Layer",
            LayerContent::Adjustment(_) => "Adjustment",
            LayerContent::Group(_) => "Group",
        }
    }

    #[must_use]
    pub const fn grid(&self) -> Option<&TileGrid> {
        match &self.content {
            LayerContent::Raster(g) => Some(g),
            _ => None,
        }
    }

    pub fn grid_mut(&mut self) -> Option<&mut TileGrid> {
        match &mut self.content {
            LayerContent::Raster(g) => Some(g),
            _ => None,
        }
    }

    #[must_use]
    pub const fn children(&self) -> Option<&Vec<Layer>> {
        match &self.content {
            LayerContent::Group(c) => Some(c),
            _ => None,
        }
    }

    /// Every id in this layer's subtree, itself first.
    #[must_use]
    pub fn ids(&self) -> Vec<LayerId> {
        let mut out = vec![self.id];
        if let LayerContent::Group(children) = &self.content {
            for c in children {
                out.extend(c.ids());
            }
        }
        out
    }
}

/// Where a moved layer goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Directly above `id`, in `id`'s list.
    Above(LayerId),
    /// Directly below `id`, in `id`'s list.
    Below(LayerId),
    /// On top of the group `id`'s children.
    IntoGroup(LayerId),
}

/// The layer `id` anywhere in the tree.
#[must_use]
pub fn find(layers: &[Layer], id: LayerId) -> Option<&Layer> {
    for l in layers {
        if l.id == id {
            return Some(l);
        }
        if let LayerContent::Group(children) = &l.content {
            if let Some(found) = find(children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// The layer `id` anywhere in the tree, for writing.
pub fn find_mut(layers: &mut [Layer], id: LayerId) -> Option<&mut Layer> {
    for l in layers.iter_mut() {
        if l.id == id {
            return Some(l);
        }
        if let LayerContent::Group(children) = &mut l.content {
            if let Some(found) = find_mut(children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// The list that holds `id` and its index there.
pub fn parent_list_mut(layers: &mut Vec<Layer>, id: LayerId) -> Option<(&mut Vec<Layer>, usize)> {
    if let Some(i) = layers.iter().position(|l| l.id == id) {
        return Some((layers, i));
    }
    for l in layers.iter_mut() {
        if let LayerContent::Group(children) = &mut l.content {
            if let Some(found) = parent_list_mut(children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// The list that holds `id` and its index there.
#[must_use]
pub fn parent_list(layers: &[Layer], id: LayerId) -> Option<(&[Layer], usize)> {
    if let Some(i) = layers.iter().position(|l| l.id == id) {
        return Some((layers, i));
    }
    for l in layers {
        if let LayerContent::Group(children) = &l.content {
            if let Some(found) = parent_list(children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// Take `id` (with its subtree) out of the tree.
pub fn remove(layers: &mut Vec<Layer>, id: LayerId) -> Option<Layer> {
    let (list, i) = parent_list_mut(layers, id)?;
    Some(list.remove(i))
}

/// Put `layer` directly above `anchor` (in `anchor`'s list), or on top of
/// the document when `anchor` is `None` or gone.
pub fn insert_above(layers: &mut Vec<Layer>, anchor: Option<LayerId>, layer: Layer) {
    match anchor.and_then(|a| parent_list_mut(layers, a)) {
        Some((list, i)) => list.insert(i + 1, layer),
        None => layers.push(layer),
    }
}

/// Move `id` to `to`. Refused (false) when `id` would go into its own subtree
/// or a target is missing.
pub fn move_layer(layers: &mut Vec<Layer>, id: LayerId, to: Placement) -> bool {
    let target = match to {
        Placement::Above(t) | Placement::Below(t) | Placement::IntoGroup(t) => t,
    };
    if target == id {
        return false;
    }
    let Some(moving) = find(layers, id) else {
        return false;
    };
    if moving.ids().contains(&target) {
        return false;
    }
    if find(layers, target).is_none() {
        return false;
    }
    if let Placement::IntoGroup(g) = to {
        if find(layers, g).and_then(Layer::children).is_none() {
            return false;
        }
    }
    let Some(layer) = remove(layers, id) else {
        return false;
    };
    match to {
        Placement::Above(t) => match parent_list_mut(layers, t) {
            Some((list, i)) => list.insert(i + 1, layer),
            None => layers.push(layer),
        },
        Placement::Below(t) => match parent_list_mut(layers, t) {
            Some((list, i)) => list.insert(i, layer),
            None => layers.insert(0, layer),
        },
        Placement::IntoGroup(g) => match find_mut(layers, g) {
            Some(Layer {
                content: LayerContent::Group(children),
                ..
            }) => children.push(layer),
            _ => layers.push(layer),
        },
    }
    true
}

/// The layer directly below `id` in its list, if any.
#[must_use]
pub fn below(layers: &[Layer], id: LayerId) -> Option<LayerId> {
    let (list, i) = parent_list(layers, id)?;
    (i > 0).then(|| list[i - 1].id)
}

/// The tree as the Layers panel lists it: top first, each row with its depth
/// (children of an expanded group follow it, indented).
#[must_use]
pub fn rows(layers: &[Layer]) -> Vec<(usize, LayerId)> {
    fn walk(list: &[Layer], depth: usize, out: &mut Vec<(usize, LayerId)>) {
        for l in list.iter().rev() {
            out.push((depth, l.id));
            if let LayerContent::Group(children) = &l.content {
                if l.expanded {
                    walk(children, depth + 1, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(layers, 0, &mut out);
    out
}

/// Every layer id in the tree.
#[must_use]
pub fn all_ids(layers: &[Layer]) -> Vec<LayerId> {
    layers.iter().flat_map(Layer::ids).collect()
}
