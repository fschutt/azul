//! The icon view's own keys stay the grid's while it has the keyboard: the window's shortcuts
//! leave them alone. F2 on a tile opens the grid's name field; the window's Rename running too
//! swapped the grid for the hand-built cells with a second field, and Escape then went to the
//! grid's field, which was gone, so the rename never ended.

use crate::{actions::grid_handles, keys::Command};

#[test]
fn f2_on_an_icon_tile_is_the_grids_alone() {
    assert!(grid_handles(Command::Rename), "the grid edits the focused tile's name itself");
}
