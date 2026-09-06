# Jig distribution status

This page distinguishes upstream release downloads, CI build coverage, and
native package-manager availability. A successful build on one Linux
distribution does not establish support for every distribution or desktop.

## Upstream packages

| Platform or channel | Format | Version source | Validation |
| --- | --- | --- | --- |
| Linux x86-64 | AppImage | [Published releases](https://github.com/guicybercode/Jig/releases) | Packaging CI uses Ubuntu 24.04 |
| macOS Apple Silicon | DMG and application ZIP | [Published releases](https://github.com/guicybercode/Jig/releases) | Packaging CI uses macOS 15 |
| Development candidates | The same unsigned formats | [Packaging artifacts](https://github.com/guicybercode/Jig/actions/workflows/packaging.yml) | Check the exact commit and both job results |
| macOS Intel / Linux ARM64 | No official prebuilt package | Not built by the current workflow | Do not infer support from another architecture |
| Windows | None | Outside this Beta's scope | Not tested |

The source tree targets `0.2.0`. A candidate artifact or draft release is not
a published release. Use the release page for the actual public version and
the [release checklist](RELEASE_CHECKLIST.md) for remaining acceptance work.
All current packages are unsigned; macOS notarization is not configured.

## Distribution repositories

The project does not currently publish its own packages to the channels below.
This is a statement about this repository's distribution workflow, not a claim
that nobody has made an independent community package with a similar name.

| Distribution or package manager | Official Jig package from this repository |
| --- | --- |
| Debian / Ubuntu APT | Not published; use the upstream AppImage |
| Fedora RPM / DNF | Not published |
| openSUSE RPM / Zypper | Not published |
| Arch Linux / AUR | Not published |
| Alpine APK | Not published; musl compatibility is not established |
| Nix / NixOS | Not published |
| Flatpak / Flathub | Not published |
| Snap | Not published |
| Homebrew Cask | Not published; use the upstream DMG or application ZIP |
| MacPorts | Not published |

See [installation](install.md) for prerequisites and verification. Do not
assume that `apt install jig`, `brew install jig`, or a similarly named
package installs this application.

## Repology version table

[Repology](https://github.com/repology/repology-rs) tracks package versions
across repositories. Its distribution table, as used by
[kitty](https://github.com/kovidgoyal/kitty), describes packages actually
indexed for that project; it does not create or publish those packages.

An indexed Repology identity for **guicybercode/Jig** has not been verified.
The README therefore uses the explicit availability table above instead of
embedding an unverified `jig` badge or copying another application's versions.

Before enabling a live Repology table:

1. Publish and maintain a real package in a repository indexed by Repology.
2. Confirm that the resulting project entry links back to
   `https://github.com/guicybercode/Jig`, not a different project named Jig.
3. Copy that verified entry's badge from its **Badges** page and link it to
   the corresponding **Versions** page.
4. Keep this page's package-manager commands and maintainer links current.

Packaging contributions are welcome through
[CONTRIBUTING.md](../CONTRIBUTING.md). They need version-pinned sources,
checksums, the MIT license notice, and validation on the target platform.
