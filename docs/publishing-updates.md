# Publishing Darah updates from GitHub

GitHub Releases holds the public downloads and corresponding source. GitHub
Pages serves a separate, signed TUF repository and signed APT archive. The
site can use a GitHub Pages address or a Darah-controlled domain. Once that URL
ships in a binary, keep it live across repository or hosting changes.

Darah checks on startup and daily while open. **Help → Check for Updates**
also works when automatic checks are off. AppImage downloads require consent.
Installing an AppImage update uses the regular unsaved-work prompt, stages on
the same filesystem, and keeps `<file>.previous` for manual recovery. A
non-writable location or filesystem that cannot create the backup falls back
to downloading the release from the website. APT updates stay under the system
package manager and its upgrade policy.

## One-time setup

1. Create the public source repository and a separate `updates` branch for the
   generated site. In **Settings → Pages**, select **Deploy from a branch**,
   then `updates` and `/ (root)`. The generated tree contains `.nojekyll`.
2. On the release machine, install Python 3, GnuPG, `gpgv` and `dpkg-dev`. Install the
   publishing dependencies in a virtual environment outside the source tree.
3. Generate TUF keys and choose a dedicated APT signing key in your local GPG
   keyring. Keep the private keys outside Git and back them up securely. The
   publisher creates distinct root, targets, snapshot and timestamp keys.
   Store `root.pem` offline after initialization. The public root is the only
   TUF key file that belongs in the Darah source tree.

```sh
python3 -m venv ../darah-publisher
../darah-publisher/bin/pip install -r scripts/update-requirements.txt
../darah-publisher/bin/python scripts/publish-updates.py init ../darah-update-keys
cp ../darah-update-keys/root.json packaging/update-root.json
export DARAH_UPDATE_BASE_URL=https://OWNER.github.io/REPO/
gpg --list-secret-keys --with-fingerprint
```

Create a signing key with `gpg --full-generate-key` if you do not have a
dedicated APT key. Record its **full fingerprint** for the publisher and the
release notes. Commit the public TUF root and `Cargo.lock`, and protect the
source and deployment branches. Set `DARAH_UPDATE_BASE_URL` in each release
build environment. Without a valid HTTPS URL and embedded public root, Darah
keeps update checks disabled. Test the first signed feed on a private test
branch/site or disposable local repository before building public binaries
with the production trust root. Use a separate `XDG_STATE_HOME` for test builds
so their update history stays separate.

## Publish a release

1. Build both installers from the same source tag. `scripts/build-deb.sh` and
   `scripts/build-appimage.sh` identify their package format to Darah at build
   time. For every architecture you ship, put both packages in one artifact
   folder. Name each AppImage
   `darah-eda-VERSION-linux-x86_64.AppImage` or
   `darah-eda-VERSION-linux-aarch64.AppImage`. The publisher reads Debian
   package versions and architectures from the packages themselves. Put release
   notes outside the artifact folder.
2. Upload the installers and matching source to GitHub Releases under the
   `vVERSION` tag. Include the APT signing-key fingerprint so users can check
   the bootstrap key against the release. Keep previous releases downloadable.
3. Build and review the signed Pages tree. Replace the Pages branch contents
   with that complete generated tree, keeping its Git metadata, and push one
   commit. Publish the site after the matching GitHub Release is available.

```sh
../darah-publisher/bin/python scripts/publish-updates.py publish \
  --keys ../darah-update-keys --apt-key YOUR_FULL_GPG_FINGERPRINT \
  --base "$DARAH_UPDATE_BASE_URL" --github OWNER/REPO \
  --version 0.7.0 --notes ../release-notes.txt --artifacts ../release-assets \
  --output ../darah-pages-next
```

For every release after the first, add `--previous ../darah-pages-current`
pointing to the currently deployed signed tree. Metadata versions must advance.
The publisher verifies the prior signed metadata and builds the new site in a
fresh directory. Never reset the version counters for an existing feed. The
branch workflow is limited to 100 MiB per file and the publisher caps the
whole tree below the GitHub Pages site limit.

Debian users can download a standalone `.deb` or explicitly follow the
generated `apt/` setup page to enroll. The initial key must match the
fingerprint published through a trusted channel. Enrolled systems use APT
with a repository-specific `Signed-By` key. Darah does not change system
unattended-upgrades settings.

## Keep the feed fresh

Refresh and republish at least weekly, even without a new app release. Both the
TUF timestamp and APT index expire after 14 days. The publisher uses the three
non-root TUF keys and the APT key for a refresh. The refresh keeps the existing
signed target hashes and APT package indexes, while incrementing metadata
versions and extending expiry.

```sh
../darah-publisher/bin/python scripts/publish-updates.py refresh \
  --keys ../darah-update-keys --apt-key YOUR_FULL_GPG_FINGERPRINT \
  --base "$DARAH_UPDATE_BASE_URL" --github OWNER/REPO \
  --previous ../darah-pages-current --output ../darah-pages-next
```

Retain the latest trusted site tree locally. Plan the TUF root rotation before
its three-year expiry and test the signed root transition with an older binary.
For an APT key rotation, distribute the replacement key to enrolled clients
while the old key is still trusted. These are operator procedures. The publisher
does not automatically rotate keys or push to GitHub.

## Verify before enabling public updates

Test a real AppImage upgrade from one built release to the next. Include a
read-only install, a corrupt or interrupted download, an unsaved project and
two running copies. Confirm the updated build can reopen the project and
`<file>.previous` remains launchable. Test APT enrollment and an upgrade from
an existing Debian package on each supported distro. Confirm both packages
open the correct updater UI and installed projects stay in place.

The optional Rust feed test checks a publisher-generated local repository
and deliberately corrupts its AppImage target. Run it with the absolute
path to the generated site and an AppImage target for the test machine's CPU.

```sh
DARAH_TEST_UPDATE_SITE=/absolute/path/to/generated/site \
  cargo +stable test verified_feed_accepts_image_and_rejects_tampering -- --ignored
```
