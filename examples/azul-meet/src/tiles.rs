//! What the call view shows: which tile is on the stage, which are in the filmstrip or the
//! gallery, and which tiles ask for a stream at all.
//!
//! A shared screen takes the stage in any view (true resolution matters for screen text); in the
//! speaker view the pinned participant, else the active speaker, else the first other one takes
//! it; the gallery has no stage. This side's own screen share is never its own stage (it shows
//! its preview as a tile). A tile hidden from view (scrolled out of the tiles pane, the window
//! minimized) or drawn smaller than [`MIN_TILE_PX`] asks for no stream: "cull what nobody
//! displays" (iroh-routes). Pure: no azul types, unit-tested here.

/// The smallest tile, in device pixels of height, that asks for a stream; smaller ones show a
/// name, not video.
pub const MIN_TILE_PX: f32 = 24.0;

/// Which of a participant's two pictures a tile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TileKind {
    Camera,
    Screen,
}

/// One tile: a participant (by peer key) and which picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    pub key: u64,
    pub kind: TileKind,
}

/// A participant as the view sees it: whether a screen share of theirs is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Participant {
    pub key: u64,
    pub sharing: bool,
}

/// How the call shows its participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Every camera in an equal gallery tile.
    Gallery,
    /// One participant large on the stage, the others in the filmstrip.
    Speaker,
}

/// The tiles of the call view: the stage (if any) and the rest, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Arrangement {
    pub stage: Option<Tile>,
    pub tiles: Vec<Tile>,
}

/// The arrangement of `others` (the remote participants, in the order they joined) and this side
/// (`me`, sharing its screen when `my_screen`): see the module docs.
pub fn arrange(
    me: u64,
    my_screen: bool,
    others: &[Participant],
    view: View,
    pinned: Option<u64>,
    active: Option<u64>,
) -> Arrangement {
    let present = |key: &u64| others.iter().any(|p| p.key == *key);
    let camera = |key: u64| Tile {
        key,
        kind: TileKind::Camera,
    };
    let screen = |key: u64| Tile {
        key,
        kind: TileKind::Screen,
    };
    let stage = match others.iter().find(|p| p.sharing) {
        Some(sharer) => Some(screen(sharer.key)),
        None if view == View::Speaker => pinned
            .filter(present)
            .or_else(|| active.filter(present))
            .or_else(|| others.first().map(|p| p.key))
            .map(camera),
        None => None,
    };
    let mut tiles = Vec::with_capacity(others.len() * 2 + 2);
    for p in others {
        tiles.push(camera(p.key));
        if p.sharing {
            tiles.push(screen(p.key));
        }
    }
    tiles.push(camera(me));
    if my_screen {
        tiles.push(screen(me));
    }
    tiles.retain(|tile| Some(*tile) != stage);
    Arrangement { stage, tiles }
}

/// The height a tile asks its stream for, in logical pixels: its laid-out height (else its box's
/// height until it is laid out), or 0 - no stream - when it is not visible or drawn smaller than
/// [`MIN_TILE_PX`] device pixels at `scale`.
pub fn tile_need(visible: bool, laid_out: Option<f32>, box_height: f32, scale: f32) -> f32 {
    if !visible {
        return 0.0;
    }
    let height = laid_out.unwrap_or(box_height);
    // `!(>=)` also turns a NaN height into no stream.
    if !(height * scale.max(1.0) >= MIN_TILE_PX) {
        return 0.0;
    }
    height
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: u64 = 1;
    const ADA: u64 = 2;
    const BEN: u64 = 3;

    fn cam(key: u64) -> Tile {
        Tile {
            key,
            kind: TileKind::Camera,
        }
    }

    fn screen(key: u64) -> Tile {
        Tile {
            key,
            kind: TileKind::Screen,
        }
    }

    fn quiet(key: u64) -> Participant {
        Participant {
            key,
            sharing: false,
        }
    }

    #[test]
    fn the_gallery_shows_every_camera_and_this_side_last_without_a_stage() {
        let a = arrange(ME, false, &[quiet(ADA), quiet(BEN)], View::Gallery, None, Some(BEN));
        assert_eq!(a.stage, None);
        assert_eq!(a.tiles, vec![cam(ADA), cam(BEN), cam(ME)]);
    }

    #[test]
    fn a_shared_screen_takes_the_stage_in_any_view() {
        let others = [quiet(ADA), Participant { key: BEN, sharing: true }];
        for view in [View::Gallery, View::Speaker] {
            let a = arrange(ME, false, &others, view, Some(ADA), Some(ADA));
            assert_eq!(a.stage, Some(screen(BEN)), "{view:?}");
            assert_eq!(a.tiles, vec![cam(ADA), cam(BEN), cam(ME)], "{view:?}");
        }
    }

    #[test]
    fn this_sides_own_share_is_a_tile_never_its_stage() {
        let a = arrange(ME, true, &[quiet(ADA)], View::Gallery, None, None);
        assert_eq!(a.stage, None);
        assert_eq!(a.tiles, vec![cam(ADA), cam(ME), screen(ME)]);
    }

    #[test]
    fn the_speaker_view_stages_the_pinned_else_the_active_else_the_first() {
        let others = [quiet(ADA), quiet(BEN)];
        let pinned = arrange(ME, false, &others, View::Speaker, Some(BEN), Some(ADA));
        assert_eq!(pinned.stage, Some(cam(BEN)));
        assert_eq!(pinned.tiles, vec![cam(ADA), cam(ME)], "the stage is not in the filmstrip");
        let active = arrange(ME, false, &others, View::Speaker, None, Some(BEN));
        assert_eq!(active.stage, Some(cam(BEN)));
        let first = arrange(ME, false, &others, View::Speaker, None, None);
        assert_eq!(first.stage, Some(cam(ADA)));
        // A pinned or active key that is not (or no longer) in the call is passed over.
        let gone = arrange(ME, false, &others, View::Speaker, Some(99), Some(98));
        assert_eq!(gone.stage, Some(cam(ADA)));
        // Alone in speaker view: this side's own camera is the only tile, no stage.
        let alone = arrange(ME, false, &[], View::Speaker, None, None);
        assert_eq!((alone.stage, alone.tiles), (None, vec![cam(ME)]));
    }

    #[test]
    fn a_hidden_or_tiny_tile_asks_for_no_stream() {
        assert_eq!(tile_need(true, Some(200.0), 100.0, 1.0), 200.0);
        assert_eq!(tile_need(true, None, 100.0, 2.0), 100.0, "the box until laid out");
        assert_eq!(tile_need(false, Some(200.0), 100.0, 1.0), 0.0, "scrolled out of view");
        assert_eq!(tile_need(true, Some(20.0), 100.0, 1.0), 0.0, "20 device pixels");
        assert_eq!(tile_need(true, Some(20.0), 100.0, 2.0), 20.0, "40 device pixels");
        assert_eq!(tile_need(true, Some(0.0), 100.0, 1.0), 0.0, "collapsed");
    }
}
