#!/usr/bin/env bash
# Reject std::time::Instant::now() reaching a WASM-compatible crate ungated.
#
# std's Instant::now() PANICS on wasm32-unknown-unknown. azul-css, azul-core and
# azul-layout all compile to wasm, so any call must sit behind a cfg that
# actually excludes wasm.
#
# This replaces an inline grep that had two holes, both verified against the tree:
#
#   1. It matched only the fully-qualified `std::time::Instant::now()` — 15 hits.
#      The idiomatic `use std::time::Instant;` + `Instant::now()`, and the alias
#      `use std::time::Instant as StdInstant;` + `StdInstant::now()` (core/src/
#      task.rs:32), were INVISIBLE to it. A new un-gated `use std::time::Instant;`
#      would have passed silently.
#
#   2. It accepted ANY `#[cfg` within 30 lines above — `#[cfg(test)]`,
#      `#[cfg(target_os = "windows")]`, `#[cfg(feature = "svg")]` all satisfied
#      it, and none of those excludes wasm.
#
# The naive widening (`\bInstant::now\(\)`) is wrong in the other direction: it
# matches 75 sites, most of which are azul_core::task::Instant — the INJECTABLE
# clock that exists precisely so azul does not assume a real one. Flagging those
# is what would push someone to "fix" the good API. So this is file-aware:
# it resolves what `Instant` means in each file first, and only then looks at
# calls.
#
# ---------------------------------------------------------------------------
# Three further defects were found and fixed here; each had produced a concrete
# wrong answer against this tree.
#
#   3. SIGPIPE + `set -o pipefail` made the check NONDETERMINISTIC. The window
#      scan was `sed -n "$start,$lno p" file | grep -qE ...`; `grep -q` exits at
#      the first match, `sed` then dies of SIGPIPE (141), and `pipefail` promotes
#      that to the PIPELINE's status — so a correctly-gated site read as
#      "not gated". Measured on layout/src/window.rs:6049 (whose
#      `#[cfg(feature = "std")]` is on the line directly above): 295 false
#      negatives in 400 runs. It reported that line as an ERROR in one
#      invocation and not in the next, on an unchanged tree. Every such scan is
#      now `grep -q ... < <(...)`, where the exit status is grep's alone.
#
#   4. Attributes were matched LINE BY LINE, so a multi-line `#[cfg(...)]` was
#      invisible. layout/src/probe.rs:68 sits under
#          #[cfg(all(
#              feature = "probe",
#              not(target_family = "wasm"),
#              not(feature = "web_lift")
#          ))]
#      — about as explicitly wasm-excluding as a cfg gets — and was reported as
#      ungated. Attributes are now flattened onto one logical line (by bracket
#      balance) before anything is matched against them.
#
#   5. A `#[cfg]` on the PARENT `mod` declaration was not resolved.
#      layout/src/e2e/runner.rs is reachable only through
#      `#[cfg(feature = "e2e-server")] pub mod e2e;` (layout/src/lib.rs), so
#      nothing in it can ever be compiled for a build that lacks that feature.
#      The declaring file is now walked up the module chain to the crate root,
#      and a wasm-excluding cfg anywhere on that chain excludes the whole file.
#
# Because of (5) the accepted-cfg test now has to understand FEATURE NAMES, not
# just the literal `std`: `e2e-server = ["std", ...]`. The crate's Cargo.toml is
# parsed and the transitive closure of features that enable `std` is computed —
# following only same-crate entries (a `dep/feature` or `dep:foo` entry enables
# something in a DEPENDENCY and says nothing about this crate's own `std`).
#
# Negation is now handled too. The old regex matched `target_family = "wasm"`
# anywhere in the attribute, which is the OPPOSITE of exclusion when it appears
# positively: `#[cfg(any(not(feature = "probe"), target_family = "wasm", ...))]`
# selects code that runs ONLY on wasm. An attribute counts as wasm-excluding
# only if a wasm predicate appears negated (or a std-implying feature appears
# positively) AND no wasm predicate appears positively.
#
# ---------------------------------------------------------------------------
# SCOPE, stated plainly because it is narrower than the name suggests.
#
# `feature = "std"` is accepted as wasm-excluding. That is the convention the
# tree uses, but it is NOT true of the wasm build this repo's own CI performs.
# .github/workflows/rust.yml builds the wasm target as:
#
#     cargo check --target wasm32-unknown-unknown -p azul-core            # default = ["std"]
#     cargo check --target wasm32-unknown-unknown -p azul-layout \
#         --no-default-features --features "text_layout,svg,xml"          # text_layout = ["std", ...]
#
# Both enable `std` on wasm32. So a site gated ONLY by `#[cfg(feature = "std")]`
# genuinely does compile into that wasm build and would panic if it ran.
# `get_system_time_libstd` in core/src/task.rs already guards itself with
# `not(target_arch = "wasm32")` on top of `feature = "std"` — that is the shape
# that actually holds.
#
# This script therefore prints, on every run, how many sites rest on that weaker
# guarantee. Run with STRICT_WASM_CFG=1 to have those count as failures.
set -uo pipefail

DIRS=("$@")
[ ${#DIRS[@]} -eq 0 ] && DIRS=(css/src core/src layout/src)

# Set to 1 to reject `feature = "std"`-only gating (see SCOPE above).
STRICT_WASM_CFG="${STRICT_WASM_CFG:-0}"

found=0
scanned=0
std_only=0

# ---------------------------------------------------------------------------
# flatten_cfgs FILE
#
# Emits one record per `#[cfg(...)]` / `#![cfg(...)]` attribute:
#   <start_line> <TAB> <end_line> <TAB> <bang|attr> <TAB> <flattened text>
# Multi-line attributes are joined by bracket balance, so the whole predicate is
# available to a single regex. `end_line` is the line the attribute closes on,
# i.e. the line immediately before the item it applies to.
# ---------------------------------------------------------------------------
flatten_cfgs() {
  awk '
    BEGIN { inattr = 0; buf = ""; start = 0; kind = "" }
    {
      if (inattr) {
        buf = buf " " $0
      } else if ($0 ~ /^[[:space:]]*#\[[[:space:]]*cfg[[:space:]]*\(/) {
        buf = $0; start = NR; kind = "attr"; inattr = 1
      } else if ($0 ~ /^[[:space:]]*#!\[[[:space:]]*cfg[[:space:]]*\(/) {
        buf = $0; start = NR; kind = "bang"; inattr = 1
      } else {
        next
      }
      t = buf; opened = gsub(/\[/, "[", t)
      t = buf; closed = gsub(/\]/, "]", t)
      if (opened <= closed) {
        print start "\t" NR "\t" kind "\t" buf
        inattr = 0; buf = ""
      } else if (NR - start > 40) {
        # Unbalanced for 40 lines: this is not an attribute we understand.
        # Drop it rather than swallowing the rest of the file.
        inattr = 0; buf = ""
      }
    }
  ' "$1"
}

# ---------------------------------------------------------------------------
# std_features CARGO_TOML
#
# Feature names whose transitive expansion enables this crate's own `std`.
# Only same-crate entries are followed: `dep:foo` pulls in an optional
# dependency and `foo/bar` enables a feature ON foo, neither of which says
# anything about whether THIS crate ends up with `std`.
# ---------------------------------------------------------------------------
std_features() {
  awk '
    BEGIN { infeat = 0; name = "" }
    /^\[/ { infeat = ($0 ~ /^\[features\]/) ? 1 : 0; next }
    !infeat { next }
    {
      line = $0
      sub(/#.*/, "", line)
      if (name == "") {
        if (line !~ /=/) next
        split(line, kv, "=")
        name = kv[1]
        gsub(/[[:space:]"]/, "", name)
        if (name == "") next
        rest = substr(line, index(line, "=") + 1)
      } else {
        rest = line
      }
      body[name] = body[name] " " rest
      # An array is complete once its brackets balance.
      t = body[name]; o = gsub(/\[/, "[", t)
      t = body[name]; c = gsub(/\]/, "]", t)
      if (o > 0 && o <= c) { name = "" }
    }
    END {
      # Seed: `std` itself.
      implies["std"] = 1
      # Fixed point over same-crate edges.
      changed = 1
      while (changed) {
        changed = 0
        for (f in body) {
          if (f in implies) continue
          n = split(body[f], parts, /[",\[\]]/)
          for (i = 1; i <= n; i++) {
            dep = parts[i]
            gsub(/[[:space:]]/, "", dep)
            if (dep == "" || dep ~ /\// || dep ~ /^dep:/) continue
            if (dep in implies) { implies[f] = 1; changed = 1; break }
          }
        }
      }
      for (f in implies) print f
    }
  ' "$1"
}

# ---------------------------------------------------------------------------
# crate_root FILE  ->  directory containing the nearest Cargo.toml
# ---------------------------------------------------------------------------
crate_root() {
  local d
  d=$(dirname "$1")
  while [ "$d" != "/" ] && [ "$d" != "." ]; do
    [ -f "$d/Cargo.toml" ] && { printf '%s' "$d"; return 0; }
    d=$(dirname "$d")
  done
  return 1
}

# ---------------------------------------------------------------------------
# excludes_wasm "<flattened cfg text>" "<newline-separated std-implying features>"
#
# True when the attribute keeps its item off wasm. See the negation discussion
# in the header: a wasm predicate must appear NEGATED (or a std-implying feature
# positively), and no wasm predicate may appear positively.
#
# On success sets EXCL_KIND to how it concluded that:
#   wasm — a negated wasm predicate. Holds unconditionally.
#   std  — a std-implying feature only. Does NOT hold for this repo's own wasm
#          CI job (see SCOPE); rejected outright under STRICT_WASM_CFG=1.
# ---------------------------------------------------------------------------
EXCL_KIND=""
excludes_wasm() {
  local text="$1" feats="$2" bare stripped prev feat
  EXCL_KIND=""
  # Collapse whitespace so `feature = "std"` and `feature="std"` are one shape.
  bare=$(printf '%s' "$text" | tr -d '[:space:]')

  # `not(<atom>)` groups removed -> what is left is the POSITIVE part.
  stripped="$bare"
  while :; do
    prev="$stripped"
    stripped=$(printf '%s' "$stripped" | sed 's/not([^()]*)//g')
    [ "$stripped" = "$prev" ] && break
  done

  # A positively-selected wasm predicate means "wasm ONLY" — never an exclusion.
  case "$stripped" in
    *'target_family="wasm"'*|*'target_arch="wasm'*) return 1 ;;
  esac

  # Negated wasm predicate: the item is compiled everywhere except wasm.
  case "$bare" in
    *'not(target_family="wasm")'*|*'not(target_arch="wasm'*) EXCL_KIND=wasm; return 0 ;;
  esac

  # A std-implying feature, positively required. This is the weak form.
  [ "$STRICT_WASM_CFG" = "1" ] && return 1
  if [ -n "$feats" ]; then
    while IFS= read -r feat; do
      [ -z "$feat" ] && continue
      case "$stripped" in
        *"feature=\"$feat\""*) EXCL_KIND=std; return 0 ;;
      esac
    done <<EOF
$feats
EOF
  fi

  return 1
}

# ---------------------------------------------------------------------------
# module_chain_excluded FILE FEATS
#
# Walks FILE up its module chain (`b.rs` <- `a/mod.rs` <- `lib.rs`, etc.) and
# succeeds if any `mod <name>;` declaration on the way carries a wasm-excluding
# cfg. This is the fix for defect (5).
# ---------------------------------------------------------------------------
MOD_CHAIN_KIND=""
module_chain_excluded() {
  local file="$1" feats="$2" root="$3"
  local cur="$file" base dir modname declarer decl_line hop
  MOD_CHAIN_KIND=""

  for hop in 1 2 3 4 5 6 7 8; do
    base=$(basename "$cur")
    dir=$(dirname "$cur")

    # Crate roots declare nothing above themselves.
    case "$base" in
      lib.rs|main.rs) return 1 ;;
    esac

    if [ "$base" = "mod.rs" ]; then
      modname=$(basename "$dir")
      dir=$(dirname "$dir")
    else
      modname="${base%.rs}"
    fi

    # The declaring file is the parent dir's mod.rs / lib.rs, or `<dir>.rs`.
    declarer=""
    for cand in "$dir/mod.rs" "$dir/lib.rs" "$dir/main.rs" "$dir.rs"; do
      [ -f "$cand" ] && { declarer="$cand"; break; }
    done
    [ -n "$declarer" ] || return 1
    [ "$declarer" = "$cur" ] && return 1

    # Line of `mod <modname>;` / `pub mod <modname>;` (possibly `pub(crate)`).
    decl_line=$(grep -nE "^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?mod[[:space:]]+${modname}[[:space:]]*;" "$declarer" | head -1 | cut -d: -f1)

    if [ -n "$decl_line" ] && mod_decl_excluded "$declarer" "$decl_line" "$feats"; then
      MOD_CHAIN_KIND="$EXCL_KIND"
      return 0
    fi

    cur="$declarer"
    [ "$(dirname "$cur")" = "$root/src" ] || true
  done
  return 1
}

# ---------------------------------------------------------------------------
# mod_decl_excluded DECLARER DECL_LINE FEATS
#
# True when the attribute block directly above `mod x;` excludes wasm. The block
# may contain doc comments and other attributes, and any attribute in it may
# span several lines — so candidates come from flatten_cfgs (which knows the
# real extent) rather than from a fixed line window.
# ---------------------------------------------------------------------------
mod_decl_excluded() {
  local declarer="$1" decl_line="$2" feats="$3"
  local top line trimmed start end kind text

  # Walk up over the contiguous run of comments / attribute lines.
  top=$decl_line
  while [ "$top" -gt 1 ]; do
    line=$(sed -n "$((top - 1))p" "$declarer")
    trimmed=$(printf '%s' "$line" | sed 's/^[[:space:]]*//')
    case "$trimmed" in
      '#['*|'#!['*|'//'*|')'*|']'*|'#'*) top=$((top - 1)) ;;
      *)
        # Also step over interior lines of a multi-line attribute.
        if printf '%s' "$trimmed" | grep -qE '^[A-Za-z_(]+[A-Za-z0-9_]*[[:space:]]*(=|\()' \
           && [ "$((decl_line - top))" -lt 20 ]; then
          top=$((top - 1))
        else
          break
        fi
        ;;
    esac
  done
  [ "$top" -ge "$decl_line" ] && return 1

  while IFS=$'\t' read -r start end kind text; do
    [ "$kind" = "attr" ] || continue
    [ "$end" -lt "$top" ] && continue
    [ "$end" -ge "$decl_line" ] && continue
    if excludes_wasm "$text" "$feats"; then
      return 0
    fi
  done < <(flatten_cfgs "$declarer")

  return 1
}

for dir in "${DIRS[@]}"; do
  [ -d "$dir" ] || { echo "ERROR: no such directory: $dir" >&2; exit 2; }
  while IFS= read -r file; do
    # What does `Instant` refer to in THIS file? Only std's counts.
    names=""
    # `use std::time::Instant as X;` binds X, NOT Instant — in such a file a bare
    # `Instant` is something else entirely (in core/src/task.rs it is azul's OWN
    # injectable Instant, which is the API we WANT people using). Flagging it
    # would push someone to "fix" the good type.
    alias=$(sed -nE 's/^[[:space:]]*use[[:space:]]+std::time::Instant[[:space:]]+as[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*;.*/\1/p' "$file" | head -1)
    if [ -n "$alias" ]; then
      names="$alias"
    elif grep -qE '^[[:space:]]*use[[:space:]]+std::time::(\{[^}]*\b)?Instant\b[[:space:]]*(,[^;]*)?;' "$file"; then
      names="Instant"
    fi
    # The fully-qualified form needs no import.
    pattern='std::time::Instant::now\(\)'
    # `\b` is NOT enough for the bare form: in `azul_core::task::Instant::now()`
    # the boundary before `Instant` is satisfied by the preceding `:`, so the
    # imported-name pattern matched the very abstraction this check tells you to
    # switch TO. Require the name to not be preceded by a path separator.
    [ -n "$names" ] && pattern="$pattern|(^|[^:[:alnum:]_])($names)::now\\(\\)"

    grep -qE "$pattern" "$file" || continue

    root=$(crate_root "$file") || root=""
    feats=""
    if [ -n "$root" ] && [ -f "$root/Cargo.toml" ]; then
      feats=$(std_features "$root/Cargo.toml")
    fi

    # Whole-file exclusions: a module-level `#![cfg(...)]`, or a `#[cfg]` on any
    # `mod` declaration on the chain from the crate root down to this file.
    cfgs=$(flatten_cfgs "$file")
    file_excluded=0
    file_excluded_kind=""
    while IFS=$'\t' read -r start end kind text; do
      [ "$kind" = "bang" ] || continue
      [ "$start" -le 40 ] || continue
      if excludes_wasm "$text" "$feats"; then
        file_excluded=1; file_excluded_kind="$EXCL_KIND"; break
      fi
    done < <(printf '%s\n' "$cfgs")

    if [ "$file_excluded" -eq 0 ] && module_chain_excluded "$file" "$feats" "$root"; then
      file_excluded=1
      file_excluded_kind="$MOD_CHAIN_KIND"
    fi

    # A file kept off wasm by a real wasm predicate needs no further scrutiny.
    # One kept off only by a std-implying feature still gets its sites COUNTED,
    # because that guarantee does not hold for this repo's wasm build.
    if [ "$file_excluded" -eq 1 ] && [ "$file_excluded_kind" != "std" ]; then
      continue
    fi

    while IFS= read -r hit; do
      lno=${hit%%:*}
      code=${hit#*:}
      trimmed=$(printf '%s' "$code" | sed 's/^[[:space:]]*//')
      case "$trimmed" in
        //*|/\**|\**) continue ;;   # comment
      esac
      scanned=$((scanned + 1))
      start_win=$(( lno > 120 ? lno - 120 : 1 ))

      gated=0
      kind_used=""
      if [ "$file_excluded" -eq 1 ]; then
        gated=1
        kind_used="$file_excluded_kind"
      else
        while IFS=$'\t' read -r astart aend akind atext; do
          [ "$akind" = "attr" ] || continue
          [ "$aend" -lt "$start_win" ] && continue
          [ "$aend" -ge "$lno" ] && continue
          if excludes_wasm "$atext" "$feats"; then
            gated=1
            kind_used="$EXCL_KIND"
            break
          fi
        done < <(printf '%s\n' "$cfgs")
      fi

      if [ "$gated" -eq 1 ]; then
        [ "$kind_used" = "std" ] && std_only=$((std_only + 1))
        continue
      fi

      echo "ERROR: std Instant::now() not gated away from wasm at ${file}:${lno}"
      echo "    $trimmed"
      found=1
    done < <(grep -nE "$pattern" "$file")
  done < <(find "$dir" -name '*.rs' -type f)
done

if [ "$found" -eq 1 ]; then
  cat >&2 <<'MSG'

std::time::Instant::now() PANICS on wasm32-unknown-unknown.

Either gate the call with a cfg that excludes wasm — #[cfg(not(target_arch =
"wasm32"))] or #[cfg(not(target_family = "wasm"))] — or, better, use
azul_core::task::Instant, which exists exactly so azul does not assume a real
clock is present.

Note that #[cfg(feature = "std")] ALONE does not exclude wasm in this repo: the
wasm CI job builds azul-core with its default features (which include std) and
azul-layout with `text_layout` (which enables std). See SCOPE in this script.
MSG
  exit 1
fi

echo "OK: $scanned std-Instant call site(s) checked, all gated away from wasm."
if [ "$std_only" -gt 0 ]; then
  echo "NOTE: $std_only of them rest on \`feature = \"std\"\` alone, which the wasm"
  echo "      CI job DOES enable (see SCOPE in scripts/check_wasm_instant.sh)."
  echo "      Re-run with STRICT_WASM_CFG=1 to list them."
fi
