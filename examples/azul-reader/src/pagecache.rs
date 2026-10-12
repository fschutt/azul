//! The pages drawn ahead: pictures of the pages around the one on screen.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_and_the_next_spread_are_drawn_first_in_one_run() {
        assert_eq!(next_run(4, 2, 20, &[]), Some((4, 4)));
    }

    #[test]
    fn then_the_spread_before_the_view_is_drawn() {
        assert_eq!(next_run(4, 2, 20, &[(4, 4)]), Some((2, 2)));
    }

    #[test]
    fn nothing_is_drawn_when_the_pages_around_the_view_are() {
        assert_eq!(next_run(4, 2, 20, &[(2, 2), (4, 4)]), None);
        assert_eq!(next_run(4, 2, 20, &[(0, 4), (4, 4)]), None);
    }

    #[test]
    fn the_runs_stop_at_the_chapters_ends() {
        assert_eq!(next_run(18, 2, 20, &[]), Some((18, 2)));
        assert_eq!(next_run(18, 2, 20, &[(18, 2)]), Some((16, 2)));
        assert_eq!(next_run(0, 1, 1, &[]), Some((0, 1)));
        assert_eq!(next_run(0, 1, 1, &[(0, 1)]), None);
    }

    #[test]
    fn a_run_far_from_the_view_is_let_go() {
        assert!(keeps((2, 2), 4, 2, 20));
        assert!(keeps((6, 4), 4, 2, 20));
        assert!(!keeps((10, 4), 4, 2, 20));
        assert!(!keeps((0, 2), 4, 2, 20));
    }
}
