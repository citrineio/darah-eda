# Building and publishing releases (maintainers)

This guide is for contributors building distributable packages and maintainers publishing official releases. To install or run Darah, use the [installation guide](installation.md).

## Build packages from a source checkout

A clone includes the release scripts; a globally installed `darah-develop` command is not required.

1. Complete the Rust and Linux build prerequisites in [Compile from source](installation.md#compile-from-source).
2. From the repository root, run:

   ```bash
   python3 scripts/release.py build
   ```

The script tests the application and produces both AppImage and Flatpak in `target/dist/VERSION/`. It checks native dependency resolution and PNG/PDF rendering. Flatpak setup installs missing platform, SDK, and Rust extension dependencies; the initial setup may download several GB.

Building packages does not publish them and does not need release signing keys. Run builds as your normal user.

## Publish an official release

Official publishing requires maintainer access. It is not part of application installation.

Before publishing, have:

- Git and the GitHub CLI (`gh`) installed and authenticated for repository writes.
- The current source checkout, with deliberate changes ready for release.
- The existing online TUF signing keys matching `packaging/update-root.json`, stored outside the checkout.
- Access to the repository's release assets and signed update-feed branch.

From the repository root:

```bash
python3 scripts/publish.py
```

If existing keys cannot be discovered automatically, provide their directory:

```bash
python3 scripts/publish.py --keys /path/to/existing/darah-update-keys
```

The publisher prepares its Python signing environment, chooses the release version, commits the release changes, builds both packages, uploads and checksum-verifies both assets, and publishes the signed update feed. A new GitHub release remains a draft until both uploaded assets pass verification. Review the changes and release notes before running it.

Publishing never creates replacement signing keys. Keep the offline root signing key separate; it is not needed for routine releases. Private key material must stay outside the repository.

## Optional command on the maintainer's workstation

The maintainer's separately installed `darah-develop` command wraps these workflows:

```bash
darah-develop --build
darah-develop --publish
darah-develop --help
```

This workstation tool is optional. It is not an application dependency and is not assumed to be installed on a contributor's machine. The repository scripts above remain the commands available from a fresh clone.

Flatpak bundles are published as GitHub release downloads. Installing a bundle, maintaining a Flatpak app remote, and submitting to Flathub are separate workflows; see [Flatpak installation and updates](installation.md#flatpak).
