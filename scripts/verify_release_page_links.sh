#!/usr/bin/env bash
# Fetch every download link the release page advertises and assert it resolves.
#
# WHY: the job that calls this is named "Release page lists the expected
# artifacts", and it passed while 18 of the 71 GitHub-Release download links on
# https://azul.rs/ui/release/0.2.0 returned 404 — fifteen iOS demo bundles CI
# has never produced, and all three azul-writer desktop tarballs. It passed
# because it only ever grepped the page for five NAMES. "The page mentions
# libazul.so" and "a user who clicks Download gets a file" are different
# claims, and only the second one matters.
#
# So: extract the links, and FETCH THEM. A 404 fails the check.
#
# Usage: verify_release_page_links.sh <page-url> [base-url] [version]
#
# Exit 0 = every advertised download resolves. Exit 1 = at least one is dead.
set -uo pipefail

PAGE_URL="${1:?usage: verify_release_page_links.sh <page-url> [base-url] [version]}"
BASE="${2:-https://azul.rs}"
VERSION="${3:-}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PAGE="$WORK/page.html"

curl -sSL --fail "$PAGE_URL" -o "$PAGE" || {
    echo "::error::cannot fetch the release page $PAGE_URL"
    exit 1
}
echo "release page: $PAGE_URL ($(wc -c < "$PAGE") bytes)"

# Both quoting styles: deploy.rs emits href='...', hand-written HTML uses "...".
# Only DOWNLOAD links are checked — navigation, anchors and guide links are the
# site's own problem and are covered by the site's link check.
grep -oE "href=['\"][^'\"]+['\"]" "$PAGE" \
  | sed -E "s/^href=['\"]//; s/['\"]$//" \
  | while IFS= read -r u; do
      case "$u" in
        /*) echo "${BASE}${u}" ;;
        http*) echo "$u" ;;
        *) ;;   # relative/anchor/mailto — not a download link
      esac
    done \
  | grep -E "releases/download/|/ui/release/" \
  | sort -u > "$WORK/links.txt"

total=$(wc -l < "$WORK/links.txt" | tr -d ' ')
if [ "$total" -eq 0 ]; then
    echo "::error::found ZERO download links on $PAGE_URL — the page rendered without its downloads, or the extraction is broken. Refusing to report success on an empty check."
    exit 1
fi
echo "checking $total advertised download link(s)${VERSION:+ for $VERSION}"

probe() {
    url="$1"
    # HEAD first (cheap). Some CDNs answer HEAD with 403 while GET works, so
    # fall back to a 1-byte ranged GET before calling anything dead.
    code=$(curl -sSL -o /dev/null -w '%{http_code}' --max-time 60 --retry 2 \
                --retry-connrefused -I "$url" 2>/dev/null || echo 000)
    case "$code" in
      2*) echo "OK $code $url"; return 0 ;;
    esac
    code=$(curl -sSL -o /dev/null -w '%{http_code}' --max-time 60 --retry 2 \
                --retry-connrefused -r 0-0 "$url" 2>/dev/null || echo 000)
    case "$code" in
      2*) echo "OK $code $url" ;;
      *)  echo "DEAD $code $url" ;;
    esac
}
export -f probe

xargs -P 8 -I{} bash -c 'probe "$@"' _ {} < "$WORK/links.txt" > "$WORK/results.txt"

sort "$WORK/results.txt" | sed 's/^/  /'

dead=$(grep -c '^DEAD' "$WORK/results.txt" || true)
ok=$(grep -c '^OK' "$WORK/results.txt" || true)
echo
echo "=== advertised downloads: $ok resolve, $dead dead (of $total) ==="
if [ "${dead:-0}" -ne 0 ]; then
    grep '^DEAD' "$WORK/results.txt" | while read -r _ code url; do
        echo "::error::$PAGE_URL offers a download that returns HTTP $code: $url"
    done
    echo "::error::$dead advertised download(s) on $PAGE_URL are dead. A user must never be offered a 404 — either publish the artifact, or stop listing it (deploy_pages' 'Remove links to artifacts that were never published' step does the latter automatically, so a dead link here means that step did not run, ran too early, or the artifact appeared and then vanished)."
    exit 1
fi
echo "every advertised download resolves"
