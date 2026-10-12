# AUR packages

| Package | Builds from | Arch |
| --- | --- | --- |
| [`realer-xpad`](https://aur.archlinux.org/packages/realer-xpad) | source of the tagged release | x86_64, aarch64 |
| [`realer-xpad-bin`](https://aur.archlinux.org/packages/realer-xpad-bin) | binary tarball of the GitHub release | x86_64 |

Each directory is a copy of the matching AUR git repository. The files here are the
source of truth: change them here, then copy them over.

## Test

```sh
packaging/aur/test.sh
```

Builds both packages in a clean `archlinux:base-devel` container, runs the unit
tests and `namcap`, installs each package and regenerates `.SRCINFO`. Needs podman
or docker. The one remaining namcap warning (unused `ld-linux-x86-64.so.2`) is a
known false positive for Rust binaries.

## Publish (first time)

Needs an [AUR account](https://aur.archlinux.org/register) with your SSH public key.

```sh
for pkg in realer-xpad realer-xpad-bin; do
  git clone "ssh://aur@aur.archlinux.org/$pkg.git" "/tmp/aur-$pkg"
  cp "packaging/aur/$pkg/PKGBUILD" "packaging/aur/$pkg/.SRCINFO" "/tmp/aur-$pkg/"
  git -C "/tmp/aur-$pkg" add PKGBUILD .SRCINFO
  git -C "/tmp/aur-$pkg" commit -m "Initial import: $pkg 0.1.1"
  git -C "/tmp/aur-$pkg" push origin master
done
```

## Update after a release

```sh
packaging/aur/bump.sh 0.1.2   # new pkgver, pkgrel=1, fresh checksums
packaging/aur/test.sh         # build, lint, regenerate .SRCINFO
```

Commit the result here, then copy `PKGBUILD` and `.SRCINFO` to each AUR repository
and push, as above.
