#!/usr/bin/env python3
"""Publish Darah: version, build, GitHub release, and signed update feed.

Use this with darah-release.py in the same directory, or scripts/release.py
after installation. Existing signing keys are required; no keys are generated.
"""
import argparse
from datetime import datetime, timedelta, timezone
import fcntl
import hashlib
import html
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
from types import SimpleNamespace
from urllib.error import URLError
from urllib.parse import urlsplit
from urllib.request import urlopen

sys.dont_write_bytecode = True
REPO = 'citrineio/darah-eda'
PUBLIC_GIT = f'https://github.com/{REPO}.git'
SSH_GIT = f'git@github.com:{REPO}.git'
PAGES = 'https://citrineio.github.io/darah-eda/'
ROLES = ('root', 'targets', 'snapshot', 'timestamp')
VERSION = re.compile(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)')
ARCHES = {'x86_64': 62, 'aarch64': 183}
SIGNING_PACKAGES = ['tuf==6.0.0', 'securesystemslib[crypto]==1.4.0']
RETIRED = ['scripts/build-appimage.sh', 'scripts/bundle-appimage-runtime.py', 'scripts/publish-updates.py']


def execute(*args, capture=False, **kwargs):
    return subprocess.run(args, check=True, text=True,
        stdout=subprocess.PIPE if capture else None, **kwargs).stdout


def load_release(source):
    for path in [Path(__file__).with_name('darah-release.py'), Path(__file__).with_name('release.py'), source / 'scripts/release.py']:
        if not path.is_file(): continue
        spec = importlib.util.spec_from_file_location('darah_release', path)
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        if getattr(module, 'CONSOLIDATED_RELEASE_API', None) == 1:
            return module, path
    raise RuntimeError('Put darah-release.py beside this script. Both files are needed once.')


def find_keys(source, configured=None, home=None):
    home = Path(home) if home is not None else Path.home()
    expected = json.loads((source / 'packaging/update-root.json').read_text())
    state = source / 'target/release-tools/publisher-config.json'
    cached = {}
    if state.is_file():
        try: cached = json.loads(state.read_text())
        except (OSError, ValueError): pass
    explicit = configured or os.environ.get('DARAH_UPDATE_KEYS')

    def matches(path):
        path = Path(path).expanduser().resolve()
        if path.is_relative_to(source): return False
        if not all((path / (role + '.pem')).is_file() for role in ['targets', 'snapshot', 'timestamp']): return False
        try: return json.loads((path / 'root.json').read_text()) == expected
        except (OSError, ValueError): return False

    if explicit:
        if not matches(explicit):
            raise RuntimeError('The configured signing-key directory is missing, inside the checkout, or does not match packaging/update-root.json.')
        return Path(explicit).expanduser().resolve()
    candidates = [cached.get('keys'), source.parent / 'darah-update-keys',
        home / '.local/share/darah-eda/update-keys', home / '.local/share/darah-release/keys']
    for candidate in candidates:
        if candidate and matches(candidate): return Path(candidate).expanduser().resolve()
    # Search only relevant project folders, and inspect public root metadata,
    # never private key contents, while locating the established key directory.
    roots = []
    for parent in [source.parent, home / 'Downloads', home / '.local/share', home / '.config']:
        if not parent.is_dir(): continue
        for child in parent.iterdir():
            if child.is_dir() and any(word in child.name.lower() for word in ['darah', 'kawn', 'k8-coding']):
                roots.append(child)
    seen = set()
    for root in roots:
        for directory, dirs, files in os.walk(root, followlinks=False):
            path = Path(directory)
            if len(path.relative_to(root).parts) >= 5: dirs[:] = []
            dirs[:] = [name for name in dirs if name not in ['target', '.git', 'node_modules', '__pycache__', 'site-packages', '.venv', 'venv']]
            if path in seen: continue
            seen.add(path)
            if 'root.json' in files and matches(path): return path.resolve()
    raise RuntimeError('Existing signing keys were not found. Rerun with --keys /path/to/the/existing/darah-update-keys directory. No replacement keys were created.')


def python_has_signer(python):
    if not os.access(python, os.X_OK): return False
    code = 'from importlib.metadata import version; assert version("tuf")=="6.0.0"; assert version("securesystemslib")=="1.4.0"; import cryptography; import tuf.api.metadata; import securesystemslib.signer'
    return subprocess.run([str(python), '-c', code], stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL).returncode == 0


def prepare_python(source):
    configured = os.environ.get('DARAH_PUBLISHER_PYTHON')
    cache_root = Path(os.environ.get('DARAH_DEVELOP_CACHE', source / 'target/release-tools'))
    cache = cache_root / 'publisher-venv'
    candidates = [Path(configured).expanduser().absolute()] if configured else []
    candidates += [cache / 'bin/python', source.parent / 'darah-publisher/bin/python', Path(sys.executable)]
    for candidate in candidates:
        if python_has_signer(candidate): return candidate.absolute()
    print('Preparing the signing Python environment automatically.', flush=True)
    python = cache / 'bin/python'
    pip_ready = python.is_file() and subprocess.run([str(python), '-m', 'pip', '--version'],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
    if not pip_ready:
        result = subprocess.run([sys.executable, '-m', 'venv', str(cache)], capture_output=True, text=True)
        if result.returncode:
            if shutil.which('apt-get') and (os.geteuid() == 0 or shutil.which('sudo')):
                prefix = [] if os.geteuid() == 0 else ['sudo']
                execute(*prefix, 'apt-get', 'install', '-y', 'python3-venv')
                execute(sys.executable, '-m', 'venv', str(cache))
            else:
                raise RuntimeError('Python venv support is missing. Install python3-venv, then rerun the same command.')
    execute(str(python), '-m', 'pip', 'install', '--disable-pip-version-check', *SIGNING_PACKAGES)
    if not python_has_signer(python): raise RuntimeError('Signing Python setup did not complete. Rerun the same command.')
    return python.absolute()


def load_signing_api():
    from securesystemslib.signer import CryptoSigner
    from tuf.api.metadata import Metadata, MetaFile, Root, Snapshot, TargetFile, Targets, Timestamp
    globals().update(CryptoSigner=CryptoSigner, Metadata=Metadata, MetaFile=MetaFile,
        Root=Root, Snapshot=Snapshot, TargetFile=TargetFile, Targets=Targets, Timestamp=Timestamp)


def version_tuple(version):
    if not VERSION.fullmatch(version): raise RuntimeError(f'Invalid release version: {version}')
    return tuple(map(int, version.split('.')))


def file_hash(path):
    with Path(path).open('rb') as file: return hashlib.file_digest(file, 'sha256').hexdigest()


def git_tag_commit(tag):
    output = execute('git', 'ls-remote', PUBLIC_GIT, f'refs/tags/{tag}', f'refs/tags/{tag}^{{}}', capture=True)
    refs = {name: sha for sha, name in (line.split() for line in output.splitlines())}
    return refs.get(f'refs/tags/{tag}^{{}}', refs.get(f'refs/tags/{tag}'))


def github_release(tag, commit, artifact, notes):
    def gh(*args, **kwargs): return execute('gh', 'release', *args, '--repo', REPO, **kwargs)
    def view():
        try: return json.loads(gh('view', tag, '--json', 'assets,isDraft,isPrerelease,targetCommitish', capture=True, stderr=subprocess.PIPE))
        except subprocess.CalledProcessError as error:
            if 'release not found' in (error.stderr or '').lower(): return None
            raise RuntimeError((error.stderr or str(error)).strip())
    found = view()
    if found is None:
        gh('create', tag, '--target', commit, '--draft', '--title', f'Darah EDA {tag}', '--notes-file', str(notes), str(artifact))
        found = view()
    if found is None: raise RuntimeError('The release draft is not visible yet. Rerun the same command.')
    if found['isPrerelease']: raise RuntimeError('The existing release is marked as a prerelease.')
    if (git_tag_commit(tag) or found['targetCommitish']) != commit: raise RuntimeError('The existing release points to a different source commit.')
    names = {item['name'] for item in found['assets']}
    if names - {artifact.name}: raise RuntimeError('The release already contains unexpected assets; it was left unchanged.')
    if artifact.name not in names: gh('upload', tag, str(artifact))
    with tempfile.TemporaryDirectory() as name:
        gh('download', tag, '--dir', name, '--pattern', artifact.name)
        if file_hash(Path(name) / artifact.name) != file_hash(artifact): raise RuntimeError('Uploaded AppImage checksum does not match the verified build.')
    if found['isDraft']: gh('edit', tag, '--draft=false')
    if git_tag_commit(tag) != commit: raise RuntimeError('The public release tag does not match the built commit.')


def install_scripts(source, release, release_path):
    documentation = '''# Build and publish Darah AppImages

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
'''
    updates = {'scripts/release.py': release_path.read_bytes(),
        'scripts/publish.py': Path(__file__).read_bytes(),
        'docs/publishing-updates.md': documentation.encode(),
        'docs/appimage-runtime.md': ('# AppImage runtime\n\nBuild with `python3 scripts/release.py`; publish with `python3 scripts/publish.py`.\n\n'
            'The release script contains all runtime packaging and checks. It bundles glibc, its matching loader, the export renderer, libraries, and fallback fonts. '
            'Older hosts use the private runtime; compatible newer hosts use their own glibc. Graphics drivers remain host supplied. '
            'Use DARAH_RUNTIME_DEBUG=1 for runtime diagnostics.\n').encode()}
    readme = source / 'README.md'
    if readme.is_file():
        text = readme.read_text()
        for before, after in [('python3 scripts/release.py publish --appimage-only', 'python3 scripts/publish.py'),
            ('python3 scripts/release.py publish', 'darah-develop --publish'),
            ('python3 scripts/release.py build --appimage-only', 'darah-develop --build'),
            ('python3 scripts/release.py build', 'darah-develop --build'),
            ('python3 scripts/publish.py', 'darah-develop --publish')]:
            text = text.replace(before, after)
        if text != readme.read_text(): updates['README.md'] = text.encode()
    backup = source / 'target/release-tools/backups' / datetime.now(timezone.utc).strftime('%Y%m%d-%H%M%S-%f')
    for name in list(updates) + RETIRED:
        path = source / name
        if path.is_symlink(): raise RuntimeError(f'Expected a regular source file: {name}')
        if path.is_file() and (name in RETIRED or path.read_bytes() != updates[name]):
            saved = backup / name; saved.parent.mkdir(parents=True, exist_ok=True); shutil.copy2(path, saved)
    for name, data in updates.items():
        path = source / name; path.parent.mkdir(parents=True, exist_ok=True)
        if not path.is_file() or path.read_bytes() != data:
            with tempfile.NamedTemporaryFile('wb', dir=path.parent, delete=False) as file:
                temporary = Path(file.name); file.write(data)
            temporary.chmod(0o755 if name.endswith('.py') else 0o644); temporary.replace(path)
    for name in RETIRED: (source / name).unlink(missing_ok=True)


def stage_project_files(source):
    # Only existing tracked source and recognizable public project files are
    # committed; downloaded installers and unrelated untracked files stay out.
    changed = execute('git', 'diff', 'HEAD', '--name-only', '-z', capture=True).split('\0')
    untracked = execute('git', 'ls-files', '--others', '--exclude-standard', '-z', capture=True).split('\0')
    candidates = {name for name in changed if name}
    extensions = {'.rs', '.toml', '.md', '.py', '.sh', '.svg', '.png', '.ico', '.rgba', '.desktop', '.json', '.xml', '.yml', '.yaml'}
    for name in untracked:
        path = Path(name)
        if name and path.parts[0] in ['src', 'scripts', 'docs', 'packaging', '.github'] and path.suffix in extensions:
            candidates.add(name)
    for name in candidates:
        if Path(name).suffix.lower() in ['.pem', '.key'] or Path(name).name.startswith('.env'):
            raise RuntimeError('A private-key/environment file is in the pending Git changes; publication stopped.')
    if candidates: execute('git', 'add', '-A', '--', *sorted(candidates))



def expires(days):
    return datetime.now(timezone.utc).replace(microsecond=0) + timedelta(days=days)

def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)

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

def build_signed_site(args):
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
    argv = sys.argv[1:]
    if argv[:1] == ['publish']: argv = argv[1:]
    parser = argparse.ArgumentParser(description='Publish Darah. Handles setup, versioning, build reuse, GitHub, and the signed feed.')
    parser.add_argument('source', nargs='?', default='.', type=Path)
    parser.add_argument('--version', help='explicit new version; otherwise choose the next patch release when needed')
    parser.add_argument('--keys', type=Path, help='existing signing keys, if automatic discovery cannot locate them')
    parser.add_argument('--appimage-only', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    release, release_path = load_release(args.source.expanduser().resolve())
    source = release.source_directory(args.source)
    keys = find_keys(source, args.keys)
    python = prepare_python(source)
    state = source / 'target/release-tools'; state.mkdir(parents=True, exist_ok=True)
    release.write_json(state / 'publisher-config.json', {'keys': str(keys), 'python': str(python)})
    os.environ.update(DARAH_UPDATE_KEYS=str(keys), DARAH_PUBLISHER_PYTHON=str(python))
    if not python_has_signer(sys.executable):
        print('Starting the prepared signing environment.', flush=True)
        os.execv(str(python), [str(python), str(Path(__file__).resolve()), *sys.argv[1:]])
    load_signing_api()
    root = Metadata.from_file(str(keys / 'root.json')); root.verify_delegate('root', root)
    if root.signed.is_expired(): raise RuntimeError('The existing public trust root has expired; publication stopped.')
    for role in ['targets', 'snapshot', 'timestamp']: load_key(keys, root, role)
    os.chdir(source)
    for name in ['git', 'gh']:
        if not shutil.which(name): raise RuntimeError(f'{name} is missing. Install it and rerun this command.')
    execute('gh', 'auth', 'status', '--hostname', 'github.com')
    if not (source / '.git').exists():
        execute('git', 'init', '-b', 'main'); execute('git', 'fetch', '--no-tags', PUBLIC_GIT, 'main')
        execute('git', 'reset', '--mixed', 'FETCH_HEAD'); execute('git', 'remote', 'add', 'origin', SSH_GIT)
    if Path(execute('git', 'rev-parse', '--show-toplevel', capture=True).strip()).resolve() != source:
        raise RuntimeError('The Git root must be the Darah source directory.')
    execute('git', 'var', 'GIT_AUTHOR_IDENT', capture=True)
    execute('git', 'fetch', '--no-tags', PUBLIC_GIT, 'main')
    execute('git', 'merge-base', '--is-ancestor', 'FETCH_HEAD', 'HEAD')
    execute('git', 'push', '--dry-run', SSH_GIT, 'HEAD:main')
    with (state / 'publish.lock').open('a') as lock:
        try: fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError: raise RuntimeError('Another publisher is already running.')
        install_scripts(source, release, release_path)
        with tempfile.TemporaryDirectory(prefix='darah-publish-', dir=state) as name:
            work = Path(name); previous = work / 'updates'; site = work / 'site'
            execute('git', 'clone', '--quiet', '--single-branch', '--branch', 'updates', '--no-tags', '--depth', '1', SSH_GIT, str(previous))
            old, _, _ = prior_tuf(previous, root)
            stable_target = old.signed.targets['stable.json']; stable_path = target_path(previous, stable_target)
            stable_target.verify_length_and_hashes(stable_path.read_bytes())
            feed_version = json.loads(stable_path.read_text())['version']
            current = tomllib.loads((source / 'Cargo.toml').read_text())['package']['version']
            pending_path = state / 'publish-state.json'
            pending = json.loads(pending_path.read_text()) if pending_path.is_file() else {}
            if args.version: version = args.version
            elif pending.get('version') == current and not pending.get('complete'):
                version = current
            elif version_tuple(current) > version_tuple(feed_version): version = current
            else:
                changes = execute('git', 'diff', 'HEAD', '--name-only', capture=True).strip()
                if not changes and current == feed_version and git_tag_commit('v' + current) == execute('git', 'rev-parse', 'HEAD', capture=True).strip():
                    print(f'Already published: https://github.com/{REPO}/releases/tag/v{current}')
                    return
                a, b, c = max(version_tuple(current), version_tuple(feed_version)); version = f'{a}.{b}.{c + 1}'
            version_tuple(version)
            if version_tuple(version) < version_tuple(feed_version): raise RuntimeError('The requested version is older than the signed feed.')
            release.set_version(source, version)
            notes = source / f'docs/releases/{version}.md'
            stage_project_files(source)
            if execute('git', 'diff', '--cached', '--name-only', capture=True).strip(): execute('git', 'commit', '-m', f'Release Darah EDA v{version}')
            commit = execute('git', 'rev-parse', 'HEAD', capture=True).strip()
            tag = 'v' + version; tagged = git_tag_commit(tag)
            if tagged and tagged != commit: raise RuntimeError(f'{tag} already belongs to a different commit. Use --version with a new release version.')
            pending = {'version': version, 'commit': commit, 'complete': False}
            release.write_json(pending_path, pending)
            artifact = release.build_project(source)
            if execute('git', 'diff', 'HEAD', '--name-only', capture=True).strip() or execute('git', 'rev-parse', 'HEAD', capture=True).strip() != commit:
                raise RuntimeError('Source changed during the build; publication stopped.')
            execute('git', 'push', SSH_GIT, 'HEAD:main')
            if version == feed_version:
                github_release(tag, commit, artifact, notes)
                pending['complete'] = True; release.write_json(pending_path, pending)
                print(f'Published: https://github.com/{REPO}/releases/tag/{tag}')
                return
            build_signed_site(SimpleNamespace(command='publish', keys=keys, base=PAGES, github=REPO,
                version=version, notes=notes, artifacts=artifact.parent, previous=previous, output=site, appimage_only=True))
            github_release(tag, commit, artifact, notes)
            execute('git', '-C', str(previous), 'rm', '-qr', '--', '.')
            shutil.copytree(site, previous, dirs_exist_ok=True)
            execute('git', '-C', str(previous), 'add', '--all')
            execute('git', '-C', str(previous), 'commit', '-m', f'Publish signed Darah {tag} update feed')
            execute('git', '-C', str(previous), 'push', 'origin', 'HEAD:updates')
            pending['complete'] = True; release.write_json(pending_path, pending)
            print(f'Published: https://github.com/{REPO}/releases/tag/{tag}', flush=True)
            expected = (site / 'tuf/metadata/timestamp.json').read_bytes()
            for attempt in range(12):
                try:
                    with urlopen(PAGES + 'tuf/metadata/timestamp.json', timeout=8) as response:
                        if response.read(len(expected) + 1) == expected:
                            print('The signed update feed is live.'); return
                except (OSError, URLError): pass
                time.sleep(5)
            print(f'The signed feed was pushed; GitHub Pages is still deploying: {PAGES}')


if __name__ == '__main__':
    try: main()
    except Exception as error:
        print(f'Publish stopped: {error}', file=sys.stderr)
        print('Correct the reported issue and rerun this same command; completed work is reused.', file=sys.stderr)
        sys.exit(1)
