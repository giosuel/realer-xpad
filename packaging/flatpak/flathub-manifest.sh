#!/usr/bin/env bash
# Print the manifest for the Flathub repository: same as ch.giosuel.RealerXpad.yml,
# but building the given release tag instead of the local checkout.
#
#   packaging/flatpak/flathub-manifest.sh v0.1.2 > ../flathub/ch.giosuel.RealerXpad.yml
set -euo pipefail
tag="${1:?usage: flathub-manifest.sh TAG}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
url=https://github.com/giosuel/realer-xpad.git
commit=$(git ls-remote "$url" "refs/tags/$tag^{}" | cut -f1)
[[ -n "$commit" ]] || commit=$(git ls-remote "$url" "refs/tags/$tag" | cut -f1)
[[ -n "$commit" ]] || { echo "error: tag $tag not found on $url" >&2; exit 1; }

awk -v url="$url" -v tag="$tag" -v commit="$commit" '
  /# flathub-manifest.sh swaps/ { next }
  /- type: dir/ {
    indent = substr($0, 1, index($0, "-") - 1)
    print indent "- type: git"
    print indent "  url: " url
    print indent "  tag: " tag
    print indent "  commit: " commit
    getline; getline   # drop path: and skip:
    next
  }
  { print }
' "$here/ch.giosuel.RealerXpad.yml"
