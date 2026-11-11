# Publishing Darah updates

GitHub Releases holds the AppImage and Flatpak downloads and matching source.
GitHub Pages serves the signed TUF feed for AppImage. Keep the feed URL live
once a published AppImage embeds it. Flatpak builds hide Darah’s self-updater.
Flatpak manages upgrades for installations from a configured remote. Install
a newer bundle manually for a standalone bundle without an update remote.

## Build and qualify the release

Use the committed source version on `main`. Install current stable Rust,
Python 3.11+, Flatpak, Flatpak Builder and the appropriate linuxdeploy AppImage.
The release script reads linuxdeploy from `LINUXDEPLOY`, the source folder, its
parent, or the executable search path.

```sh
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
python3 scripts/release.py build
```

- Launch a previously published AppImage and upgrade it through the signed
  feed. The progress display is provided by the version you start from, so
  exercise the new updater by starting from 0.7.2 or later. Check the default
  filename behavior and the opt-in rename separately. Confirm the compiled
  version in About after restart and reopen an existing project. Verify that
  the `.previous` backup is launchable.
- Install the Flatpak bundle and open an existing project through the file
  chooser. Edit it, save in place, relaunch and reopen it. Export a diagram
  and a power review to PDF outside the sandbox through the Save dialog.
  Confirm that the Help menu has no Darah update action. Run the same checks
  against a remote installation before advertising automatic Flatpak upgrades.

For an AppImage release before Flatpak is ready, use
`python3 scripts/release.py build --appimage-only` and
`python3 scripts/release.py publish --appimage-only`. This mode keeps the
existing APT landing page and packages available while publishing the new
AppImage feed. It stores prepared artifacts separately so a retry uses the
same package selection.

## Publish and refresh

The release helper expects the existing TUF keys and publisher virtual
environment next to the source checkout. Set `DARAH_UPDATE_KEYS` and
`DARAH_PUBLISHER_PYTHON` if you keep them elsewhere. Keep the private keys
outside the checkout and the root signing key offline. An already configured
feed must retain its root and metadata history.

```sh
python3 scripts/release.py publish --notes ../release-notes.md
python3 scripts/release.py refresh
```

The helper builds and checksums both artifacts before it publishes the GitHub
release or deploys the updated feed. The Pages build verifies the previous
TUF metadata and advances its version counters. It signs only AppImages as
application targets. The former APT landing page becomes a migration page on
publish. Existing release tags and the last APT archive remain available.
Retired APT indexes expire on their original schedule, so enrolled users
should remove the Darah source when they migrate.

Refresh and deploy the TUF feed at least weekly. The timestamp expires after
14 days. Plan root rotation before expiry and test it with an older binary.
The publisher supports manual builds with
`python scripts/publish-updates.py publish --help` and
`python scripts/publish-updates.py refresh --help`. Always pass the last
deployed tree as `--previous` for an existing feed. Only a new feed should
generate a fresh trust root with `scripts/publish-updates.py init`.

To exercise signature and target verification against a local fixture:

```sh
DARAH_TEST_UPDATE_SITE=/absolute/path/to/generated/site \
  cargo +stable test verified_feed_accepts_image_and_rejects_tampering -- --ignored
```
