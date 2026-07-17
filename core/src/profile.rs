//! Unified profiling gate.
//!
//! Reads `AZ_PROFILE` once on first access, caches the result forever.
//! Value is a comma-separated list of tokens; unknown tokens are ignored,
//! whitespace is trimmed, matching is case-insensitive.
//!
//! Tokens:
//! - `memory`  — heap-breakdown dumps (StyledDom, LayoutCache, text cache,
//!               cascade maps, RSS). Printed to stderr once per frame.
//! - `cpu`     — per-phase wall-clock timings from `Probe::span` (layout,
//!               style, cascade, paint, callbacks, …), dumped once per
//!               frame so stuttering frames are easy to spot.
//! - `cascade` — narrow diagnostic for prop-cache work: top-N CSS
//!               properties by cascade-walk count per frame.
//! - `heap`    — phase-boundary heap probes in `regenerate_layout`
//!               (`emit_phase_heap`). By themselves print nothing —
//!               pair with `jsonl` + `AZ_PROFILE_OUT` to persist.
//! - `jsonl`   — format heap probes as JSONL to the file named by
//!               `AZ_PROFILE_OUT=<path>`. Requires `heap` to do anything.
//! - `detail`  — opt-in to the fine-grained per-step probes inside each
//!               phase (e.g. `rf_*` labels inside
//!               `rust_fontconfig::request_fonts`, and the `_extra`
//!               cache-size payloads). Layered on top of `heap`.
//!
//! ## Examples
//! - `AZ_PROFILE=cpu` — per-phase CPU timings to stderr.
//! - `AZ_PROFILE=heap,jsonl AZ_PROFILE_OUT=/tmp/run.jsonl`
//!     → coarse phase heap probes to JSONL.
//! - `AZ_PROFILE=heap,jsonl,detail AZ_PROFILE_OUT=/tmp/detail.jsonl`
//!     → fine-grained (per-step) heap probes to JSONL.
//! - `AZ_PROFILE=cpu,cascade` — both dumps simultaneously.
//!
//! Tokens are independent flags, not mutually exclusive modes. Unset
//! or empty leaves every quick path silent.
//!
//! ## Path for jsonl output
//! `AZ_PROFILE_OUT` is read separately (not folded into `AZ_PROFILE`
//! because the value can contain `,` and `=` and a path is a different
//! shape from a flag). When `jsonl` is set but `AZ_PROFILE_OUT` is
//! unset, writers silently skip — no stderr fallback so benchmarks
//! don't get polluted.
//!
//! ## Portability
//! - **macOS / Linux**: full support. Span timings via `Instant`; RSS
//!   checkpoints via `task_info` / `/proc/self/statm`.
//! - **Windows**: span timings work. RSS checkpoints silently read 0
//!   (the RSS helpers in `azul_layout::probe` are `cfg(unix)`-gated).
//! - **WASM (`target_family = "wasm"`)**: `Instant::now()` panics on
//!   browser WASM (no monotonic clock) and `libc::getrusage` isn't
//!   available. The probe module detects WASM at compile time and
//!   forces the no-op impl.

#[cfg(feature = "std")]
use std::sync::OnceLock;

/// Set of active `AZ_PROFILE` tokens. Parsed once from the env var.
// independent profile toggles parsed from the env var; a bitflags type would
// not improve this flat set of named booleans.
#[allow(clippy::struct_excessive_bools)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileFlags {
    pub memory: bool,
    pub cpu: bool,
    pub cascade: bool,
    pub heap: bool,
    pub jsonl: bool,
    pub detail: bool,
}

impl ProfileFlags {
    fn parse(value: &str) -> Self {
        let mut f = Self::default();
        for tok in value.split(',') {
            let t = tok.trim();
            if t.eq_ignore_ascii_case("memory") || t.eq_ignore_ascii_case("mem") {
                f.memory = true;
            } else if t.eq_ignore_ascii_case("cpu") || t.eq_ignore_ascii_case("perf") {
                f.cpu = true;
            } else if t.eq_ignore_ascii_case("cascade") || t.eq_ignore_ascii_case("css") {
                f.cascade = true;
            } else if t.eq_ignore_ascii_case("heap") {
                f.heap = true;
            } else if t.eq_ignore_ascii_case("jsonl") {
                f.jsonl = true;
            } else if t.eq_ignore_ascii_case("detail") {
                f.detail = true;
            }
        }
        f
    }
}

#[cfg(feature = "std")]
#[inline]
pub fn flags() -> ProfileFlags {
    static FLAGS: OnceLock<ProfileFlags> = OnceLock::new();
    *FLAGS.get_or_init(|| {
        std::env::var("AZ_PROFILE")
            .map(|v| ProfileFlags::parse(&v))
            .unwrap_or_default()
    })
}

/// `no_std` builds have no environment; profiling is always off.
#[cfg(not(feature = "std"))]
#[inline]
pub fn flags() -> ProfileFlags {
    let _ = ProfileFlags::parse;
    ProfileFlags::default()
}

/// `AZ_PROFILE_OUT=<path>` — destination for JSONL heap probes.
/// Returns `None` if unset. Cached on first access.
#[cfg(feature = "std")]
#[inline]
pub fn out_path() -> Option<&'static str> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var("AZ_PROFILE_OUT").ok())
        .as_deref()
}

/// `no_std` builds have no environment; no output path.
#[cfg(not(feature = "std"))]
#[inline]
pub fn out_path() -> Option<&'static str> {
    None
}

#[inline]
#[must_use] pub fn memory_enabled() -> bool { flags().memory }

#[inline]
#[must_use] pub fn cpu_enabled() -> bool { flags().cpu }

#[inline]
#[must_use] pub fn cascade_enabled() -> bool { flags().cascade }

#[inline]
#[must_use] pub fn heap_enabled() -> bool { flags().heap }

#[inline]
#[must_use] pub fn jsonl_enabled() -> bool { flags().jsonl }

#[inline]
#[must_use] pub fn detail_enabled() -> bool { flags().detail }

#[cfg(test)]
mod tests {
    use super::ProfileFlags;

    #[test]
    fn parse_single_token() {
        let f = ProfileFlags::parse("cpu");
        assert!(f.cpu && !f.memory && !f.heap);
    }

    #[test]
    fn parse_multiple_tokens() {
        let f = ProfileFlags::parse("heap,jsonl,detail");
        assert!(f.heap && f.jsonl && f.detail);
        assert!(!f.cpu && !f.memory);
    }

    #[test]
    fn parse_is_case_insensitive_and_trims() {
        let f = ProfileFlags::parse(" Heap , JSONL ");
        assert!(f.heap && f.jsonl);
    }

    #[test]
    fn parse_ignores_unknown_tokens() {
        let f = ProfileFlags::parse("cpu,bogus,heap");
        assert!(f.cpu && f.heap);
    }

    #[test]
    fn parse_accepts_aliases() {
        let f = ProfileFlags::parse("mem,perf,css");
        assert!(f.memory && f.cpu && f.cascade);
    }
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison)]
mod autotest_generated {
    use alloc::{
        string::{String, ToString},
        vec::Vec,
    };

    use super::*;

    // ---- helpers ---------------------------------------------------------

    /// Canonical token for every flag, in field-declaration order.
    const CANONICAL: [&str; 6] = ["memory", "cpu", "cascade", "heap", "jsonl", "detail"];

    /// The documented aliases, paired with the canonical token they mean.
    const ALIASES: [(&str, &str); 3] = [("mem", "memory"), ("perf", "cpu"), ("css", "cascade")];

    /// Read field `idx` of a flag set, indices matching `CANONICAL`.
    fn field(f: &ProfileFlags, idx: usize) -> bool {
        match idx {
            0 => f.memory,
            1 => f.cpu,
            2 => f.cascade,
            3 => f.heap,
            4 => f.jsonl,
            5 => f.detail,
            _ => unreachable!("CANONICAL has 6 entries"),
        }
    }

    /// Inverse of `ProfileFlags::parse`: render a flag set as an `AZ_PROFILE`
    /// value. Used for the encode/decode round-trip below.
    fn encode(f: ProfileFlags) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if f.memory {
            parts.push("memory");
        }
        if f.cpu {
            parts.push("cpu");
        }
        if f.cascade {
            parts.push("cascade");
        }
        if f.heap {
            parts.push("heap");
        }
        if f.jsonl {
            parts.push("jsonl");
        }
        if f.detail {
            parts.push("detail");
        }
        parts.join(",")
    }

    /// Build a flag set from a 6-bit mask (bit i == field i in `CANONICAL`).
    fn from_mask(mask: u8) -> ProfileFlags {
        ProfileFlags {
            memory: mask & 0b00_0001 != 0,
            cpu: mask & 0b00_0010 != 0,
            cascade: mask & 0b00_0100 != 0,
            heap: mask & 0b00_1000 != 0,
            jsonl: mask & 0b01_0000 != 0,
            detail: mask & 0b10_0000 != 0,
        }
    }

    fn any_set(f: &ProfileFlags) -> bool {
        f.memory || f.cpu || f.cascade || f.heap || f.jsonl || f.detail
    }

    /// Core soundness invariant: a flag can only be set if the token that
    /// enables it literally occurs in the input (ASCII-case-insensitively).
    /// `parse` never invents flags out of thin air.
    fn assert_flags_are_justified(input: &str, f: &ProfileFlags) {
        let lower = input.to_ascii_lowercase();
        if f.memory {
            assert!(lower.contains("memory") || lower.contains("mem"));
        }
        if f.cpu {
            assert!(lower.contains("cpu") || lower.contains("perf"));
        }
        if f.cascade {
            assert!(lower.contains("cascade") || lower.contains("css"));
        }
        if f.heap {
            assert!(lower.contains("heap"));
        }
        if f.jsonl {
            assert!(lower.contains("jsonl"));
        }
        if f.detail {
            assert!(lower.contains("detail"));
        }
    }

    // ---- parser: empty / whitespace / separators -------------------------

    #[test]
    fn parse_empty_input_is_all_off() {
        assert_eq!(ProfileFlags::parse(""), ProfileFlags::default());
        assert!(!any_set(&ProfileFlags::parse("")));
    }

    #[test]
    fn parse_whitespace_only_is_all_off() {
        for input in [
            " ",
            "   ",
            "\t",
            "\n",
            "\r\n",
            "\t\n",
            " \t\r\n\x0c ",
            "\u{a0}",       // NBSP (Unicode White_Space)
            "\u{2003}",     // EM SPACE
            "\u{3000}",     // IDEOGRAPHIC SPACE
        ] {
            let f = ProfileFlags::parse(input);
            assert_eq!(f, ProfileFlags::default(), "input {input:?} set a flag");
        }
    }

    #[test]
    fn parse_separators_only_is_all_off() {
        for input in [",", ",,", ",,,,,,,,,,", " , , ", "\t,\n,\r", ",,cpu,,"] {
            let f = ProfileFlags::parse(input);
            assert_eq!(f.memory, false);
            assert_eq!(f.cascade, false);
            assert_eq!(f.heap, false);
            assert_eq!(f.jsonl, false);
            assert_eq!(f.detail, false);
        }
        // ...but real tokens surrounded by empty ones still register.
        assert!(ProfileFlags::parse(",,cpu,,").cpu);
        assert!(!ProfileFlags::parse(",,,,").cpu);
    }

    // ---- parser: garbage / junk ------------------------------------------

    #[test]
    fn parse_garbage_never_panics_and_sets_nothing() {
        for input in [
            "\0",
            "\0\0\0",
            "cpu\0",              // NUL is not whitespace -> not trimmed -> no match
            "%s%n%s%n",
            "../../etc/passwd",
            "{\"cpu\":true}",
            "-1",
            "--cpu",
            "cpu=1",
            "cpu=true",
            "CPU;HEAP",           // ';' is not a separator
            "cpu heap",           // ' ' is not a separator
            "cpu\tjsonl",
            "cpux",
            "xcpu",
            "cp",
            "c,p,u",
            "\u{7f}\u{1}\u{2}",
            "\\x63\\x70\\x75",
        ] {
            let f = ProfileFlags::parse(input);
            assert_eq!(
                f,
                ProfileFlags::default(),
                "garbage input {input:?} should set no flags, got {f:?}"
            );
        }
    }

    #[test]
    fn parse_leading_trailing_junk_is_trimmed_or_rejected() {
        // Surrounding ASCII whitespace is trimmed -> token still matches.
        assert!(ProfileFlags::parse("  cpu  ").cpu);
        assert!(ProfileFlags::parse("\t\ncpu\r\n").cpu);
        assert!(ProfileFlags::parse("  heap ,  jsonl  ").heap);
        assert!(ProfileFlags::parse("  heap ,  jsonl  ").jsonl);

        // Non-whitespace junk glued to the token is *not* stripped: the token
        // must match exactly, so "valid;garbage" is rejected wholesale.
        assert_eq!(ProfileFlags::parse("cpu;garbage"), ProfileFlags::default());
        assert_eq!(ProfileFlags::parse("garbage;cpu"), ProfileFlags::default());
        assert_eq!(ProfileFlags::parse("'cpu'"), ProfileFlags::default());
        assert_eq!(ProfileFlags::parse("\"cpu\""), ProfileFlags::default());

        // ...but a junk *token* next to a valid token only kills itself.
        let f = ProfileFlags::parse("garbage,cpu,;;;,heap");
        assert!(f.cpu && f.heap);
        assert!(!f.memory && !f.cascade && !f.jsonl && !f.detail);
    }

    // ---- parser: numeric boundaries --------------------------------------

    #[test]
    fn parse_boundary_numeric_strings_are_ignored() {
        for input in [
            "0",
            "-0",
            "+0",
            "1",
            "9223372036854775807",     // i64::MAX
            "-9223372036854775808",    // i64::MIN
            "9223372036854775808",     // i64::MAX + 1
            "18446744073709551615",    // u64::MAX
            "18446744073709551616",    // u64::MAX + 1
            "340282366920938463463374607431768211456",
            "1.7976931348623157e308",  // f64::MAX
            "5e-324",                  // f64 min subnormal
            "1e309",                   // overflows to inf
            "NaN",
            "nan",
            "inf",
            "-inf",
            "infinity",
            "0x7fffffffffffffff",
            "0b1111",
            "1e",
            ".",
            "..",
        ] {
            let f = ProfileFlags::parse(input);
            assert_eq!(
                f,
                ProfileFlags::default(),
                "numeric-ish input {input:?} should set no flags, got {f:?}"
            );
        }
    }

    #[test]
    fn parse_numeric_tokens_mixed_with_valid_tokens_do_not_corrupt_flags() {
        let f = ProfileFlags::parse("NaN,cpu,inf,-0,9223372036854775807,heap,1e309");
        assert!(f.cpu && f.heap);
        assert!(!f.memory && !f.cascade && !f.jsonl && !f.detail);
    }

    // ---- parser: size / nesting limits -----------------------------------

    #[test]
    fn parse_extremely_long_single_token_does_not_panic_or_hang() {
        let huge: String = std::iter::repeat_n('a', 1_000_000).collect();
        assert_eq!(ProfileFlags::parse(&huge), ProfileFlags::default());

        // A 1M-char token that *starts* with a valid token must still not match
        // (exact equality, not prefix matching).
        let mut prefixed = String::from("cpu");
        prefixed.push_str(&huge);
        assert_eq!(ProfileFlags::parse(&prefixed), ProfileFlags::default());
    }

    #[test]
    fn parse_million_separators_does_not_panic_or_hang() {
        let commas: String = std::iter::repeat_n(',', 1_000_000).collect();
        assert_eq!(ProfileFlags::parse(&commas), ProfileFlags::default());

        // 1M empty tokens with one real token buried at the end.
        let mut with_token = commas.clone();
        with_token.push_str("cpu");
        assert!(ProfileFlags::parse(&with_token).cpu);
    }

    #[test]
    fn parse_repeated_token_250k_times_is_idempotent() {
        let repeated = "cpu,".repeat(250_000);
        let f = ProfileFlags::parse(&repeated);
        assert!(f.cpu);
        // Repetition is a union, never a toggle: parsing "cpu" 250k times is
        // the same as parsing it once.
        assert_eq!(f, ProfileFlags::parse("cpu"));
    }

    #[test]
    fn parse_deeply_nested_brackets_does_not_stack_overflow() {
        let depth = 10_000;
        let mut nested = String::new();
        for _ in 0..depth {
            nested.push('[');
        }
        nested.push_str("cpu");
        for _ in 0..depth {
            nested.push(']');
        }
        // Not a recursive-descent grammar: no recursion, no overflow, and the
        // bracket-wrapped token does not match.
        assert_eq!(ProfileFlags::parse(&nested), ProfileFlags::default());

        // Same depth, but comma-separated so every bracket is its own token.
        let nested_csv = "[,".repeat(depth);
        assert_eq!(ProfileFlags::parse(&nested_csv), ProfileFlags::default());
    }

    #[test]
    fn parse_many_distinct_unknown_tokens_does_not_hang() {
        let mut s = String::new();
        for i in 0..100_000u32 {
            s.push_str(&i.to_string());
            s.push(',');
        }
        s.push_str("detail");
        let f = ProfileFlags::parse(&s);
        assert!(f.detail);
        assert!(!f.cpu && !f.memory && !f.cascade && !f.heap && !f.jsonl);
    }

    // ---- parser: unicode --------------------------------------------------

    #[test]
    fn parse_unicode_does_not_panic_and_matches_exactly() {
        // Multibyte junk: never matches, never panics on a char boundary.
        for input in [
            "\u{1F600}",                 // emoji
            "\u{1F600},\u{1F4A9}",
            "cpu\u{301}",                // "cpu" + combining acute -> different token
            "\u{301}cpu",
            "\u{feff}cpu",               // BOM is NOT Unicode White_Space -> not trimmed
            "ｃｐｕ",                     // fullwidth latin
            "СРU",                       // Cyrillic С/Р homoglyphs
            "cpü",
            "ＨＥＡＰ",
            "日本語,中文,한국어",
            "\u{202e}cpu",               // RTL override
            "e\u{301}\u{301}\u{301}",
        ] {
            let f = ProfileFlags::parse(input);
            assert_eq!(
                f,
                ProfileFlags::default(),
                "unicode input {input:?} should set no flags, got {f:?}"
            );
        }
    }

    #[test]
    fn parse_trims_unicode_whitespace_around_ascii_tokens() {
        // `str::trim` uses the Unicode White_Space property, so NBSP / EM SPACE
        // / IDEOGRAPHIC SPACE are stripped just like ASCII spaces.
        assert!(ProfileFlags::parse("\u{a0}cpu\u{a0}").cpu);
        assert!(ProfileFlags::parse("\u{2003}heap\u{2003}").heap);
        assert!(ProfileFlags::parse("\u{3000}jsonl").jsonl);
    }

    #[test]
    fn parse_unicode_mixed_with_valid_tokens_keeps_valid_ones() {
        let f = ProfileFlags::parse("\u{1F600},cpu,日本語,heap,\u{202e}");
        assert!(f.cpu && f.heap);
        assert!(!f.memory && !f.cascade && !f.jsonl && !f.detail);
    }

    // ---- parser: positive controls & invariants ---------------------------

    #[test]
    fn parse_each_canonical_token_sets_exactly_one_flag() {
        for (idx, tok) in CANONICAL.iter().enumerate() {
            let f = ProfileFlags::parse(tok);
            assert!(field(&f, idx), "token {tok:?} did not set its own flag");
            let leaked = (0..CANONICAL.len())
                .filter(|other| *other != idx)
                .filter(|other| field(&f, *other))
                .count();
            assert_eq!(leaked, 0, "token {tok:?} leaked into another flag: {f:?}");
        }
    }

    #[test]
    fn parse_each_alias_is_equivalent_to_its_canonical_token() {
        for (alias, canonical) in ALIASES {
            assert_eq!(
                ProfileFlags::parse(alias),
                ProfileFlags::parse(canonical),
                "alias {alias:?} != canonical {canonical:?}"
            );
        }
    }

    #[test]
    fn parse_is_case_insensitive_for_every_token() {
        for (idx, tok) in CANONICAL.iter().enumerate() {
            for variant in [tok.to_ascii_uppercase(), tok.to_ascii_lowercase()] {
                let f = ProfileFlags::parse(&variant);
                assert!(field(&f, idx), "case variant {variant:?} did not match");
            }
        }
        let all = ProfileFlags::parse("MEMORY,CPU,CaScAdE,HeAp,jSoNl,DETAIL");
        assert_eq!(all, from_mask(0b11_1111));
    }

    #[test]
    fn parse_is_order_independent() {
        let a = ProfileFlags::parse("cpu,heap,detail");
        let b = ProfileFlags::parse("detail,heap,cpu");
        let c = ProfileFlags::parse("heap,detail,cpu");
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn parse_is_monotone_unknown_tokens_never_unset_a_flag() {
        let base = ProfileFlags::parse("cpu,heap");
        for junk in ["bogus", "", "   ", "\u{1F600}", "NaN", "-cpu", "heap;"] {
            let mut with_junk = String::from("cpu,heap,");
            with_junk.push_str(junk);
            let f = ProfileFlags::parse(&with_junk);
            assert!(f.cpu && f.heap, "junk {junk:?} cleared a flag: {f:?}");
            assert_eq!(f, base, "junk {junk:?} changed the flag set");
        }
    }

    #[test]
    fn parse_jsonl_does_not_implicitly_enable_heap() {
        // The docs say `jsonl` "requires heap to do anything" — that dependency
        // is *not* enforced at parse time, and this test pins that down.
        let f = ProfileFlags::parse("jsonl");
        assert!(f.jsonl);
        assert!(!f.heap);

        // Same for `detail`, which layers on top of `heap`.
        let d = ProfileFlags::parse("detail");
        assert!(d.detail);
        assert!(!d.heap);
    }

    // ---- round-trip: encode == decode ------------------------------------

    #[test]
    fn round_trip_all_64_flag_combinations() {
        for mask in 0..64u8 {
            let original = from_mask(mask);
            let encoded = encode(original);
            let decoded = ProfileFlags::parse(&encoded);
            assert_eq!(
                decoded, original,
                "round-trip failed for mask {mask:#08b} (encoded {encoded:?})"
            );
        }
    }

    #[test]
    fn round_trip_survives_whitespace_and_case_mangling() {
        for mask in 0..64u8 {
            let original = from_mask(mask);
            let encoded = encode(original);
            // Re-render as " TOKEN , TOKEN " in upper case with padding.
            let mangled: Vec<String> = encoded
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|t| {
                    let mut s = String::from("  ");
                    s.push_str(&t.to_ascii_uppercase());
                    s.push_str(" \t");
                    s
                })
                .collect();
            let decoded = ProfileFlags::parse(&mangled.join(","));
            assert_eq!(decoded, original, "mangled round-trip failed for {mask:#08b}");
        }
    }

    #[test]
    fn round_trip_is_stable_under_re_encoding() {
        for mask in 0..64u8 {
            let f = from_mask(mask);
            let once = encode(f);
            let twice = encode(ProfileFlags::parse(&once));
            assert_eq!(once, twice, "encode is not a fixed point for {mask:#08b}");
        }
    }

    // ---- deterministic pseudo-random fuzz --------------------------------

    #[test]
    fn parse_fuzz_is_deterministic_and_never_invents_flags() {
        const PIECES: [&str; 24] = [
            "cpu", "CPU", "mem", "memory", "cascade", "css", "heap", "jsonl", "detail", "perf",
            ",", ";", " ", "\t", "\n", "", "x", "0", "NaN", "\u{1F600}", "\u{301}", "\u{a0}", "=",
            "-",
        ];

        // Fixed-seed LCG: no Math.random / wall-clock, fully reproducible.
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as usize
        };

        for _ in 0..2_000 {
            let len = next() % 24;
            let mut input = String::new();
            for _ in 0..len {
                input.push_str(PIECES[next() % PIECES.len()]);
            }

            let f = ProfileFlags::parse(&input);
            // 1. deterministic
            assert_eq!(f, ProfileFlags::parse(&input), "parse is not deterministic");
            // 2. never sets a flag whose token isn't present
            assert_flags_are_justified(&input, &f);
            // 3. a completely token-free input never sets anything
            let lower = input.to_ascii_lowercase();
            if !["memory", "mem", "cpu", "perf", "cascade", "css", "heap", "jsonl", "detail"]
                .iter()
                .any(|t| lower.contains(t))
            {
                assert_eq!(f, ProfileFlags::default(), "flags set for {input:?}");
            }
        }
    }

    // ---- flags() / out_path() / predicates --------------------------------

    #[test]
    fn default_flags_are_all_off() {
        let d = ProfileFlags::default();
        assert!(!any_set(&d));
        assert_eq!(d, ProfileFlags::parse(""));
    }

    #[test]
    fn flags_is_cached_and_stable_across_calls() {
        // NOTE: the env var is deliberately *not* mutated here — `flags()` is a
        // process-wide `OnceLock` and tests run in parallel threads, so any
        // `set_var` would be both racy and useless after first access.
        let first = flags();
        for _ in 0..1_000 {
            assert_eq!(flags(), first, "flags() is not stable across calls");
        }
    }

    #[test]
    fn predicates_agree_with_flags() {
        let f = flags();
        assert_eq!(memory_enabled(), f.memory);
        assert_eq!(cpu_enabled(), f.cpu);
        assert_eq!(cascade_enabled(), f.cascade);
        assert_eq!(heap_enabled(), f.heap);
        assert_eq!(jsonl_enabled(), f.jsonl);
        assert_eq!(detail_enabled(), f.detail);
    }

    #[test]
    fn predicates_are_idempotent() {
        for _ in 0..100 {
            assert_eq!(memory_enabled(), memory_enabled());
            assert_eq!(cpu_enabled(), cpu_enabled());
            assert_eq!(cascade_enabled(), cascade_enabled());
            assert_eq!(heap_enabled(), heap_enabled());
            assert_eq!(jsonl_enabled(), jsonl_enabled());
            assert_eq!(detail_enabled(), detail_enabled());
        }
    }

    #[test]
    fn out_path_does_not_panic_and_is_cached() {
        let first = out_path();
        for _ in 0..100 {
            assert_eq!(out_path(), first, "out_path() is not stable across calls");
        }
        // If a path *is* configured it must be a real (possibly empty) &'static
        // str handed back from the same cached allocation every time.
        if let Some(p) = first {
            assert_eq!(out_path().map(str::as_ptr), Some(p.as_ptr()));
        }
    }

    /// `no_std` builds have no environment: profiling must be hard-off.
    #[cfg(not(feature = "std"))]
    #[test]
    fn nostd_profiling_is_always_off() {
        assert_eq!(flags(), ProfileFlags::default());
        assert_eq!(out_path(), None);
        assert!(!memory_enabled());
        assert!(!cpu_enabled());
        assert!(!cascade_enabled());
        assert!(!heap_enabled());
        assert!(!jsonl_enabled());
        assert!(!detail_enabled());
    }

    /// Under `std`, `flags()` must agree with whatever `AZ_PROFILE` said at
    /// first access — and, crucially, must keep agreeing even if the env var is
    /// later changed by some other part of the process (it is cached forever).
    #[cfg(feature = "std")]
    #[test]
    fn std_flags_match_env_or_default_and_never_change() {
        let observed = flags();
        let expected_now = std::env::var("AZ_PROFILE")
            .map(|v| ProfileFlags::parse(&v))
            .unwrap_or_default();
        // The cache is filled on first access; within one test binary the env
        // var is not mutated, so these must agree.
        assert_eq!(observed, expected_now);
        assert_eq!(flags(), observed);
    }
}
