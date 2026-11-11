#!/usr/bin/env python3
"""Build a new static GitHub Pages update tree from trusted release artifacts."""
import argparse
import hashlib
import html
import json
import os
import re
import shutil
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path
from urllib.parse import urlsplit

from securesystemslib.signer import CryptoSigner
from tuf.api.metadata import Metadata, MetaFile, Role, Root, Snapshot, TargetFile, Targets, Timestamp

ROLES = ("root", "targets", "snapshot", "timestamp")
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
ARCHES = {"x86_64": 62, "aarch64": 183}


def expires(days):
    return datetime.now(timezone.utc).replace(microsecond=0) + timedelta(days=days)


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def initialize(directory):
    directory.mkdir(mode=0o700, parents=True)
    root = Metadata(Root(expires=expires(365 * 3), consistent_snapshot=True))
    for role in ROLES:
        signer = CryptoSigner.generate_ed25519()
        private = directory / f"{role}.pem"
        with os.fdopen(os.open(private, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb") as file:
            file.write(signer.private_bytes)
        root.signed.roles[role] = Role([], 1)
        root.signed.add_key(signer.public_key, role)
    root.sign(load_key(directory, root, "root"))
    root.to_file(str(directory / "root.json"))


def load_key(directory, root, role):
    key_id, = root.signed.roles[role].keyids
    key = root.signed.keys[key_id]
    return CryptoSigner.from_priv_key_uri(f"file2:{directory / (role + '.pem')}", key)


def signed(directory, root, role, payload):
    metadata = Metadata(payload)
    metadata.sign(load_key(directory, root, role))
    root.verify_delegate(role, metadata)
    return metadata.to_bytes()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def metafile(version, data):
    return MetaFile(version, len(data), {"sha256": digest(data)})


def prior_tuf(site, root):
    meta = site / "tuf/metadata"
    timestamp = Metadata.from_file(str(meta / "timestamp.json"))
    root.verify_delegate("timestamp", timestamp)
    snapshot_file = meta / f"{timestamp.signed.snapshot_meta.version}.snapshot.json"
    timestamp.signed.snapshot_meta.verify_length_and_hashes(snapshot_file.read_bytes())
    snapshot = Metadata.from_file(str(snapshot_file))
    root.verify_delegate("snapshot", snapshot)
    target_meta = snapshot.signed.meta["targets.json"]
    target_file = meta / f"{target_meta.version}.targets.json"
    target_meta.verify_length_and_hashes(target_file.read_bytes())
    targets = Metadata.from_file(str(target_file))
    root.verify_delegate("targets", targets)
    return targets, snapshot, timestamp


def target_path(site, target):
    return site / "tuf/targets" / f"{target.hashes['sha256']}.{target.path}"


def write_tuf(site, keys, root, targets, versions):
    tv, sv, xv = versions
    meta = site / "tuf/metadata"
    root_bytes = (keys / "root.json").read_bytes()
    root_path = meta / f"{root.signed.version}.root.json"
    if root_path.exists() and root_path.read_bytes() != root_bytes:
        raise ValueError("The TUF root changed without a version change")
    write(root_path, root_bytes)
    targets_data = signed(keys, root, "targets", Targets(version=tv, expires=expires(90), targets=targets))
    snapshot_data = signed(keys, root, "snapshot", Snapshot(version=sv, expires=expires(90),
                          meta={"targets.json": metafile(tv, targets_data)}))
    timestamp_data = signed(keys, root, "timestamp", Timestamp(version=xv, expires=expires(14),
                            snapshot_meta=metafile(sv, snapshot_data)))
    write(meta / f"{tv}.targets.json", targets_data)
    write(meta / f"{sv}.snapshot.json", snapshot_data)
    write(meta / "timestamp.json", timestamp_data)


def safe_base(value):
    parsed = urlsplit(value)
    if (parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password
            or parsed.query or parsed.fragment or not value.endswith("/")
            or not re.fullmatch(r"https://[A-Za-z0-9.-]+(?::[0-9]+)?/[A-Za-z0-9._~/-]*", value)):
        raise ValueError("Use an HTTPS GitHub Pages base URL ending in /")
    return value


def pages(site, github, version, migrate):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", github):
        raise ValueError("Use a GitHub owner/repository name")
    release = html.escape(f"https://github.com/{github}/releases/tag/v{version}")
    apt_link = "Migration from APT" if migrate or (site / "apt/retired").exists() else "APT setup"
    index = (
        f'<!doctype html><meta charset="utf-8"><title>Darah updates</title><h1>Darah {version}</h1>'
        f'<p><a href="{release}">Download Darah and its source</a></p>'
        f'<p><a href="apt/">{apt_link}</a></p>\n'
    )
    write(site / "index.html", index.encode())
    if migrate:
        apt_page = (
            '<!doctype html><meta charset="utf-8"><title>Darah APT migration</title>'
            '<h1>Darah APT migration</h1><p>New releases use AppImage and Flatpak. '
            'Existing Debian packages and project files remain usable.</p>'
            f'<p><a href="{release}">Get the current release</a></p>'
            '<p>Remove the Darah APT source when you switch packages:</p>'
            '<pre>sudo rm -f /etc/apt/sources.list.d/darah.sources\nsudo apt update</pre>'
        )
        write(site / "apt/index.html", apt_page.encode())
        write(site / "apt/retired", b"")
    write(site / ".nojekyll", b"")


def publish(args):
    safe_base(args.base)
    root = Metadata.from_file(str(args.keys / "root.json"))
    root.verify_delegate("root", root)
    if root.signed.is_expired():
        raise ValueError("The TUF root has expired")
    output = args.output.resolve()
    if output.exists():
        raise ValueError("Choose a new output directory. The previous site stays intact.")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".darah-pages-", dir=output.parent) as temp:
        site = Path(temp) / "site"
        site.mkdir()
        versions = (1, 1, 1)
        prior_arches = []
        if args.previous:
            # Treat the previous tree as untrusted input. Its signatures anchor the next release.
            previous = args.previous.resolve()
            old, snap, time = prior_tuf(previous, root)
            versions = tuple(item.signed.version + 1 for item in (old, snap, time))
            stable = old.signed.targets["stable.json"]
            stable_path = target_path(previous, stable)
            stable.verify_length_and_hashes(stable_path.read_bytes())
            prior_version = json.loads(stable_path.read_bytes())["version"]
            prior_arches = {
                cpu for cpu in ARCHES
                if f"darah-eda-{prior_version}-linux-{cpu}.AppImage" in old.signed.targets
            }
            if (previous / "apt").is_dir():
                shutil.copytree(previous / "apt", site / "apt")
            for item in old.signed.targets.values():
                source = target_path(previous, item)
                with source.open("rb") as file:
                    item.verify_length_and_hashes(file)
                destination = target_path(site, item)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)
            # Preserve versioned TUF indexes for clients mid-update.
            for path in (previous / "tuf/metadata").glob("*.json"):
                if re.fullmatch(r"[0-9]+\.(root|snapshot|targets)\.json", path.name):
                    write(site / "tuf/metadata" / path.name, path.read_bytes())

        if args.command == "refresh":
            version = prior_version
            targets = old.signed.targets
        else:
            version = args.version
            if not VERSION.fullmatch(version):
                raise ValueError("Use a stable MAJOR.MINOR.PATCH version")
            if args.previous and tuple(map(int, version.split("."))) <= tuple(map(int, prior_version.split("."))):
                raise ValueError("New stable releases must have a higher version")
            arches = [
                cpu for cpu in ARCHES
                if (args.artifacts / f"darah-eda-{version}-linux-{cpu}.AppImage").is_file()
            ]
            if not arches or not set(prior_arches).issubset(arches):
                raise ValueError("Provide AppImages for every previously published architecture")
            manifest = Path(temp) / "stable.json"
            notes = args.notes.read_text()
            manifest.write_text(json.dumps({"version": version, "notes": notes}, ensure_ascii=False))
            if manifest.stat().st_size > 64 * 1024:
                raise ValueError("The update manifest exceeds 64 KiB")
            artifacts = [manifest]
            for cpu in arches:
                flatpak = args.artifacts / f"darah-eda-{version}-linux-{cpu}.flatpak"
                if not args.appimage_only and (not flatpak.is_file() or flatpak.stat().st_size == 0):
                    raise ValueError(f"Missing Flatpak bundle: {flatpak.name}")
                appimage = args.artifacts / f"darah-eda-{version}-linux-{cpu}.AppImage"
                with appimage.open("rb") as file:
                    header = file.read(20)
                if (header[:6] != b"\x7fELF\x02\x01" or header[8:11] != b"AI\x02"
                        or int.from_bytes(header[18:20], "little") != ARCHES[cpu] or appimage.stat().st_size > 512 * 1024**2):
                    raise ValueError(f"Invalid type 2 AppImage, architecture or size: {appimage}")
                artifacts.append(appimage)
            targets = {}
            for file in artifacts:
                target = TargetFile.from_file(file.name, str(file), ["sha256"])
                destination = target_path(site, target)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(file, destination)
                targets[file.name] = target
        write_tuf(site, args.keys, root, targets, versions)
        pages(site, args.github, version, migrate=args.command == "publish" and not args.appimage_only)
        sizes = [path.stat().st_size for path in site.rglob("*") if path.is_file()]
        if max(sizes) > 100 * 1024**2 or sum(sizes) > 900 * 1024**2:
            raise ValueError("The update site exceeds the GitHub Pages branch size budget")
        site.rename(output)
    print(f"Prepared {output}. Review and deploy the complete directory together.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init")
    init.add_argument("keys", type=Path)
    for name in ("publish", "refresh"):
        cmd = commands.add_parser(name)
        cmd.set_defaults(appimage_only=False)
        for flag in ("keys", "output", "previous"):
            cmd.add_argument(f"--{flag}", type=Path, required=flag != "previous" or name == "refresh")
        for flag in ("base", "github"):
            cmd.add_argument(f"--{flag}", required=True)
        if name == "publish":
            cmd.add_argument("--artifacts", type=Path, required=True)
            cmd.add_argument("--notes", type=Path, required=True)
            cmd.add_argument("--version", required=True)
            cmd.add_argument("--appimage-only", action="store_true", help="publish an AppImage release before Flatpak is ready")
    args = parser.parse_args()
    try:
        if args.command == "init":
            initialize(args.keys.resolve())
        else:
            publish(args)
    except Exception as error:
        parser.exit(1, f"Update repository was not created: {error}\n")


if __name__ == "__main__":
    main()
