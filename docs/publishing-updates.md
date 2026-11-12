# Build and publish Darah AppImage and Flatpak

Use the installed command:

    darah-develop --build
    darah-develop --publish
    darah-develop --help

Both commands build AppImage and Flatpak together. The release command tests
locally. The publish command handles the version, commits, build reuse,
both GitHub assets, and signed update feed. It locates
the existing signing keys and creates its Python environment automatically.
Use --publish directly when the goal is to publish; a separate build is optional.
Both artifacts must pass their checks before the GitHub release becomes public.
Flatpak setup adds the user Flathub remote and installs missing manifest runtime,
SDK, and Rust extension automatically. The first setup may download several GB.
Both artifacts are written to target/dist/VERSION/ and verified builds are reused.

The command detects and remembers the project. Use --project PATH to select one
explicitly, --release-version 0.7.4 to override the version, or --keys PATH to
import existing signing keys once. --status shows the selected project.
Private signing keys must remain outside the source checkout. These scripts
never create replacement signing keys. All tools, backups, and build state live
under target/. Older helper and patch scripts are no longer used.
