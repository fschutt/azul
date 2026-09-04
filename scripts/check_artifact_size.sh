#!/usr/bin/env bash
#
# Fail the build when a shipped artifact outgrows its budget.
#
# Sizes were already reported all over CI and nothing acted on them, so a size
# regression could only be noticed by someone downloading a release. This makes
# the budget explicit (scripts/artifact_size_budgets.txt) and the failure loud,
# and prints every artifact as a percentage of its budget so the numbers stay
# visible even when everything passes.
#
# Usage:
#   scripts/check_artifact_size.sh --target <triple> <file>...
#
# An artifact with no matching budget line is REPORTED, not failed: a new
# artifact should not break CI the day it appears - but its size is printed so
# a budget can be added deliberately.
#
# Escape hatch: AZ_SIZE_GATE=report downgrades every breach to a warning (for
# a branch that is knowingly mid-refactor). It does not silence the output.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUDGETS="${AZ_SIZE_BUDGETS:-${SCRIPT_DIR}/artifact_size_budgets.txt}"
TARGET="*"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target) TARGET="${2:-*}"; shift 2 ;;
        --budgets) BUDGETS="$2"; shift 2 ;;
        --) shift; break ;;
        *) break ;;
    esac
done

if [[ ! -f "${BUDGETS}" ]]; then
    echo "[size-gate] no budget file at ${BUDGETS}" >&2
    exit 1
fi
if [[ $# -eq 0 ]]; then
    echo "[size-gate] no artifacts given (nothing built?) — nothing to check"
    exit 0
fi

# Longest match wins over the `*` fallback: budget lines are read in order and
# the first line whose target AND glob both match is used.
budget_for() {
    local target="$1" base="$2" line btarget bglob bmax
    while read -r btarget bglob bmax _; do
        [[ -z "${btarget:-}" || "${btarget}" == \#* ]] && continue
        [[ "${btarget}" != "*" && "${btarget}" != "${target}" ]] && continue
        # shellcheck disable=SC2053  # glob match is intended
        [[ "${base}" == ${bglob} ]] || continue
        echo "${bmax}"
        return 0
    done < <(grep -vE '^\s*(#|$)' "${BUDGETS}")
    return 1
}

human() { numfmt --to=iec --suffix=B "$1" 2>/dev/null || echo "$1 bytes"; }

FAILED=0
CHECKED=0
UNGATED=0
echo "[size-gate] target=${TARGET} budgets=${BUDGETS}"
for f in "$@"; do
    [[ -f "$f" ]] || continue
    base="$(basename "$f")"
    size="$(wc -c < "$f" | tr -d ' ')"
    CHECKED=$((CHECKED + 1))
    if max="$(budget_for "${TARGET}" "${base}")"; then
        pct=$(( size * 100 / max ))
        if (( size > max )); then
            over=$(( size - max ))
            echo "[size-gate] FAIL ${base}: $(human "${size}") > budget $(human "${max}") (+$(human "${over}"), ${pct}% of budget)"
            FAILED=$((FAILED + 1))
        else
            echo "[size-gate]   ok ${base}: $(human "${size}") of $(human "${max}") (${pct}%)"
        fi
    else
        echo "[size-gate] UNGATED ${base}: $(human "${size}") — no budget line; add one to $(basename "${BUDGETS}")"
        UNGATED=$((UNGATED + 1))
    fi
done

if (( CHECKED == 0 )); then
    echo "[size-gate] none of the given paths existed — nothing to check"
    exit 0
fi
if (( FAILED > 0 )); then
    if [[ "${AZ_SIZE_GATE:-}" == "report" ]]; then
        echo "[size-gate] ${FAILED} over budget, but AZ_SIZE_GATE=report — not failing"
        exit 0
    fi
    echo "[size-gate] ${FAILED} artifact(s) over budget."
    echo "[size-gate] If the growth is intended, raise the number in"
    echo "[size-gate] scripts/artifact_size_budgets.txt in the SAME commit, with a reason."
    exit 1
fi
GATED=$(( CHECKED - UNGATED ))
if (( UNGATED > 0 )); then
    echo "[size-gate] ${GATED} artifact(s) within budget, ${UNGATED} ungated."
else
    echo "[size-gate] all ${GATED} artifact(s) within budget."
fi
exit 0
