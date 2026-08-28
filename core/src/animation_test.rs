#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{LogicalPosition, LogicalRect, LogicalSize};

    /// `Spring` is a real `AnimationInterpolationFunction` variant now, so it
    /// must survive the C-ABI enum's own accessors rather than panicking or
    /// silently answering as some other curve.
    #[test]
    fn spring_is_a_first_class_interpolation_function() {
        let f = AnimationInterpolationFunction::Spring(Spring::SNAPPY);
        assert!(f.is_spring());
        assert!(!AnimationInterpolationFunction::EaseInOut.is_spring());
        // No duration to evaluate against: it answers as the documented
        // ease-in-out stand-in, and `get_curve`/`ease` must not disagree.
        assert_eq!(
            f.get_curve(),
            AnimationInterpolationFunction::EaseInOut.get_curve()
        );
        for t in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            assert!((ease(f, t) - ease(AnimationInterpolationFunction::EaseInOut, t)).abs() < 1e-6);
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition::new(x, y),
            size: LogicalSize::new(w, h),
        }
    }

    #[test]
    fn flip_inverts_a_pure_translation() {
        // Moved right 100 and down 50, same size: the inversion must put it back.
        let f = flip(rect(0.0, 0.0, 10.0, 10.0), rect(100.0, 50.0, 10.0, 10.0));
        assert_eq!(f.translate_x, -100.0);
        assert_eq!(f.translate_y, -50.0);
        assert_eq!(f.scale_x, 1.0);
        assert_eq!(f.scale_y, 1.0);
    }

    #[test]
    fn flip_never_scales_a_size_change() {
        // USER ruling 2026-08-17 (was `flip_inverts_a_pure_scale`, asserting
        // 0.5): a resized node has already RELAYOUTED at its final size, and
        // drawing it at half scale for the flight squashes freshly laid-out
        // content — a card growing from half-width to full-width rendered its
        // text visibly compressed for the whole transition. Size is layout's
        // job; the animation only travels.
        let f = flip(rect(0.0, 0.0, 50.0, 20.0), rect(0.0, 0.0, 100.0, 40.0));
        assert_eq!(f.scale_x, 1.0);
        assert_eq!(f.scale_y, 1.0);
        assert!(
            f.is_identity(),
            "same origin, changed size: nothing to animate"
        );
    }

    #[test]
    fn flip_of_an_unchanged_rect_is_identity() {
        let r = rect(12.0, 34.0, 56.0, 78.0);
        assert!(flip(r, r).is_identity());
    }

    #[test]
    fn flip_never_produces_a_non_finite_scale() {
        // A collapsed target would divide by zero; the display list must never
        // see a NaN transform.
        let f = flip(rect(0.0, 0.0, 10.0, 10.0), rect(0.0, 0.0, 0.0, 0.0));
        assert!(f.scale_x.is_finite() && f.scale_y.is_finite());
        assert_eq!(f.scale_x, 1.0);
        assert_eq!(f.scale_y, 1.0);
    }

    #[test]
    fn a_spring_settles_at_its_target() {
        let mut c = AnimChannel::spring(0.0, 100.0, Spring::SMOOTH);
        for _ in 0..600 {
            c.tick(1.0 / 60.0);
            if c.is_finished() {
                break;
            }
        }
        assert!(c.is_finished(), "spring did not settle within 10s");
        assert_eq!(c.current, 100.0);
        assert_eq!(c.velocity, 0.0);
    }

    #[test]
    fn a_curve_reaches_its_target_at_the_duration() {
        // NOTE the frame budget: 60 ticks of 1/60 sum to 0.99999994, not 1.0,
        // so a curve legitimately lands on the frame AFTER its nominal
        // duration. Asserting exact arrival at tick 60 would be asserting that
        // f32 addition is exact.
        let mut c = AnimChannel::curve(0.0, 10.0, AnimationInterpolationFunction::Linear, 1.0);
        for _ in 0..60 {
            c.tick(1.0 / 60.0);
        }
        assert!(
            (c.current - 10.0).abs() < 0.01,
            "should be at the target within a frame, got {}",
            c.current
        );
        c.tick(1.0 / 60.0);
        assert!(
            c.is_finished(),
            "curve did not finish one frame past its duration"
        );
        assert_eq!(
            c.current, 10.0,
            "a finished curve must land exactly on `to`"
        );
    }

    #[test]
    fn retarget_preserves_position_and_velocity() {
        // THE differentiator: mid-flight redirect must not snap back to a new
        // `from`, and must keep the momentum it had.
        let mut c = AnimChannel::spring(0.0, 100.0, Spring::SMOOTH);
        for _ in 0..10 {
            c.tick(1.0 / 60.0);
        }
        let value_before = c.current;
        let velocity_before = c.velocity;
        assert!(
            value_before > 0.0 && velocity_before > 0.0,
            "should be mid-flight"
        );

        c.retarget(-50.0);

        assert_eq!(c.current, value_before, "retarget must not move the value");
        assert_eq!(
            c.velocity, velocity_before,
            "retarget must not discard velocity"
        );
        assert_eq!(c.from, value_before);
        assert_eq!(c.to, -50.0);
        assert!(!c.is_finished());
    }

    #[test]
    fn retargeting_to_the_same_target_does_not_restart_the_clock() {
        let mut c = AnimChannel::curve(0.0, 10.0, AnimationInterpolationFunction::Linear, 1.0);
        c.tick(0.5);
        let elapsed = c.elapsed_secs;
        c.retarget(10.0);
        assert_eq!(
            c.elapsed_secs, elapsed,
            "a no-op retarget restarted the animation"
        );
    }

    #[test]
    fn a_settled_spring_can_be_woken_by_a_retarget() {
        let mut c = AnimChannel::spring(0.0, 1.0, Spring::SNAPPY);
        for _ in 0..600 {
            c.tick(1.0 / 60.0);
            if c.is_finished() {
                break;
            }
        }
        assert!(c.is_finished());
        c.retarget(0.0);
        assert!(
            !c.is_finished(),
            "retarget must un-finish a settled channel"
        );
        c.tick(1.0 / 60.0);
        assert!(
            c.current < 1.0,
            "woken channel did not move toward the new target"
        );
    }

    #[test]
    fn a_huge_frame_gap_cannot_fling_a_spring() {
        // A stalled frame must be clamped, not integrated verbatim.
        let mut c = AnimChannel::spring(0.0, 1.0, Spring::SNAPPY);
        c.tick(10.0);
        assert!(c.current.is_finite());
        assert!(c.current.abs() < 100.0, "clamping failed: {}", c.current);
    }

    #[test]
    fn zero_duration_curves_apply_instantly() {
        let mut c = AnimChannel::curve(0.0, 42.0, AnimationInterpolationFunction::Ease, 0.0);
        c.tick(0.0);
        assert!(c.is_finished());
        assert_eq!(c.current, 42.0);
    }

    #[test]
    fn easing_curves_are_pinned_at_both_ends() {
        for f in [
            AnimationInterpolationFunction::Linear,
            AnimationInterpolationFunction::Ease,
            AnimationInterpolationFunction::EaseIn,
            AnimationInterpolationFunction::EaseOut,
            AnimationInterpolationFunction::EaseInOut,
        ] {
            assert_eq!(ease(f, 0.0), 0.0, "{f:?} did not start at 0");
            assert_eq!(ease(f, 1.0), 1.0, "{f:?} did not end at 1");
            // Out of range must clamp, not extrapolate.
            assert_eq!(ease(f, -1.0), 0.0);
            assert_eq!(ease(f, 2.0), 1.0);
        }
    }

    #[test]
    fn ease_in_starts_slower_than_linear_and_ease_out_starts_faster() {
        let t = 0.25;
        let linear = ease(AnimationInterpolationFunction::Linear, t);
        assert!(ease(AnimationInterpolationFunction::EaseIn, t) < linear);
        assert!(ease(AnimationInterpolationFunction::EaseOut, t) > linear);
    }

    #[test]
    fn damping_ratio_identifies_the_regime() {
        // Critically damped: damping = 2*sqrt(k*m).
        let critical = Spring {
            stiffness: 100.0,
            damping: 20.0,
            mass: 1.0,
        };
        assert!((critical.damping_ratio() - 1.0).abs() < 1e-5);
        assert!(
            Spring {
                stiffness: 100.0,
                damping: 5.0,
                mass: 1.0
            }
            .damping_ratio()
                < 1.0
        );
        assert!(
            Spring {
                stiffness: 100.0,
                damping: 40.0,
                mass: 1.0
            }
            .damping_ratio()
                > 1.0
        );
    }

    #[test]
    fn a_degenerate_spring_snaps_instead_of_dividing_by_zero() {
        let s = Spring {
            stiffness: 100.0,
            damping: 10.0,
            mass: 0.0,
        };
        let (value, velocity) = s.step(0.0, 5.0, 0.0, 1.0 / 60.0);
        assert_eq!(value, 5.0);
        assert_eq!(velocity, 0.0);
    }

    #[test]
    fn the_manager_retargets_instead_of_stacking() {
        let mut m = AnimationManager::new();
        let key = AnimKey(7);
        let interp = Interp::Spring(Spring::SMOOTH);

        m.start_or_retarget_move(
            key,
            flip(rect(0.0, 0.0, 10.0, 10.0), rect(100.0, 0.0, 10.0, 10.0)),
            interp,
        );
        assert_eq!(m.len(), 1);
        for _ in 0..10 {
            m.tick(1.0 / 60.0);
        }
        let mid = m.get(key).expect("still animating").current_transform();

        // A second move for the SAME key must not create a second animation.
        m.start_or_retarget_move(
            key,
            flip(rect(0.0, 0.0, 10.0, 10.0), rect(200.0, 0.0, 10.0, 10.0)),
            interp,
        );
        assert_eq!(m.len(), 1, "retarget created a second animation");
        let after = m.get(key).expect("still animating").current_transform();
        assert_ne!(
            after.translate_x, mid.translate_x,
            "retarget did not fold in the new offset"
        );
    }

    #[test]
    fn the_manager_reports_and_drops_finished_animations() {
        let mut m = AnimationManager::new();
        m.start_enter(
            AnimKey(1),
            (-120.0, 0.0),
            Interp::Curve {
                function: AnimationInterpolationFunction::Linear,
                duration_secs: 0.1,
            },
        );
        assert_eq!(m.len(), 1);
        let mut finished = Vec::new();
        for _ in 0..20 {
            finished = m.tick(1.0 / 60.0);
            if !finished.is_empty() {
                break;
            }
        }
        assert_eq!(finished, alloc::vec![AnimKey(1)]);
        assert!(m.is_empty(), "finished animation was not dropped");
    }

    #[test]
    fn an_exit_replaces_an_in_flight_move() {
        // The node is leaving; continuing toward a layout slot it will never
        // occupy would be wrong.
        let mut m = AnimationManager::new();
        let key = AnimKey(3);
        let interp = Interp::Spring(Spring::SMOOTH);
        m.start_or_retarget_move(
            key,
            flip(rect(0.0, 0.0, 10.0, 10.0), rect(50.0, 0.0, 10.0, 10.0)),
            interp,
        );
        assert_eq!(m.get(key).map(|a| a.class), Some(AnimClass::Move));
        m.start_exit(key, (-120.0, 0.0), interp);
        assert_eq!(m.get(key).map(|a| a.class), Some(AnimClass::Exit));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn the_anim_key_survives_a_node_id_change() {
        // THE property the whole store depends on. A keyed node that shifts
        // position in the array (a sibling was prepended) must keep its
        // identity — otherwise the second produce looks like a brand-new
        // animation and retargeting never fires.
        use crate::dom::NodeData;

        let tree_a = [NodeData::create_div().with_key("hero")];
        let tree_b = [
            NodeData::create_div().with_key("spacer"),
            NodeData::create_div().with_key("hero"),
        ];

        let key_a = AnimKey(calculate_reconciliation_key(&tree_a, &[], NodeId::ZERO));
        let key_b = AnimKey(calculate_reconciliation_key(&tree_b, &[], NodeId::new(1)));
        assert_eq!(
            key_a, key_b,
            "the same keyed node got two different AnimKeys"
        );

        // And a DIFFERENT key must not collide with it.
        let other = AnimKey(calculate_reconciliation_key(&tree_b, &[], NodeId::ZERO));
        assert_ne!(key_a, other);
    }

    #[test]
    fn correspondences_drop_pairs_with_no_geometry() {
        use crate::dom::NodeData;
        let new_data = [
            NodeData::create_div().with_key("a"),
            NodeData::create_div().with_key("b"),
        ];
        let moves = [
            NodeMove {
                old_node_id: NodeId::ZERO,
                new_node_id: NodeId::ZERO,
            },
            NodeMove {
                old_node_id: NodeId::new(1),
                new_node_id: NodeId::new(1),
            },
        ];
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let out = correspondences_from_moves(
            &moves,
            &new_data,
            &[],
            |id| (id == NodeId::ZERO).then_some(r), // only node 0 existed before
            |_| Some(rect(5.0, 0.0, 10.0, 10.0)),
        );
        assert_eq!(
            out.len(),
            1,
            "a node with no previous geometry has nothing to fly from"
        );
    }

    #[test]
    fn seed_moves_skips_nodes_that_did_not_move() {
        // An identity FLIP would take a GPU key and animate nothing.
        let mut m = AnimationManager::new();
        let stayed = rect(0.0, 0.0, 10.0, 10.0);
        let moved_first = rect(0.0, 0.0, 10.0, 10.0);
        let moved_last = rect(40.0, 0.0, 10.0, 10.0);
        let seeded = seed_moves(
            &mut m,
            [
                (AnimKey(1), stayed, stayed),
                (AnimKey(2), moved_first, moved_last),
            ],
            Interp::Spring(Spring::SMOOTH),
        );
        assert_eq!(seeded, 1, "only the node that moved should animate");
        assert!(m.get(AnimKey(1)).is_none());
        assert!(m.get(AnimKey(2)).is_some());
    }

    #[test]
    fn seed_moves_retargets_a_key_that_is_already_animating() {
        // Two produces in quick succession must not stack two animations on
        // one node — that is the visible "fighting" artefact.
        let mut m = AnimationManager::new();
        let interp = Interp::Spring(Spring::SMOOTH);
        seed_moves(
            &mut m,
            [(
                AnimKey(9),
                rect(0.0, 0.0, 10.0, 10.0),
                rect(50.0, 0.0, 10.0, 10.0),
            )],
            interp,
        );
        for _ in 0..5 {
            m.tick(1.0 / 60.0);
        }
        let seeded = seed_moves(
            &mut m,
            [(
                AnimKey(9),
                rect(0.0, 0.0, 10.0, 10.0),
                rect(90.0, 0.0, 10.0, 10.0),
            )],
            interp,
        );
        assert_eq!(seeded, 1);
        assert_eq!(m.len(), 1, "a second produce stacked a second animation");
    }

    #[test]
    fn an_enter_does_not_clobber_an_animation_already_in_flight() {
        let mut m = AnimationManager::new();
        let key = AnimKey(5);
        let interp = Interp::Spring(Spring::SMOOTH);
        m.start_exit(key, (-120.0, 0.0), interp);
        m.start_enter(key, (-120.0, 0.0), interp);
        assert_eq!(m.get(key).map(|a| a.class), Some(AnimClass::Exit));
    }
}
