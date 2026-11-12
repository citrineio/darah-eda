# Build and publish Darah AppImages

Use the installed command:

    darah-develop --build
    darah-develop --publish
    darah-develop --help

The release command tests and builds locally. The publish command handles the
version, commits, build reuse, GitHub release, and signed update feed. It locates
the existing signing keys and creates its Python environment automatically.
Use --publish directly when the goal is to publish; a separate build is optional.

The command detects and remembers the project. Use --project PATH to select one
explicitly, --release-version 0.7.4 to override the version, or --keys PATH to
import existing signing keys once. --status shows the selected project.
Private signing keys must remain outside the source checkout. These scripts
never create replacement signing keys. All tools, backups, and build state live
under target/. Older helper and patch scripts are no longer used.
