#!/usr/bin/env python3
"""Build and publish Darah EDA from its source checkout (Linux, Python 3.11+)."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
from urllib.error import URLError
from urllib.request import urlopen

REPO = "citrineio/darah-eda"
PUBLIC_GIT = f"https://github.com/{REPO}.git"
SSH_GIT = f"git@github.com:{REPO}.git"
PAGES = "https://citrineio.github.io/darah-eda/"
ARCHITECTURES = {"x86_64": 62, "aarch64": 183}


class ReleaseError(Exception):
    pass


def run(*args, capture=False, **kwargs):
    return subprocess.run(
        args, check=True, text=True, stdout=subprocess.PIPE if capture else None, **kwargs
    ).stdout


def require(*commands):
    for name in commands:
        if shutil.which(name) is None:
            raise ReleaseError(f"Install {name} and run this command again.")


def version_from_source():
    package = tomllib.loads(Path("Cargo.toml").read_text())["package"]
    version = package["version"]
    if package["name"] != "darah-eda" or not re.fullmatch(
        r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version
    ):
        raise ReleaseError("Run from the Darah source directory with a stable version in Cargo.toml.")
    return version


def build_environment(appimage_only=False):
    require("cargo")
    if not appimage_only:
        require("flatpak", "flatpak-builder")
    cpu = platform.machine()
    if cpu not in ARCHITECTURES:
        raise ReleaseError(f"Unsupported build architecture: {cpu}")
    filename = f"linuxdeploy-{cpu}.AppImage"
    configured = os.environ.get("LINUXDEPLOY")
    candidates = [configured] if configured else [
        filename, f"../{filename}", shutil.which(filename),
    ]
    for candidate in candidates:
        if candidate and Path(candidate).is_file() and os.access(candidate, os.X_OK):
            linuxdeploy = Path(candidate).resolve()
            break
    else:
        raise ReleaseError(f"Put the executable {filename} next to the source folder or set LINUXDEPLOY.")
    env = os.environ.copy()
    flags = env.get("CARGO_ENCODED_RUSTFLAGS")
    flags = flags.split("\x1f") if flags else shlex.split(env.get("RUSTFLAGS", ""))
    cargo_home = Path(env.get("CARGO_HOME", Path.home() / ".cargo")).resolve()
    flags.extend((
        f"--remap-path-prefix={Path.cwd()}=/usr/src/darah-eda",
        f"--remap-path-prefix={cargo_home}=/cargo",
    ))
    env.update(
        LINUXDEPLOY=str(linuxdeploy),
        DARAH_UPDATE_BASE_URL=PAGES,
        CARGO_ENCODED_RUSTFLAGS="\x1f".join(flags),
    )
    return cpu, env


def build(version, destination, appimage_only=False):
    cpu, env = build_environment(appimage_only)
    print(f"Testing and building Darah {version} for {cpu}.", flush=True)
    run("cargo", "+stable", "test", "--locked", env=env)
    if not appimage_only:
        run("bash", "scripts/build-flatpak.sh", env=env)
    machine = ARCHITECTURES[cpu]
    appimage = Path(f"target/appimage/Darah_EDA-{cpu}.AppImage")
    appimage.unlink(missing_ok=True)
    run("bash", "scripts/build-appimage.sh", env=env)
    with appimage.open("rb") as file:
        header = file.read(20)
    if (header[:6] != b"\x7fELF\x02\x01" or header[8:11] != b"AI\x02"
            or int.from_bytes(header[18:20], "little") != machine):
        raise ReleaseError("linuxdeploy did not produce a matching type 2 AppImage.")
    destination.mkdir(parents=True, exist_ok=True)
    sources = [appimage]
    artifacts = [destination / f"darah-eda-{version}-linux-{cpu}.AppImage"]
    if not appimage_only:
        flatpak = Path(f"target/flatpak/darah-eda-{version}-linux-{cpu}.flatpak")
        sources.append(flatpak)
        artifacts.append(destination / flatpak.name)
    for source, target in zip(sources, artifacts):
        shutil.copy2(source, target)
    for artifact in artifacts:
        print(artifact.resolve())
    return artifacts


def remote_tag(tag):
    ref = f"refs/tags/{tag}"
    output = run("git", "ls-remote", PUBLIC_GIT, ref, ref + "^{}", capture=True)
    refs = {name: oid for oid, name in (line.split() for line in output.splitlines())}
    return refs.get(ref + "^{}", refs.get(ref))


def public_source_commit():
    if run("git", "status", "--porcelain", capture=True).strip():
        raise ReleaseError("Commit your release changes and move any untracked files out of the checkout.")
    commit = run("git", "rev-parse", "HEAD", capture=True).strip()
    main = run("git", "ls-remote", PUBLIC_GIT, "refs/heads/main", capture=True).split()
    if not main or main[0] != commit:
        raise ReleaseError(f"Check out the exact commit published on {REPO} main before releasing.")
    return commit


def sha256(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def prepare_release(version, commit, state, notes, appimage_only=False):
    prepared = state / f"v{version}-{commit}"
    if not prepared.is_dir():
        print("Preparing installers and release notes.", flush=True)
        with tempfile.TemporaryDirectory(dir=state) as directory:
            staging = Path(directory) / "prepared"
            artifacts = build(version, staging / "assets", appimage_only)
            if public_source_commit() != commit:
                raise ReleaseError("The release commit changed during the build.")
            changes = notes.read_text().strip() if notes else f"Darah EDA v{version}."
            notes_path = staging / "release-notes.md"
            notes_path.write_text(
                f"{changes}\n\n"
                f"[Downloads and update instructions]({PAGES})\n"
            )
            checksums = {str(p.relative_to(staging)): sha256(p) for p in [*artifacts, notes_path]}
            (staging / "checksums.json").write_text(json.dumps(checksums))
            staging.rename(prepared)
    checksums = json.loads((prepared / "checksums.json").read_text())
    artifacts = sorted((prepared / "assets").iterdir())
    notes_path = prepared / "release-notes.md"
    if {str(p.relative_to(prepared)): sha256(p) for p in [*artifacts, notes_path]} != checksums:
        raise ReleaseError(f"The prepared release changed on disk. Inspect {prepared}.")
    return artifacts, notes_path


def publisher():
    python = Path(os.environ.get("DARAH_PUBLISHER_PYTHON", "../darah-publisher/bin/python")).absolute()
    keys = Path(os.environ.get("DARAH_UPDATE_KEYS", "../darah-update-keys")).resolve()
    if not os.access(python, os.X_OK):
        raise ReleaseError(f"Cannot find the publisher Python at {python}. Set DARAH_PUBLISHER_PYTHON if it moved.")
    if not keys.is_dir():
        raise ReleaseError(f"Cannot find your existing signing keys at {keys}. Set DARAH_UPDATE_KEYS if they moved.")
    if keys.is_relative_to(Path.cwd()):
        raise ReleaseError("Keep your signing keys outside the source checkout.")
    if json.loads((keys / "root.json").read_text()) != json.loads(
        Path("packaging/update-root.json").read_text()
    ):
        raise ReleaseError("The signing root does not match the public root bundled with Darah.")
    env = os.environ.copy()
    return python, keys, env


def github_release(tag, commit, artifacts, notes):
    def gh(*args, **kwargs):
        return run("gh", "release", *args, "--repo", REPO, **kwargs)

    def view():
        try:
            return json.loads(gh(
                "view", tag, "--json", "assets,isDraft,isPrerelease,targetCommitish",
                capture=True, stderr=subprocess.PIPE,
            ))
        except subprocess.CalledProcessError as error:
            if "release not found" in error.stderr.lower():
                return None
            raise ReleaseError(error.stderr.strip()) from error

    release = view()
    if release is None:
        gh("create", tag, "--target", commit, "--draft", "--title", f"Darah EDA {tag}",
           "--notes-file", notes, *artifacts)
        release = view()
        if release is None:
            raise ReleaseError("The release draft is not visible yet. Rerun publish to resume.")
    if release["isPrerelease"]:
        raise ReleaseError(f"{tag} is marked as a prerelease.")
    tagged = remote_tag(tag)
    if (tagged or release["targetCommitish"]) != commit:
        raise ReleaseError(f"The existing GitHub release {tag} points to a different commit.")
    uploaded = {item["name"] for item in release["assets"]}
    expected = {p.name for p in artifacts}
    if uploaded - expected or (not release["isDraft"] and uploaded != expected):
        raise ReleaseError(f"The existing release {tag} has unexpected assets. Inspect it before continuing.")
    missing = [p for p in artifacts if p.name not in uploaded]
    if missing:
        gh("upload", tag, *missing)
    with tempfile.TemporaryDirectory() as directory:
        options = [item for path in artifacts for item in ("--pattern", path.name)]
        gh("download", tag, "--dir", directory, *options)
        for artifact in artifacts:
            if sha256(Path(directory) / artifact.name) != sha256(artifact):
                raise ReleaseError(f"The uploaded {artifact.name} differs from the prepared file.")
    if release["isDraft"]:
        gh("edit", tag, "--draft=false")
    if remote_tag(tag) != commit:
        raise ReleaseError(f"The public tag for {tag} does not match the built commit. Rerun publish to recheck.")


TIMESTAMP = Path("tuf/metadata/timestamp.json")


def wait_for_pages(expected):
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        try:
            with urlopen(PAGES + TIMESTAMP.as_posix(), timeout=8) as response:
                if response.read(len(expected) + 1) == expected:
                    print("The new feed is live. Open Darah and click Updates → Check now.")
                    return
        except (URLError, TimeoutError):
            pass
        time.sleep(5)
    print(f"The push succeeded and Pages is still deploying. Check {PAGES} in a few minutes.")


def deploy(checkout, site, message):
    # The checkout is temporary and contains only the public updates branch.
    run("git", "-C", checkout, "rm", "-qr", "--", ".")
    shutil.copytree(site, checkout, dirs_exist_ok=True)
    run("git", "-C", checkout, "add", "--all")
    run("git", "-C", checkout, "commit", "-m", message)
    run("git", "-C", checkout, "push", "origin", "HEAD:updates")
    print("Pushed the signed feed. Waiting for GitHub Pages.", flush=True)
    wait_for_pages((site / TIMESTAMP).read_bytes())


def signed_feed_version(python, checkout):
    # Reuse the publisher's signature and target verification.
    code = """
import json, runpy, sys
from pathlib import Path
publisher = runpy.run_path("scripts/publish-updates.py")
root = publisher["Metadata"].from_file("packaging/update-root.json")
site = Path(sys.argv[1])
targets, _, _ = publisher["prior_tuf"](site, root)
target = targets.signed.targets["stable.json"]
data = publisher["target_path"](site, target).read_bytes()
target.verify_length_and_hashes(data)
print(json.loads(data)["version"])
"""
    return run(python, "-c", code, checkout, capture=True).strip()


def publish_or_refresh(args, version, state):
    require("git")
    python, keys, env = publisher()
    publishing = args.command == "publish"
    if publishing:
        require("gh")
        run("gh", "auth", "status", "--hostname", "github.com")
        commit = public_source_commit()
        tag = f"v{version}"
        tagged = remote_tag(tag)
        if tagged and tagged != commit:
            raise ReleaseError(f"{tag} already exists on a different commit. Bump the version in Cargo.toml.")
    with tempfile.TemporaryDirectory(prefix="darah-pages-") as directory:
        checkout = Path(directory) / "updates"
        site = Path(directory) / "site"
        run("git", "clone", "--quiet", "--single-branch", "--branch", "updates",
            "--no-tags", "--depth", "1", SSH_GIT, checkout)
        options = []
        if publishing:
            current = signed_feed_version(python, checkout)
            if current == version and tagged == commit:
                print(f"{tag} is already on the signed updates branch. Checking GitHub Pages.", flush=True)
                wait_for_pages((checkout / TIMESTAMP).read_bytes())
                return
            if tuple(map(int, version.split("."))) <= tuple(map(int, current.split("."))):
                raise ReleaseError(f"The signed feed is already at {current}. Bump the version in Cargo.toml.")
            if tagged and not (state / f"{tag}-{commit}").is_dir():
                raise ReleaseError(f"{tag} already exists. Restore the prepared release to resume.")
            artifacts, notes = prepare_release(version, commit, state, args.notes, args.appimage_only)
            options = ["--version", version, "--notes", notes, "--artifacts", artifacts[0].parent]
            if args.appimage_only:
                options.append("--appimage-only")
        run(python, "scripts/publish-updates.py", args.command, "--keys", keys,
            "--base", PAGES, "--github", REPO,
            "--previous", checkout, "--output", site, *options, env=env)
        if publishing:
            github_release(tag, commit, artifacts, notes)
        message = f"Publish signed Darah {tag} update feed" if publishing else "Refresh Darah update feed"
        deploy(checkout, site, message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build_command = commands.add_parser("build", help="test and build AppImage and Flatpak locally")
    publish = commands.add_parser("publish", help="publish the committed version and deploy the signed feed")
    publish.add_argument("--notes", type=Path, help="optional release notes file")
    for command in (build_command, publish):
        command.add_argument("--appimage-only", action="store_true", help="build or publish the AppImage independently")
    commands.add_parser("refresh", help="renew the existing feed without rebuilding")
    args = parser.parse_args()

    try:
        version = version_from_source()
        state = Path("target/release-tools").resolve()
        state.mkdir(parents=True, exist_ok=True)
        with (state / "lock").open("a") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise ReleaseError("Another Darah release command is already running here.") from error
            if args.command == "build":
                build(version, Path("target/dist") / version, args.appimage_only)
            else:
                prepared_state = state / "appimage-only" if getattr(args, "appimage_only", False) else state
                prepared_state.mkdir(exist_ok=True)
                publish_or_refresh(args, version, prepared_state)
    except subprocess.CalledProcessError as error:
        parser.exit(1, f"Stopped after a command exited with status {error.returncode}. See the output above.\n")
    except (ReleaseError, OSError, KeyError, ValueError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
