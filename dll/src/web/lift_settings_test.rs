use std::{ffi::OsString, path::PathBuf};

use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn an_empty_environment_is_the_default_the_audit_strict_and_nothing_traced() {
    let none = WebLiftSettings::from_lookup(&NoEnv);
    assert_eq!(none, WebLiftSettings::default());
    // the run
    assert!(!none.skip_layout_lift);
    assert!(none.lift_strict, "the lift audit refuses a bad build unless AZ_LIFT_STRICT=0");
    assert!(!none.shards);
    assert!(!none.preflight);
    // the toolchain
    assert_eq!(none.remill_lift_bin, None);
    assert_eq!(none.llc, None);
    assert_eq!(none.llvm_opt, None);
    assert_eq!(none.llvm_link, None);
    assert_eq!(none.wasm_ld, None);
    assert_eq!(none.wasm_opt, None);
    assert!(!none.skip_wasm_opt && !none.native_remill);
    assert_eq!(none.lift_jobs, None);
    // scratch and caches: no cache unless asked for
    assert_eq!(none.temp_dir, PathBuf::from("/tmp"));
    assert!(!none.keep_scratch && !none.lift_cache && !none.no_lift_cache && !none.reloc_verify);
    assert_eq!(none.lift_cache_dir, None);
    // the passes that are on unless a variable turns them off are fields named for the variable
    assert!(!none.keep_alias_scope && !none.no_fix_sp && !none.no_trap_selfloop);
    assert!(!none.no_indirect_dispatch && !none.full_cs_restore && !none.no_host_scope);
    assert!(!none.unk_trap);
    assert_eq!(none.lto_level, None);
    assert_eq!(none.cb_max_depth, 8192);
    assert_eq!(none.mini_max_depth, 4096);
    assert_eq!(none.opt_flag, "-O2");
    assert!(none.lowopt_fns.is_empty());
    assert_eq!(none.fuel_limit, 200_000_000);
    assert_eq!((none.lswin_hi, none.lswin_lo, none.lsid_lo), (196_608, 0, 0));
    assert!(none.instrumentation_targets().iter().all(Option::is_none));
}

#[test]
fn a_flag_variable_is_set_for_any_value_like_var_os_is_some() {
    let env = MapEnv::new()
        .with("AZ_SKIP_LAYOUT_LIFT", "")
        .with("AZ_PREFLIGHT", "0")
        .with("AZ_REMILL_KEEP_SCRATCH", "1")
        .with("AZ_NATIVE_REMILL", "no")
        .with("AZ_LIFT_CACHE", "1")
        .with("AZ_NO_LIFT_CACHE", "1")
        .with("AZ_RELOC_VERIFY", "1")
        .with("AZ_KEEP_ALIAS_SCOPE", "1")
        .with("AZ_NO_FIX_SP", "1")
        .with("AZ_NO_TRAP_SELFLOOP", "1")
        .with("AZ_NO_INDIRECT_DISPATCH", "1")
        .with("AZ_FULL_CS_RESTORE", "1")
        .with("AZ_WASM_DEBUG", "1")
        .with("AZ_REMILL_DEBUG", "1")
        .with("AZ_REMILL_MERGED_COMPILE", "1")
        .with("AZ_REMILL_DISABLE_AUTO_MERGE", "1")
        .with("AZ_REMILL_MERGED_ALWAYSINLINE", "1")
        .with("AZ_REMILL_SKIP_WASM_OPT", "1")
        .with("AZ_FORCE_WHOLE_PAGE", "1")
        .with("AZ_EMPTY_GROUP_TRACE", "1")
        .with("AZ_WASM_MIRROR_TRACE", "1")
        .with("AZ_TRACE_STALE_PTR", "1")
        .with("AZ_REG_TRACE_NOWRAP", "1")
        .with("AZ_SP_TRACE", "1");
    let s = WebLiftSettings::from_lookup(&env);
    assert!(s.skip_layout_lift, "an empty value is set");
    assert!(s.preflight, "0 is set too");
    assert!(s.keep_scratch && s.native_remill && s.lift_cache && s.no_lift_cache);
    assert!(s.reloc_verify && s.keep_alias_scope && s.no_fix_sp && s.no_trap_selfloop);
    assert!(s.no_indirect_dispatch && s.full_cs_restore && s.wasm_debug && s.remill_debug);
    assert!(s.merged_compile && s.disable_auto_merge && s.merged_alwaysinline && s.skip_wasm_opt);
    assert!(s.force_whole_page && s.empty_group_trace && s.wasm_mirror_trace);
    assert!(s.trace_stale_ptr && s.reg_trace_nowrap && s.sp_trace);
}

#[test]
fn the_audit_is_strict_unless_the_value_is_zero() {
    let lift_strict = |v: Option<&str>| {
        let env = v.map_or_else(MapEnv::new, |v| MapEnv::new().with("AZ_LIFT_STRICT", v));
        WebLiftSettings::from_lookup(&env).lift_strict
    };
    assert!(lift_strict(None));
    assert!(lift_strict(Some("1")));
    assert!(lift_strict(Some("")));
    assert!(!lift_strict(Some("0")));
}

#[test]
fn shards_need_the_opt_in_and_no_legacy_switch() {
    let shards = |pairs: &[&str]| {
        let mut env = MapEnv::new();
        for name in pairs {
            env.set(name, "1");
        }
        WebLiftSettings::from_lookup(&env).shards
    };
    assert!(!shards(&[]));
    assert!(shards(&["AZ_ENABLE_SHARDS"]));
    assert!(!shards(&["AZ_BUNDLED_LEGACY"]));
    assert!(!shards(&["AZ_ENABLE_SHARDS", "AZ_BUNDLED_LEGACY"]));
}

#[test]
fn the_unknown_trap_is_off_unless_a_value_that_is_not_zero_asks_for_it() {
    let unk = |v: &str| {
        WebLiftSettings::from_lookup(&MapEnv::new().with("AZ_UNK_TRAP", v)).unk_trap
    };
    assert!(unk("1"));
    assert!(unk(""));
    assert!(!unk("0"));
    assert!(!WebLiftSettings::default().unk_trap);
}

#[test]
fn numbers_that_do_not_parse_are_the_default_and_a_job_count_is_at_least_one() {
    let env = MapEnv::new()
        .with("AZ_CB_MAX_DEPTH", "100")
        .with("AZ_MINI_MAX_DEPTH", "many")
        .with("AZ_FUEL_LIMIT", "5000")
        .with("AZ_LSID_LO", "7")
        .with("AZ_LIFT_JOBS", "0");
    let s = WebLiftSettings::from_lookup(&env);
    assert_eq!(s.cb_max_depth, 100);
    assert_eq!(s.mini_max_depth, 4096, "garbage is the default");
    assert_eq!(s.fuel_limit, 5000);
    assert_eq!(s.lsid_lo, 7);
    assert_eq!(s.lift_jobs, None, "0 jobs is not a count");
    let jobs = |v: &str| WebLiftSettings::from_lookup(&MapEnv::new().with("AZ_LIFT_JOBS", v)).lift_jobs;
    assert_eq!(jobs("4"), Some(4));
    assert_eq!(jobs("x"), None);
}

#[test]
fn the_store_window_takes_hex_or_decimal() {
    let env = MapEnv::new()
        .with("AZ_LSWIN_HI", "0x40000")
        .with("AZ_LSWIN_LO", "12288");
    let s = WebLiftSettings::from_lookup(&env);
    assert_eq!(s.lswin_hi, 0x40000);
    assert_eq!(s.lswin_lo, 12288);
    let bad = WebLiftSettings::from_lookup(&MapEnv::new().with("AZ_LSWIN_HI", "0xzz"));
    assert_eq!(bad.lswin_hi, DEFAULT_LSWIN_HI);
}

#[test]
fn the_opt_level_picks_the_llvm_flag() {
    let flag = |v: &str| WebLiftSettings::from_lookup(&MapEnv::new().with("AZ_OPT_LEVEL", v)).opt_flag;
    assert_eq!(flag("0"), "-O0");
    assert_eq!(flag("1"), "-O1");
    assert_eq!(flag("s"), "-Os");
    assert_eq!(flag("S"), "-Os");
    assert_eq!(flag("z"), "-Oz");
    assert_eq!(flag("Z"), "-Oz");
    assert_eq!(flag("2"), "-O2");
    assert_eq!(flag("fast"), "-O2");
}

#[test]
fn the_low_opt_list_and_the_bisect_pair_are_kept_as_the_old_reads_took_them() {
    let env = MapEnv::new()
        .with("AZ_LOWOPT_FNS", "a,,b")
        .with("AZ_BISECT_FN", "layout")
        .with("AZ_BISECT_LIMIT", " 12 ");
    let s = WebLiftSettings::from_lookup(&env);
    assert_eq!(s.lowopt_fns, ["a".to_string(), "b".to_string()]);
    assert_eq!(s.bisect_fn.as_deref(), Some("layout"));
    assert_eq!(s.bisect_limit.as_deref(), Some(" 12 "), "trimmed where it is used");
}

#[test]
fn the_cache_directory_is_used_unless_it_is_empty_and_the_lto_level_likewise() {
    let env = MapEnv::new()
        .with("AZ_LIFT_CACHE_DIR", "/baked/cache")
        .with("AZ_LTO_LEVEL", "2");
    let s = WebLiftSettings::from_lookup(&env);
    assert_eq!(s.lift_cache_dir, Some(PathBuf::from("/baked/cache")));
    assert_eq!(s.lto_level.as_deref(), Some("2"));
    let empty = WebLiftSettings::from_lookup(
        &MapEnv::new().with("AZ_LIFT_CACHE_DIR", "").with("AZ_LTO_LEVEL", ""),
    );
    assert_eq!(empty.lift_cache_dir, None);
    assert_eq!(empty.lto_level, None);
}

#[test]
fn the_tools_and_the_tracers_are_text() {
    let env = MapEnv::new()
        .with("LLC", "/opt/llvm/llc")
        .with("LLVM_OPT", "/opt/llvm/opt")
        .with("LLVM_LINK", "/opt/llvm/llvm-link")
        .with("WASM_LD", "/opt/llvm/wasm-ld")
        .with("WASM_OPT", "/opt/binaryen/wasm-opt")
        .with("REMILL_LIFT_BIN", "/opt/remill/remill-lift-17")
        .with("AZ_WRITE_TRACE", "ALL")
        .with("AZ_FUEL", "layout,hydrate")
        .with("TMPDIR", "/scratch");
    let s = WebLiftSettings::from_lookup(&env);
    assert_eq!(s.llc.as_deref(), Some("/opt/llvm/llc"));
    assert_eq!(s.llvm_opt.as_deref(), Some("/opt/llvm/opt"));
    assert_eq!(s.llvm_link.as_deref(), Some("/opt/llvm/llvm-link"));
    assert_eq!(s.wasm_ld.as_deref(), Some("/opt/llvm/wasm-ld"));
    assert_eq!(s.wasm_opt.as_deref(), Some("/opt/binaryen/wasm-opt"));
    assert_eq!(s.remill_lift_bin_text(), Some("/opt/remill/remill-lift-17"));
    assert_eq!(s.temp_dir, PathBuf::from("/scratch"));
    let targets = s.instrumentation_targets();
    assert_eq!(targets[0], Some("ALL"));
    assert_eq!(targets[6], Some("layout,hydrate"));
    assert_eq!(targets.iter().flatten().count(), 2);
}

/// `var_os(..).is_some()` was true for a value that is not UTF-8, `var(..)` was not.
#[cfg(unix)]
#[test]
fn a_value_that_is_not_utf8_is_set_for_a_flag_and_no_text_for_a_tool_path() {
    use std::os::unix::ffi::OsStringExt;
    let raw = || OsString::from_vec(vec![0x2f, 0x80]);
    let env = MapEnv::new()
        .with_os("AZ_NO_FIX_SP", raw())
        .with_os("AZ_NO_HOST_SCOPE", raw())
        .with_os("REMILL_LIFT_BIN", raw())
        .with_os("LLC", raw());
    let s = WebLiftSettings::from_lookup(&env);
    assert!(s.no_fix_sp, "var_os(..).is_some()");
    assert!(!s.no_host_scope, "var(..).is_ok() is false for it");
    assert_eq!(s.remill_lift_bin, Some(raw()));
    assert_eq!(s.remill_lift_bin_text(), None);
    assert_eq!(s.llc, None);
}
