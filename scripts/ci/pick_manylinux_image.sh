#!/usr/bin/env bash
# Pick a registry that serves the manylinux image cibuildwheel builds wheels in,
# and export it as CIBW_MANYLINUX_X86_64_IMAGE for the following steps.
#
# cibuildwheel creates its container with `docker create --pull=always`, so it
# asks the image's registry for the manifest on every build, even when the image
# is already on disk. A quay.io outage (HTTP 502 on that request, 2026-09-15)
# therefore failed the whole release. This tries quay.io a few times, then a copy
# of the same image on ghcr.io, and refreshes that copy whenever quay.io is up so
# it exists the next time quay.io is not.
#
# Usage:  pick_manylinux_image.sh <manylinux tag>
# Env:    GITHUB_ENV, GITHUB_REPOSITORY_OWNER, GITHUB_ACTOR (set by Actions)
#         GH_TOKEN  a token with packages:write, to read and refresh the mirror
set -uo pipefail

tag="${1:?usage: pick_manylinux_image.sh <manylinux tag>}"
name=manylinux2014_x86_64
primary="quay.io/pypa/$name:$tag"
owner=$(printf '%s' "${GITHUB_REPOSITORY_OWNER:?}" | tr '[:upper:]' '[:lower:]')
mirror="ghcr.io/$owner/$name:$tag"

use() {
    echo "CIBW_MANYLINUX_X86_64_IMAGE=$1" >> "${GITHUB_ENV:?}"
    echo "manylinux image: $1"
}

logged_in=0
if [ -n "${GH_TOKEN:-}" ] &&
    printf '%s' "$GH_TOKEN" | docker login ghcr.io -u "${GITHUB_ACTOR:?}" --password-stdin >/dev/null 2>&1; then
    logged_in=1
else
    echo "::warning::not logged in to ghcr.io; the manylinux mirror can be neither used nor refreshed"
fi

for attempt in 1 2 3; do
    if docker pull --quiet "$primary"; then
        use "$primary"
        if [ "$logged_in" = 1 ] && ! docker manifest inspect "$mirror" >/dev/null 2>&1; then
            if docker tag "$primary" "$mirror" && docker push --quiet "$mirror"; then
                echo "mirrored $primary to $mirror"
            else
                echo "::warning::could not refresh the manylinux mirror $mirror"
            fi
        fi
        exit 0
    fi
    echo "pulling $primary failed (attempt $attempt of 3)"
    sleep $((attempt * 15))
done

if [ "$logged_in" = 1 ] && docker pull --quiet "$mirror"; then
    echo "::warning::quay.io is unreachable; building the wheel in the mirror $mirror"
    use "$mirror"
    exit 0
fi

echo "::error::neither quay.io nor the ghcr.io mirror serve $name:$tag"
exit 1
