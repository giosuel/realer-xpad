#!/usr/bin/env bash
# Print the version of the current checkout, from git tags (like MinVer):
# - on a tag vX.Y.Z (or vX.Y.Z-rc.1): that version;
# - otherwise: the next patch after the last tag, as a dev version,
#   e.g. 0.1.8-dev.5 for 5 commits after v0.1.7.
# Needs the tags in the checkout (actions/checkout with fetch-depth: 0).
set -euo pipefail
if tag=$(git describe --tags --exact-match --match 'v[0-9]*' 2>/dev/null); then
  echo "${tag#v}"
  exit
fi
last=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || echo v0.0.0)
base=${last#v}
IFS=. read -r major minor patch <<<"${base%%-*}"
# After a pre-release (v0.2.0-rc.1) the next version is still 0.2.0.
[[ "$base" == *-* ]] || patch=$((patch + 1))
count=$(git rev-list --count HEAD)
[[ "$last" == v0.0.0 ]] || count=$(git rev-list --count "$last..HEAD")
echo "$major.$minor.$patch-dev.$count"
