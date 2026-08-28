//! Pointer coordinate spaces, as distinct types.
//!
//! # Why this module exists
//!
//! Five different "cursor position" conventions coexist in this codebase and,
//! until this module, every one of them was a bare
//! [`LogicalPosition`](crate::geom::LogicalPosition). Nothing stopped a value
//! from one convention being handed to a consumer expecting another, and the
//! resulting bugs are all invisible in the default widget set (which happens to
//! use unpadded, unbordered, unscrolled text boxes) and appear the moment a
//! real app pads an editable or scrolls a field.
//!
//! # The five spaces
//!
//! Let, for one node in one DOM:
//!
//! * `P` = the node's STATIC border-box origin — what `calculated_positions`
//!   stores. "Static" means *before* any scroll offset is applied.
//! * `A` = the sum of every scrolling ANCESTOR's current offset.
//! * `S` = the node's OWN current scroll offset.
//! * `E` = the node's content inset, `padding-left + border-left` /
//!   `padding-top + border-top` (see [`ContentInset`]).
//!
//! The raster paints a glyph whose inline-layout position is `g` at window
//! position `P + E + g − S − A`. Inverting that one equation names every space:
//!
//! | # | Type | Value | Who produces it |
//! |---|------|-------|-----------------|
//! | 1 | [`WindowPoint`] | `w` | the platform cursor event |
//! | 2 | [`StaticLayoutPoint`] | `w + A` | `CpuHitTester::hit_test_scrolled`, `headless::resolve_chain` |
//! | 3 | [`BorderBoxLocal`] | `w + A − P` | `WebRender`'s `point_relative_to_item` |
//! | 4 | [`ContentBoxLocal`] | `w + A − P − E` | [`BorderBoxLocal::to_content_box_local`] |
//! | 5 | [`ScrolledContentPoint`] | `w + A − P − E + S` | [`ContentBoxLocal::scrolled_by`] — the ONLY space `UnifiedLayout::hittest_cursor` accepts |
//!
//! The historical sixth convention — "static layout space with the node's own
//! scroll added back too" (`w + A + S`, what the self-inclusive scroll walkers
//! produced) — is deliberately **not** a type here. It is the mixed space that
//! caused the bugs: own scroll belongs to the node's *content*, so it may only
//! be added once the point is already node-local AND content-box-relative.
//! With this vocabulary that combination is unreachable: [`scrolled_by`] exists
//! only on [`ContentBoxLocal`].
//!
//! [`scrolled_by`]: ContentBoxLocal::scrolled_by
//!
//! # Cost
//!
//! Every type here is `#[repr(transparent)]` over `LogicalPosition` and every
//! conversion is a `const fn` doing at most two `f32` adds, so the vocabulary
//! is free at runtime and ABI-identical to the bare position it replaces.

use crate::geom::LogicalPosition;

/// A scroll offset, i.e. how far a scroll container's content has been moved
/// UP/LEFT relative to its scrollport.
///
/// Distinct from a position so that "add the scroll" and "add a position"
/// cannot be confused, and so the two very different sums — ancestors-only vs
/// self-and-ancestors — are at least visible at the call site.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ScrollOffset(pub LogicalPosition);

impl ScrollOffset {
    /// The zero offset (nothing scrolled).
    #[inline]
    #[must_use]
    pub const fn zero() -> Self {
        Self(LogicalPosition::zero())
    }

    /// Build an offset from raw components.
    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self(LogicalPosition { x, y })
    }

    /// The raw offset.
    #[inline]
    #[must_use]
    pub const fn get(self) -> LogicalPosition {
        self.0
    }

    /// Accumulate another container's offset into this one.
    #[inline]
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self(LogicalPosition {
            x: self.0.x + other.0.x,
            y: self.0.y + other.0.y,
        })
    }
}

/// The left/top inset from a node's BORDER box to its CONTENT box:
/// `padding-left + border-left-width` and `padding-top + border-top-width`.
///
/// This is the `E` term in the module docs. It is the difference between the
/// box layout positions the node (border box, `calculated_positions`) and the
/// box inline text is laid out in (content box) — which is exactly the term
/// that used to be silently missing on one of the two hit-test hosts.
#[derive(Debug, Copy, Clone, Default, PartialEq)]
#[repr(C)]
pub struct ContentInset {
    /// `padding-left + border-left-width`
    pub left: f32,
    /// `padding-top + border-top-width`
    pub top: f32,
}

impl ContentInset {
    /// No padding and no border — the content box IS the border box.
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
    };

    /// Build an inset from the already-summed left/top edges.
    #[inline]
    #[must_use]
    pub const fn new(left: f32, top: f32) -> Self {
        Self { left, top }
    }
}

/// Declare a `#[repr(transparent)]` point newtype with the shared boilerplate.
macro_rules! point_space {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        // The full set `LogicalPosition` itself carries (its Eq/Ord/Hash are
        // quantized, so they agree with each other), so a typed point can go
        // wherever an untyped one used to — including as a BTreeMap value in a
        // derived-Ord struct.
        #[derive(Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        pub struct $name(LogicalPosition);

        impl $name {
            /// Assert that `p` is already in this space.
            ///
            /// Only correct at a PRODUCER boundary — the place that computed
            /// the point and therefore knows which space it is in. Everywhere
            /// else, use one of the named conversions instead; that is the
            /// entire point of this module.
            #[inline]
            #[must_use]
            pub const fn new(p: LogicalPosition) -> Self {
                Self(p)
            }

            /// The origin of this space.
            #[inline]
            #[must_use]
            pub const fn zero() -> Self {
                Self(LogicalPosition::zero())
            }

            /// Drop back to an untyped position.
            ///
            /// Only correct at a CONSUMER boundary that documents which space
            /// it wants.
            #[inline]
            #[must_use]
            pub const fn get(self) -> LogicalPosition {
                self.0
            }

            /// The x component, in this space.
            #[inline]
            #[must_use]
            pub const fn x(self) -> f32 {
                self.0.x
            }

            /// The y component, in this space.
            #[inline]
            #[must_use]
            pub const fn y(self) -> f32 {
                self.0.y
            }
        }
    };
}

point_space! {
    /// **Space 1** — a raw pointer position in window coordinates, exactly as
    /// the platform delivered it. Nothing has been unwound.
    WindowPoint
}

point_space! {
    /// **Space 2** — a window point mapped into a DOM's STATIC layout
    /// coordinate system: every scrolling ANCESTOR's offset added back (and
    /// any ancestor transform inverted).
    ///
    /// This is the space `calculated_positions` lives in, so a
    /// `StaticLayoutPoint` may be compared against a node's static rect. It is
    /// what `CpuHitTester::hit_test_scrolled` returns and what
    /// `headless::resolve_chain`'s `map_screen_to_local` produces.
    ///
    /// It does NOT include the node's own scroll offset: a container's own
    /// scrolling moves its CONTENT, never its box.
    StaticLayoutPoint
}

point_space! {
    /// **Space 3** — relative to a node's static BORDER-box origin, own scroll
    /// NOT applied.
    ///
    /// This is what `WebRender` reports as `point_relative_to_item`: azul
    /// pushes a scroll container's hit rect BEFORE its scroll frame, so the
    /// point WR subtracts the rect from is in the parent's (unscrolled) space.
    ///
    /// It is also the space the public `CallbackInfo::get_cursor_relative_to_node`
    /// promises, which is why widgets that divide by the node's border-box
    /// width (sliders, split panes, colour wheels, map panning) are correct
    /// against it.
    BorderBoxLocal
}

point_space! {
    /// **Space 4** — relative to a node's static CONTENT-box origin, own
    /// scroll NOT applied.
    ///
    /// Padding and border have been removed ([`ContentInset`]), so this is the
    /// space inline text is laid out in — but only for an UNSCROLLED box.
    ContentBoxLocal
}

point_space! {
    /// **Space 5** — content-box-local WITH the node's own scroll added back:
    /// a point in the node's scrollable CONTENT.
    ///
    /// This is the only space `UnifiedLayout::hittest_cursor` accepts, because
    /// the inline layout is built once, unscrolled, and the scroll frame moves
    /// it at paint time. Producing it requires all four of: ancestor scroll,
    /// the node's static origin, its content inset, and its own scroll — and
    /// the conversion chain in this module is the only way to have supplied
    /// all four.
    ScrolledContentPoint
}

impl WindowPoint {
    /// Map into the DOM's static layout space by adding back the accumulated
    /// scroll of the node's ANCESTORS (`w → w + A`).
    ///
    /// Pass an ancestors-only sum ([`Inclusivity::AncestorsOnly`]). Passing a
    /// self-inclusive sum here is the classic double-count.
    #[inline]
    #[must_use]
    pub const fn to_static_layout(self, ancestor_scroll: ScrollOffset) -> StaticLayoutPoint {
        StaticLayoutPoint(LogicalPosition {
            x: self.0.x + ancestor_scroll.0.x,
            y: self.0.y + ancestor_scroll.0.y,
        })
    }
}

impl StaticLayoutPoint {
    /// Back to window space (`w + A → w`).
    #[inline]
    #[must_use]
    pub const fn to_window(self, ancestor_scroll: ScrollOffset) -> WindowPoint {
        WindowPoint(LogicalPosition {
            x: self.0.x - ancestor_scroll.0.x,
            y: self.0.y - ancestor_scroll.0.y,
        })
    }

    /// Make the point node-local by subtracting the node's STATIC border-box
    /// origin (`w + A → w + A − P`).
    #[inline]
    #[must_use]
    pub const fn to_border_box_local(self, border_box_origin: LogicalPosition) -> BorderBoxLocal {
        BorderBoxLocal(LogicalPosition {
            x: self.0.x - border_box_origin.x,
            y: self.0.y - border_box_origin.y,
        })
    }
}

impl BorderBoxLocal {
    /// Back to the DOM's static layout space (`w + A − P → w + A`).
    #[inline]
    #[must_use]
    pub const fn to_static_layout(self, border_box_origin: LogicalPosition) -> StaticLayoutPoint {
        StaticLayoutPoint(LogicalPosition {
            x: self.0.x + border_box_origin.x,
            y: self.0.y + border_box_origin.y,
        })
    }

    /// Step in from the border box to the content box (`… − P → … − P − E`).
    #[inline]
    #[must_use]
    pub const fn to_content_box_local(self, inset: ContentInset) -> ContentBoxLocal {
        ContentBoxLocal(LogicalPosition {
            x: self.0.x - inset.left,
            y: self.0.y - inset.top,
        })
    }
}

impl ContentBoxLocal {
    /// Step back out to the border box (`… − P − E → … − P`).
    #[inline]
    #[must_use]
    pub const fn to_border_box_local(self, inset: ContentInset) -> BorderBoxLocal {
        BorderBoxLocal(LogicalPosition {
            x: self.0.x + inset.left,
            y: self.0.y + inset.top,
        })
    }

    /// Add back the node's OWN scroll offset to reach the point in its
    /// scrollable content (`… − P − E → … − P − E + S`).
    ///
    /// Pass the node's own offset only. This is the step both hit-test hosts
    /// used to skip, which is why clicking in a horizontally scrolled text
    /// field placed the caret `scroll_x` px to the left of the pointer.
    #[inline]
    #[must_use]
    pub const fn scrolled_by(self, own_scroll: ScrollOffset) -> ScrolledContentPoint {
        ScrolledContentPoint(LogicalPosition {
            x: self.0.x + own_scroll.0.x,
            y: self.0.y + own_scroll.0.y,
        })
    }
}

impl ScrolledContentPoint {
    /// Remove the node's own scroll again (`… + S → …`).
    #[inline]
    #[must_use]
    pub const fn unscrolled_by(self, own_scroll: ScrollOffset) -> ContentBoxLocal {
        ContentBoxLocal(LogicalPosition {
            x: self.0.x - own_scroll.0.x,
            y: self.0.y - own_scroll.0.y,
        })
    }

    /// Clamp into `0 ..= size`, staying in this space.
    ///
    /// Used when a drag leaves the block: the nearest line is wanted, not a
    /// miss.
    #[inline]
    #[must_use]
    pub const fn clamp_to(self, width: f32, height: f32) -> Self {
        Self(LogicalPosition {
            x: self.0.x.clamp(0.0, width.max(0.0)),
            y: self.0.y.clamp(0.0, height.max(0.0)),
        })
    }
}

/// Whether a tree walk starts at the node itself or at its parent.
///
/// Five different ancestor walks in this codebase encoded this choice in a
/// loop's starting value, so the difference between "the caret's own scroll
/// box" and "the scroll box around it" was invisible at the call site — and
/// the two mirror-image helper pairs (`accumulated_scroll_for_node` vs
/// `node_rect_to_screen`, `find_scrollable_ancestor` vs `find_scroll_parent`)
/// had names that gave no hint which was which.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Inclusivity {
    /// Start at the node itself. Correct when the node's own scrolling is part
    /// of the answer: how far this box's content has moved, or which box a
    /// caret inside it lives in.
    SelfAndAncestors,
    /// Start at the node's parent. Correct when the answer is about where the
    /// node's BOX sits, or which OTHER container should take over: a
    /// container's own scrolling never moves its own box, and momentum must
    /// chain outwards, not back into itself.
    AncestorsOnly,
}

impl Inclusivity {
    /// Whether the walk visits the starting node.
    #[inline]
    #[must_use]
    pub const fn includes_self(self) -> bool {
        matches!(self, Self::SelfAndAncestors)
    }
}


#[cfg(test)]
#[path = "spaces_test.rs"]
mod spaces_test;
