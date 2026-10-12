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

## Updates are automatic

When a stable `v*` tag is released, the `AUR` jobs in `.github/workflows/ci.yml`:

1. set `pkgver` to the tag and `pkgrel=1`,
2. refresh the checksums and build each package in Arch,
3. push `PKGBUILD` and a new `.SRCINFO` to the AUR.

They push with the `AUR_SSH_PRIVATE_KEY` repository secret, a key registered on the
AUR account. Pre-releases (`v1.0.0-rc1`) are skipped.

Change anything else (dependencies, install steps) here first, check it with
`test.sh`, and the next release carries it over. To push an update by hand, run
`bump.sh <version>` and `test.sh`, then copy `PKGBUILD` and `.SRCINFO` over as above.
