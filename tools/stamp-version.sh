#!/usr/bin/env bash
# Stamp a version into the checkout: Cargo.toml, Cargo.lock and, for releases,
# a <release> entry in the AppStream metainfo. The repository itself stays at
# version 0.0.0; the real version comes from the git tag (see version.sh).
#
#   tools/stamp-version.sh 0.2.0 [NOTES_FILE]
#
# NOTES_FILE holds the release notes as written in the tag's message: a
# paragraph, then "- " lines for a list. Pre-release and dev versions
# (anything with a "-"), and 0.0.0, get no metainfo entry.
set -euo pipefail
version="${1:?usage: stamp-version.sh VERSION [NOTES_FILE]}"
notes="${2:-}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
metainfo="$root/extras/ch.giosuel.RealerXpad.metainfo.xml"

# The package's own version only: the first `version =` in Cargo.toml, and the
# line after `name = "realer-xpad"` in Cargo.lock.
sed -i "0,/^version = \".*\"/s//version = \"$version\"/" "$root/Cargo.toml"
sed -i "/^name = \"realer-xpad\"$/{n;s/^version = \".*\"/version = \"$version\"/}" "$root/Cargo.lock"

if [[ "$version" != *-* && "$version" != 0.0.0 ]] && ! grep -q "<release version=\"$version\"" "$metainfo"; then
  entry=$(mktemp)
  {
    echo "    <release version=\"$version\" date=\"$(date -u +%F)\">"
    if [[ -n "$notes" && -s "$notes" ]]; then
      echo "      <description>"
      sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' "$notes" | awk '
        function close_p() { if (p != "") { print "        <p>" p "</p>"; p = "" } }
        function close_ul() { if (ul) { print "        </ul>"; ul = 0 } }
        /^-----BEGIN PGP SIGNATURE-----/ { exit }
        /^[-*] / { close_p(); if (!ul) { print "        <ul>"; ul = 1 }
                   print "          <li>" substr($0, 3) "</li>"; next }
        /^[[:space:]]*$/ { close_p(); close_ul(); next }
        { close_ul(); p = (p == "" ? $0 : p " " $0) }
        END { close_p(); close_ul() }'
      echo "      </description>"
    fi
    echo "    </release>"
  } > "$entry"
  sed -i "/<releases>/r $entry" "$metainfo"
  rm -f "$entry"
fi

grep -m1 '^version' "$root/Cargo.toml"
