#!/usr/bin/env bash
# Reproducible RSS baseline for miniword — the measurement procedure behind
# scripts/RSS_MAP_2026_08_07.md.
#
# WHY THIS EXISTS. The map's numbers are only useful if a later change can be
# measured against them the same way. Every trap that cost a run while building
# it is encoded here:
#
#   * miniword has NO argv handling — the document is opened by MINIWORD_OPEN.
#     `./miniword doc.md` silently measures a BLANK document and still plateaus
#     convincingly. (Cost two heaptrack runs.)
#   * MINIWORD_SHOT ends in process::exit(0), which skips heaptrack's atexit
#     flush and yields an EMPTY profile. Never set it while profiling.
#   * `heaptrack -o out env VAR=x ./app` traces `env`, not the app: exec
#     replaces the image and 0 allocations are flushed. Export the variable.
#   * A fixed sleep is not enough — heaptrack slows startup ~10x, so this waits
#     for an RSS PLATEAU instead of a clock.
#   * LOCALE. Under a comma-decimal locale (de_DE etc.) awk parses "9.57" as
#     9 — the period reads as a thousands separator — so every per-site
#     figure silently truncates. This understated the report's filter totals
#     by 5-35%%. Every awk that parses heaptrack output here is LC_ALL=C.
#   * big.md is 98% duplicate lines. Content-hash caching makes it flatter than
#     real prose, so a caching change measured against it looks ~40% better than
#     it is. `gen` below produces a 100%-distinct corpus; use it.
#
# WARNING: this corpus is NOT the one scripts/RSS_MAP_2026_08_07.md measured.
# The report used a scratchpad doc-uniq.md (640 non-empty lines, 145.4);
# `gen` produces uniq-960.md (786 non-empty, 147.9). Same 960 total lines and
# both 100% distinct, but a different structural mix — the 2.5 MB between them
# is CORPUS, NOT REGRESSION. Baseline and re-measure with the SAME corpus and
# compare those to each other; never compare a run here against the report.
#
# BASELINE, azul 743eb5837, 1280x800 window, CPU backend, this machine.
# Regenerate with `gen` and compare; these are what the map was built against.
#
#    lines  non-empty        VmHWM        [heap]
#      240        197     96 152 kB     41.5 MB
#      960        786    147 860 kB     90.6 MB
#     1920       1571    217 088 kB    164.3 MB
#
#   marginal cost   71.8 then 72.1 kB per line   (flat)
#   fit             RSS(MB) = 78.7 + 0.0721 x lines
#
# The 78.7 MB intercept reproduces across three independently generated
# corpora (78.5 / 78.7 / 79.3), so a change in the INTERCEPT is a change to
# fixed cost and a change in the SLOPE is a change to per-line cost. Read them
# separately — most optimisations move only one.
#
# `heap` baseline on uniq-960.md, same build:
#
#   peak heap        100.19 M      total leaked   87.87 M
#   text3::cache      44.78 M / 81 sites   <- shaped text; THE target
#   compute_document_pagination
#                     19.98 M / 87 sites   <- app-side A4 layout, intentional
#   solver3::sizing   19.67 M / 39 sites   <- intrinsic-width pass
#   ParsedFont        13.06 M / 70 sites   <- decoded font tables
#   LayoutTreeBuilder  4.78 M /  4 sites   <- does NOT grow with text content
#
# These filters OVERLAP by construction (pagination and sizing both reach
# text3; font decode is reached from text3's FontManager). Do NOT add them.
# Track each against its own previous value instead.
#
# Usage:
#   scripts/rss-baseline.sh gen [outdir]     regenerate the corpus (deterministic)
#   scripts/rss-baseline.sh rss  <file.md>   settled RSS + mapping breakdown
#   scripts/rss-baseline.sh heap <file.md>   heaptrack + per-owner filter totals
set -uo pipefail

MINIWORD="${MINIWORD:-$HOME/Development/pdf2html/miniword/target/release/miniword}"
OUTDIR="${2:-${TMPDIR:-/tmp}/azul-rss-corpus}"

die() { echo "error: $*" >&2; exit 1; }

# --- deterministic 100%-distinct corpus -------------------------------------
# Seeded, so the same sizes reproduce byte-for-byte on any machine.
gen() {
  mkdir -p "$OUTDIR" || die "cannot create $OUTDIR"
  python3 - "$OUTDIR" <<'PY'
import random, sys, os
out = sys.argv[1]
# Structural mix mirroring a real markdown document: headings, bullets, prose,
# blank lines. Blank lines shape to nothing, so distinctness is measured over
# NON-EMPTY lines only — all of which are unique here.
PROTO = ['# H', '## H', '### H', '- b', '> q', '', 'p', 'p', 'p', 'p', '']
for n in (240, 480, 960, 1920, 3840):
    random.seed(20260807 + n)
    words = [''.join(random.choice('abcdefghijklmnopqrstuvwxyz')
             for _ in range(random.randint(3, 11))) for _ in range(n * 12)]
    lines, wi = [], 0
    for i in range(n):
        proto = PROTO[i % len(PROTO)]
        if proto == '':
            lines.append('')
            continue
        prefix = proto[:-1]
        target, buf, ln = 32, [], 0
        while ln < target:
            w = words[wi % len(words)]; wi += 1
            buf.append(w); ln += len(w) + 1
        lines.append(prefix + ' '.join(buf))
    p = os.path.join(out, f'uniq-{n}.md')
    open(p, 'w').write('\n'.join(lines))
    ne = [l for l in lines if l.strip()]
    print(f'  {p}  {n} lines, {len(ne)} non-empty, {len(set(ne))} distinct')
PY
}

# --- wait for the process to stop growing, then report ----------------------
# Returns once RSS has been stable for 3 consecutive samples. A plateau proves
# the process stopped changing; it does NOT prove it did the work, so the
# caller must sanity-check the heap figure against expectations.
plateau() {
  local pid=$1 prev=0 stable=0 rss i
  for i in $(seq 1 80); do
    [ -d "/proc/$pid" ] || return 1
    rss=$(awk '/^VmRSS:/{print $2}' "/proc/$pid/status" 2>/dev/null) || return 1
    if [ $((rss - prev)) -lt 250 ] && [ "$i" -gt 3 ]; then
      stable=$((stable + 1))
    else
      stable=0
    fi
    [ "$stable" -ge 3 ] && return 0
    prev=$rss; sleep 3
  done
}

rss_run() {
  local doc=$1
  [ -f "$doc" ] || die "no such document: $doc"
  [ -x "$MINIWORD" ] || die "miniword not built at $MINIWORD (set MINIWORD=)"
  pkill -x miniword 2>/dev/null; sleep 1
  MINIWORD_OPEN="$doc" "$MINIWORD" >/dev/null 2>&1 &
  sleep 10
  local pid; pid=$(pgrep -x miniword | head -1)
  [ -n "$pid" ] || die "miniword did not start"
  plateau "$pid" || die "process exited before settling"
  echo "### $(basename "$doc")"
  awk '/^VmHWM:|^VmRSS:/{printf "  %-8s %s kB\n", $1, $2}' "/proc/$pid/status"
  awk '/^[0-9a-f]+-[0-9a-f]+ /{n=$6; for(j=7;j<=NF;j++) n=n" "$j; if(n=="") n="[anon]"}
       /^Rss:/{r[n]+=$2}
       END{for(x in r) if(r[x]>400) printf "  %8.1f MB  %s\n", r[x]/1024, x}' \
      "/proc/$pid/smaps" | sort -rn
  kill -TERM "$pid" 2>/dev/null
}

# --- heaptrack + the per-owner totals the map is built from -----------------
heap_run() {
  local doc=$1 out; out="${TMPDIR:-/tmp}/rss-baseline-$$"
  [ -f "$doc" ] || die "no such document: $doc"
  command -v heaptrack >/dev/null || die "heaptrack not installed"
  pkill -x miniword 2>/dev/null; sleep 1
  # Export, do NOT wrap in `env` — heaptrack would trace env and flush nothing.
  export MINIWORD_OPEN="$doc"
  unset MINIWORD_SHOT
  heaptrack -o "$out" "$MINIWORD" >/dev/null 2>&1 &
  sleep 20
  local pid; pid=$(pgrep -x miniword | head -1)
  [ -n "$pid" ] || die "miniword did not start under heaptrack"
  plateau "$pid"
  kill -TERM "$pid" 2>/dev/null; sleep 20
  local zst="$out.zst"; [ -f "$zst" ] || die "no profile written"
  echo "### $(basename "$doc") — heaptrack"
  local stats; stats=$(heaptrack_print "$zst" 2>/dev/null \
    | grep -E '^peak heap|^total memory leaked|^calls to alloc')
  echo "$stats"
  # GUARD, not a comment. The traps in the header all fail the SAME way: a
  # profile that exists, exits 0 and reports implausibly little. An empty
  # profile (MINIWORD_SHOT / the `env` wrapper) reports 0B; a plateau that
  # fired early reports a fraction of the real peak. Both look like success.
  # A blank document peaks near 29 M, so anything under 40 M on a real
  # document did not measure what you think it did.
  local peak; peak=$(printf '%s\n' "$stats" | LC_ALL=C awk '/^peak heap/{
      v=$5; u=substr(v,length(v),1); n=substr(v,1,length(v)-1)+0;
      if(u=="G") n*=1024; else if(u=="K") n/=1024; else if(u=="B") n=0;
      printf "%d", n }')
  if [ -z "${peak:-}" ] || [ "$peak" -lt 40 ]; then
    echo "  *** SUSPECT PROFILE: peak heap ${peak:-?} M, expected >40 M." >&2
    echo "  *** A blank document peaks near 29 M. Check: was MINIWORD_SHOT" >&2
    echo "  *** set? was the app wrapped in \`env\`? did the plateau fire" >&2
    echo "  *** early under load? See the traps at the top of this script." >&2
  fi
  for f in azul_layout::text3::cache compute_document_pagination \
           LayoutTreeBuilder solver3::sizing ParsedFont; do
    printf '  %-32s' "$f"
    heaptrack_print --filter-bt-function "$f" -p 0 -a 0 -T 0 -l 1 -n 250 -s 1 \
        --print-flamegraph /dev/null "$zst" 2>/dev/null \
      | grep -E '^[0-9.]+[KMGB]? leaked over [0-9]+ calls from$' \
      | LC_ALL=C awk '{v=$1;u=substr(v,length(v),1);n=substr(v,1,length(v)-1)+0;
              if(u=="M")b=n*1e6; else if(u=="K")b=n*1e3; else b=v+0; s+=b;c++}
             END{printf " %7.2f M / %d sites\n", s/1e6, c}'
  done
  echo "  profile: $zst"
}

case "${1:-}" in
  gen)  gen ;;
  rss)  rss_run "${2:?usage: rss <file.md>}" ;;
  heap) heap_run "${2:?usage: heap <file.md>}" ;;
  *)    sed -n '2,66p' "$0"; exit 1 ;;
esac
