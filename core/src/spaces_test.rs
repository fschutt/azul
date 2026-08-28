#[cfg(test)]
mod tests {
    use super::*;

    const fn p(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition { x, y }
    }

    #[test]
    fn the_full_chain_reproduces_the_raster_equation() {
        // The raster paints inline position g at `P + E + g - S - A`.
        // Feeding that window position through the chain must return g.
        let node_origin = p(100.0, 50.0); // P
        let ancestor = ScrollOffset::new(7.0, 13.0); // A
        let own = ScrollOffset::new(30.0, 4.0); // S
        let inset = ContentInset::new(6.0, 3.0); // E
        let g = p(12.0, 9.0);

        let window = WindowPoint::new(p(
            node_origin.x + inset.left + g.x - own.get().x - ancestor.get().x,
            node_origin.y + inset.top + g.y - own.get().y - ancestor.get().y,
        ));

        let content = window
            .to_static_layout(ancestor)
            .to_border_box_local(node_origin)
            .to_content_box_local(inset)
            .scrolled_by(own);

        assert_eq!(content.get(), g);
    }

    #[test]
    fn every_conversion_round_trips() {
        let w = WindowPoint::new(p(3.5, -8.25));
        let a = ScrollOffset::new(11.0, 2.0);
        let origin = p(-4.0, 60.0);
        let inset = ContentInset::new(5.0, 5.0);
        let own = ScrollOffset::new(90.0, 0.5);

        let s = w.to_static_layout(a);
        assert_eq!(s.to_window(a).get(), w.get());

        let b = s.to_border_box_local(origin);
        assert_eq!(b.to_static_layout(origin).get(), s.get());

        let c = b.to_content_box_local(inset);
        assert_eq!(c.to_border_box_local(inset).get(), b.get());

        let sc = c.scrolled_by(own);
        assert_eq!(sc.unscrolled_by(own).get(), c.get());
    }

    #[test]
    fn a_zero_inset_makes_border_and_content_boxes_agree() {
        // Why the WebRender/CPU divergence stayed latent: the TextInput's
        // value <p> sets neither padding nor border.
        let b = BorderBoxLocal::new(p(17.0, 4.0));
        assert_eq!(b.to_content_box_local(ContentInset::ZERO).get(), b.get());
    }

    #[test]
    fn own_scroll_shifts_the_content_point_right_by_the_offset() {
        // A field scrolled 40px right: the pointer sitting on the box's left
        // edge is 40px into the CONTENT.
        let c = ContentBoxLocal::new(p(0.0, 5.0));
        let scrolled = c.scrolled_by(ScrollOffset::new(40.0, 0.0));
        assert_eq!(scrolled.get(), p(40.0, 5.0));
    }

    #[test]
    fn scroll_offsets_accumulate() {
        let total = ScrollOffset::new(1.0, 2.0).plus(ScrollOffset::new(10.0, 20.0));
        assert_eq!(total.get(), p(11.0, 22.0));
        assert_eq!(ScrollOffset::zero().plus(total).get(), total.get());
    }

    #[test]
    fn clamping_keeps_the_point_inside_the_block() {
        let sc = ScrolledContentPoint::new(p(-5.0, 900.0));
        assert_eq!(sc.clamp_to(100.0, 40.0).get(), p(0.0, 40.0));
        // A degenerate (negative) size clamps to the origin rather than
        // panicking on an inverted range.
        assert_eq!(sc.clamp_to(-1.0, -1.0).get(), p(0.0, 0.0));
    }

    #[test]
    fn inclusivity_is_explicit() {
        assert!(Inclusivity::SelfAndAncestors.includes_self());
        assert!(!Inclusivity::AncestorsOnly.includes_self());
    }

    #[test]
    fn the_newtypes_are_free() {
        use core::mem::{align_of, size_of};
        assert_eq!(size_of::<WindowPoint>(), size_of::<LogicalPosition>());
        assert_eq!(size_of::<StaticLayoutPoint>(), size_of::<LogicalPosition>());
        assert_eq!(size_of::<BorderBoxLocal>(), size_of::<LogicalPosition>());
        assert_eq!(size_of::<ContentBoxLocal>(), size_of::<LogicalPosition>());
        assert_eq!(
            size_of::<ScrolledContentPoint>(),
            size_of::<LogicalPosition>()
        );
        assert_eq!(size_of::<ScrollOffset>(), size_of::<LogicalPosition>());
        assert_eq!(
            align_of::<ScrolledContentPoint>(),
            align_of::<LogicalPosition>()
        );
    }

    #[test]
    fn nan_and_infinity_pass_through_without_panicking() {
        let w = WindowPoint::new(p(f32::NAN, f32::INFINITY));
        let out = w
            .to_static_layout(ScrollOffset::new(1.0, 1.0))
            .to_border_box_local(p(1.0, 1.0))
            .to_content_box_local(ContentInset::new(1.0, 1.0))
            .scrolled_by(ScrollOffset::new(1.0, 1.0));
        assert!(out.x().is_nan());
        assert!(out.y().is_infinite());
    }
}
