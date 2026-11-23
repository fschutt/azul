use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn the_default_is_what_an_empty_environment_gives() {
    assert_eq!(LayoutSettings::default(), LayoutSettings::from_lookup(&NoEnv));
}

#[test]
fn every_group_reads_its_own_variables_and_only_those() {
    let env = MapEnv::new()
        .with("AZ_NO_DL_PATCH", "1")
        .with("TEXTDBG", "1");
    let settings = LayoutSettings::from_lookup(&env);
    assert_eq!(
        settings.solver,
        SolverSettings {
            patching: false,
            ..SolverSettings::default()
        }
    );
    assert_eq!(
        settings.text,
        TextSettings {
            trace_text: true,
            ..TextSettings::default()
        }
    );
}

#[test]
fn the_groups_are_the_ones_the_modules_define() {
    let settings = LayoutSettings::default();
    assert_eq!(settings.solver, solver::SolverSettings::default());
    assert_eq!(settings.text, text::TextSettings::default());
}

/// The carriers of the groups: the objects that hold a group for the code that has no
/// environment to ask.
#[cfg(feature = "text_layout")]
mod carriers {
    use azul_css::props::basic::FontRef;
    use rust_fontconfig::FcFontCache;

    use super::*;
    use crate::{
        solver3::PassMemo,
        text3::cache::{FontManager, TextShapingCache},
        window::LayoutWindow,
    };

    fn gc_trace() -> TextSettings {
        TextSettings {
            trace_font_gc: true,
            ..TextSettings::default()
        }
    }

    #[test]
    fn a_font_manager_has_the_defaults_until_it_is_given_settings_and_its_clones_copy_them() {
        let manager = FontManager::<FontRef>::new(FcFontCache::default()).expect("a font manager");
        assert_eq!(manager.settings, TextSettings::default());
        let manager = manager.with_settings(gc_trace());
        assert_eq!(manager.settings, gc_trace());
        assert_eq!(manager.clone_shared().settings, gc_trace());
    }

    #[test]
    fn a_shaping_cache_shapes_under_the_settings_it_is_told_and_its_fork_does_too() {
        let mut cache = TextShapingCache::new();
        assert_eq!(cache.settings(), TextSettings::default());
        cache.set_settings(gc_trace());
        assert_eq!(cache.settings(), gc_trace());
        assert_eq!(cache.fork_shared().settings(), gc_trace());
    }

    #[test]
    fn the_memo_of_a_pass_holds_the_settings_it_was_made_for() {
        let solver = SolverSettings {
            trace_patch: true,
            ..SolverSettings::default()
        };
        assert_eq!(PassMemo::<FontRef>::with_settings(solver).settings, solver);
        assert_eq!(
            PassMemo::<FontRef>::default().settings,
            SolverSettings::default(),
            "a context made by hand runs under the defaults"
        );
    }

    #[test]
    fn a_window_takes_the_settings_it_is_given_and_keeps_the_skip_of_its_host() {
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        assert_eq!(lw.settings, LayoutSettings::default());
        assert_eq!(lw.display_list_options, SolverSettings::default());
        lw.display_list_options.skip = true;
        // what an environment with two switches asks for
        let settings = LayoutSettings::from_lookup(
            &MapEnv::new()
                .with("AZ_NO_DL_PATCH", "1")
                .with("AZ_FONT_GC_TRACE", "1"),
        );
        lw.set_settings(settings.clone());
        assert!(
            !lw.display_list_options.patching,
            "the solver group is what the window's passes run under"
        );
        assert!(lw.display_list_options.skip, "the web backend's skip is the host's");
        assert_eq!(lw.font_manager.settings, gc_trace());
        assert_eq!(lw.text_cache.settings(), gc_trace());
        assert_eq!(lw.settings, settings);
    }
}
