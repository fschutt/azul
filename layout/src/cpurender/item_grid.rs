//! Which items of a list can reach a rect, without asking each of them.
//!
//! A walk steps over every item of a list and tests each against the clip it paints under
//! (`raster::outside_active_clip`). A view's list is a map of 17,000 items of which a damage
//! rect - a pan's strip, a label's box - reaches a few dozen, and a repaint in bands walks the
//! list once per band. The grid answers "which items can paint into this rect" from the items'
//! bounds, found once for the list.
