#!/usr/bin/env python3
"""Build and test Darah AppImage and Flatpak packages, including runtime and export checks."""
import argparse
import fcntl
import sys
import tomllib
import shlex
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import urllib.request

WINDOW_LIBS = (
    'libX11.so.6', 'libX11-xcb.so.1', 'libxcb.so.1', 'libXcursor.so.1',
    'libXrandr.so.2', 'libXi.so.6', 'libXinerama.so.1', 'libXrender.so.1',
    'libXfixes.so.3', 'libxkbcommon.so.0', 'libxkbcommon-x11.so.0',
    'libwayland-client.so.0', 'libwayland-cursor.so.0', 'libwayland-egl.so.1',
)
GLIBC_LIBS = (
    'libc.so.6', 'libm.so.6', 'libdl.so.2', 'libpthread.so.0', 'librt.so.1',
    'libutil.so.1', 'libresolv.so.2', 'libanl.so.1', 'libBrokenLocale.so.1',
    'libnss_files.so.2', 'libnss_dns.so.2', 'libnss_compat.so.2',
)
GRAPHICS = re.compile(r'^(lib(?:GL|EGL|GLES|OpenGL|GLX|GLdispatch|drm|gbm)|.*_dri\.so)')
TOOL_SHA256 = {
    'x86_64': 'e8c1169c4af66c3184d7ee110713d4be04befe671fa358e2c1591ee840553b03',
    'aarch64': 'd6a061996de690434996c2a523eafedd0a23780aaf02d69ce364537e71607e94',
}

LAUNCHER = r'''#!/bin/sh
set -eu
APPDIR=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
export APPDIR
program=$1
shift
case "$program" in darah-eda|rsvg-convert) ;; *) echo "Invalid Darah runtime program" >&2; exit 1;; esac
export PATH="$APPDIR/usr/bin${PATH:+:$PATH}"
private="$APPDIR/usr/lib/darah-glibc"
bundled_version=$(cat "$private/version")
host_version=$(getconf GNU_LIBC_VERSION 2>/dev/null || true)
host_version=${host_version#glibc }
host_loader='@HOST_LOADER@'

# Compare major/minor numbers without requiring Python in the target system.
version_at_least() {
    case "$1" in *[!0-9.]*|'') return 1;; esac
    old_ifs=$IFS
    IFS=.
    set -- $1 $2
    IFS=$old_ifs
    [ "$#" -eq 4 ] || return 1
    [ "$1" -gt "$3" ] || { [ "$1" -eq "$3" ] && [ "$2" -ge "$4" ]; }
}
mode=${DARAH_RUNTIME:-auto}
case "$mode" in
    auto)
        if [ -x "$host_loader" ] && version_at_least "$host_version" "$bundled_version"; then
            mode=host
        else mode=bundled
        fi
        ;;
    host|bundled) ;;
    *) echo "DARAH_RUNTIME must be auto, host, or bundled" >&2; exit 1;;
esac
if [ "$mode" = host ]; then
    loader=$host_loader
    libraries="$APPDIR/usr/lib"
else
    loader="$private/@LOADER_BASENAME@"
    libraries="$private:$APPDIR/usr/lib"
    if [ -d "$private/gconv" ]; then export GCONV_PATH="$private/gconv"; fi
fi
if [ "${DARAH_RUNTIME_DEBUG:-0}" = 1 ]; then
    echo "Darah runtime: $mode (host=$host_version bundled=$bundled_version)" >&2
fi
export FONTCONFIG_FILE="$APPDIR/usr/share/darah-runtime/fonts.conf"
if [ "${DARAH_RUNTIME_SELF_TEST:-0}" = 1 ]; then
    exec "$loader" --inhibit-cache --library-path "$libraries" --list "$APPDIR/usr/libexec/$program"
fi
# Keep the private libc out of the inherited LD_LIBRARY_PATH: external desktop
# utilities and /bin/sh must still start with their own matching host loader.
exec "$loader" --inhibit-cache --library-path "$libraries" \
    --argv0 "$APPDIR/usr/libexec/$program" "$APPDIR/usr/libexec/$program" "$@"
'''


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT)


def elf(path):
    try:
        with path.open('rb') as f:
            return f.read(4) == b'\x7fELF'
    except OSError:
        return False


def dependency_paths(binary):
    output = run('ldd', str(binary))
    if 'not found' in output:
        raise RuntimeError(f'Unresolved dependency of {binary}:\n{output}')
    paths = []
    for line in output.splitlines():
        match = re.search(r'=>\s+(/\S+)', line) or re.match(r'\s*(/\S+)', line)
        if match:
            paths.append(Path(match.group(1)))
    return paths


def write_executable(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        path.unlink()
    path.write_text(text)
    path.chmod(0o755)


def bundle(appdir):
    appdir = appdir.resolve()
    libdir = appdir / 'usr/lib'
    private = libdir / 'darah-glibc'
    libexec = appdir / 'usr/libexec'
    data = appdir / 'usr/share/darah-runtime'
    for directory in [libdir, private, libexec, data]:
        directory.mkdir(parents=True, exist_ok=True)
    for name in ['darah-eda', 'rsvg-convert']:
        if not elf(appdir / 'usr/bin' / name):
            raise RuntimeError(f'Expected native ELF program: {name}')
    app = appdir / 'usr/bin/darah-eda'
    interpreter = re.search(r'Requesting program interpreter: ([^\]]+)', run('readelf', '-l', str(app)))
    if not interpreter:
        raise RuntimeError('Cannot identify the matching glibc loader')
    interpreter = Path(interpreter.group(1))
    # Determine the actual build libc, not a version inferred from the OS name.
    libc = ctypes.CDLL(None)
    libc.gnu_get_libc_version.restype = ctypes.c_char_p
    version = libc.gnu_get_libc_version().decode()
    (private / 'version').write_text(version + '\n')
    cache = run('/sbin/ldconfig', '-p')
    libraries = {}
    for line in cache.splitlines():
        match = re.match(r'\s*(\S+)\s+\(([^)]+)\)\s+=>\s+(/\S+)', line)
        if match:
            libraries.setdefault(match.group(1), []).append(Path(match.group(3)))
    machine = re.search(r'Machine:\s*(.+)', run('readelf', '-h', str(app))).group(1)

    def locate(name, required=True):
        for candidate in libraries.get(name, []):
            if not candidate.exists() or not elf(candidate):
                continue
            if re.search(r'Machine:\s*(.+)', run('readelf', '-h', str(candidate))).group(1) == machine:
                return candidate
        if required:
            raise RuntimeError(f'Missing build library {name}; install the X11/Wayland client development/runtime packages.')

    roots = [appdir / 'usr/bin/darah-eda', appdir / 'usr/bin/rsvg-convert']
    roots.extend(locate(name) for name in WINDOW_LIBS)
    roots.extend(p for name in GLIBC_LIBS if (p := locate(name, required=False)))
    sources = {}
    work = list(roots)
    seen = set()
    glibc_names = set(GLIBC_LIBS)
    while work:
        source = work.pop()
        resolved = source.resolve()
        if resolved in seen:
            continue
        seen.add(resolved)
        for dependency in dependency_paths(source):
            if GRAPHICS.match(dependency.name):
                continue
            is_loader = dependency.resolve() == interpreter.resolve()
            destination = private if dependency.name in glibc_names or is_loader else libdir
            target = destination / (interpreter.name if is_loader else dependency.name)
            if source != dependency and target.resolve() != dependency.resolve():
                if target.is_symlink():
                    target.unlink()
                shutil.copy2(dependency.resolve(), target)
            sources[str(target.relative_to(appdir))] = str(dependency)
            work.append(dependency)
        if source in roots[2:]:
            target = (private if source.name in glibc_names else libdir) / source.name
            if target.resolve() != source.resolve():
                if target.is_symlink(): target.unlink()
                shutil.copy2(source.resolve(), target)
            sources[str(target.relative_to(appdir))] = str(source)
    shutil.copy2(interpreter.resolve(), private / interpreter.name)
    (private / interpreter.name).chmod(0o755)
    loader_alias = appdir / str(interpreter).lstrip('/')
    loader_alias.parent.mkdir(parents=True, exist_ok=True)
    if loader_alias.exists() or loader_alias.is_symlink(): loader_alias.unlink()
    loader_alias.symlink_to(os.path.relpath(private / interpreter.name, loader_alias.parent))
    # Catch any libc-family files deployed by a previous deployment pass.
    for name in GLIBC_LIBS:
        misplaced = libdir / name
        if misplaced.exists() or misplaced.is_symlink():
            misplaced.unlink()
    gconv = locate('libc.so.6').resolve().parent / 'gconv'
    if gconv.is_dir():
        shutil.copytree(gconv, private / 'gconv', dirs_exist_ok=True, symlinks=False)

    # Ship a font fallback so SVG text also renders on minimal installations.
    fontdir = appdir / 'usr/share/fonts/truetype/darah'
    fontdir.mkdir(parents=True, exist_ok=True)
    found = False
    for candidate in [Path('/usr/share/fonts/truetype/dejavu'), Path('/usr/share/fonts/dejavu')]:
        if candidate.is_dir():
            for font in candidate.glob('DejaVuSans*.ttf'):
                shutil.copy2(font, fontdir / font.name)
                found = True
    if not found:
        raise RuntimeError('Install fonts-dejavu-core on the build machine for portable exports.')
    (data / 'fonts.conf').write_text('''<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig><dir prefix="relative">../fonts</dir><dir>/usr/share/fonts</dir>
<dir>/usr/local/share/fonts</dir><cachedir prefix="xdg">fontconfig</cachedir>
<alias><family>sans-serif</family><prefer><family>DejaVu Sans</family></prefer></alias>
</fontconfig>\n''')

    # Preserve packaged dependency notices, including glibc's LGPL notice.
    notices = data / 'licenses'
    notices.mkdir(exist_ok=True)
    copied_packages = set()
    for source in set(sources.values()) | {str(interpreter)}:
        try:
            result = run('dpkg-query', '-S', source).splitlines()[0]
            package = result.rsplit(': ', 1)[0].split(':')[0]
            copied_packages.add(package)
        except (subprocess.CalledProcessError, IndexError):
            pass
    copied_packages.update(['libc6', 'fonts-dejavu-core'])
    for package in copied_packages:
        notice = Path('/usr/share/doc') / package / 'copyright'
        if notice.is_file():
            shutil.copy2(notice, notices / f'{package}.copyright')
    manifest = {'glibc': version, 'loader': str(interpreter),
                'host_graphics': True, 'libraries': sources,
                'note': 'Private loader is selected on older hosts. Newer compatible hosts use their own loader/glibc.'}
    (data / 'runtime.json').write_text(json.dumps(manifest, indent=2) + '\n')

    for name in ['darah-eda', 'rsvg-convert']:
        shutil.move(str(appdir / 'usr/bin' / name), libexec / name)
        write_executable(appdir / 'usr/bin' / name,
            '#!/bin/sh\nbase=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)\n'
            f'exec "$base/usr/libexec/darah-launch" {name} "$@"\n')
    write_executable(libexec / 'darah-launch', LAUNCHER.replace('@HOST_LOADER@', str(interpreter)).replace('@LOADER_BASENAME@', interpreter.name))
    write_executable(appdir / 'AppRun',
        '#!/bin/sh\nbase=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\n'
        'exec "$base/usr/libexec/darah-launch" darah-eda "$@"\n')
    print(f'Bundled glibc {version}, its matching loader, both native programs, and windowing libraries.')


def package(appdir, output, cache):
    import platform
    arch = platform.machine()
    expected = TOOL_SHA256[arch]
    configured = os.environ.get('APPIMAGETOOL')
    if configured:
        tool = Path(configured).expanduser().resolve()
        if not tool.is_file() or not os.access(tool, os.X_OK):
            raise RuntimeError('APPIMAGETOOL must point to an executable official appimagetool.')
    else:
        cache.mkdir(parents=True, exist_ok=True)
        tool = cache / f'appimagetool-{arch}-{expected[:12]}.AppImage'
        if not tool.exists():
            url = f'https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-{arch}.AppImage'
            temporary = tool.with_suffix('.download')
            try:
                print('Downloading official appimagetool (checksum pinned).', flush=True)
                with urllib.request.urlopen(url, timeout=120) as src, temporary.open('wb') as dest:
                    shutil.copyfileobj(src, dest)
                if hashlib.sha256(temporary.read_bytes()).hexdigest() != expected:
                    raise RuntimeError('The appimagetool download changed. Set APPIMAGETOOL to a verified tool, or update the pinned digest.')
                temporary.chmod(0o755)
                temporary.replace(tool)
            finally:
                temporary.unlink(missing_ok=True)
        if hashlib.sha256(tool.read_bytes()).hexdigest() != expected:
            raise RuntimeError('Cached appimagetool checksum mismatch; remove it and retry.')
    env = os.environ.copy()
    env.update(APPIMAGE_EXTRACT_AND_RUN='1', ARCH=arch)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    subprocess.run([str(tool), '--comp', 'zstd', '--mksquashfs-opt', '-processors',
                    '--mksquashfs-opt', '2', str(appdir.resolve()), str(output.resolve())],
                   env=env, check=True)


def verify(image):
    image = image.resolve()
    with tempfile.TemporaryDirectory(prefix='darah-runtime-check-') as name:
        work = Path(name)
        with (work / 'extract.log').open('w') as log:
            subprocess.run([str(image), '--appimage-extract'], cwd=work, stdout=log, check=True)
        appdir = work / 'squashfs-root'
        svg = work / 'input.svg'
        svg.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="160" height="80"><rect width="160" height="80" fill="#12685b"/><text x="8" y="44" fill="white" font-size="16">Darah runtime test</text></svg>')
        for mode in ['auto', 'bundled']:
            env = os.environ.copy()
            env.update(DARAH_RUNTIME=mode, DARAH_RUNTIME_DEBUG='1')
            for fmt, signature in [('png', b'\x89PNG\r\n\x1a\n'), ('pdf', b'%PDF-')]:
                result = subprocess.run([str(appdir / 'usr/bin/rsvg-convert'), f'--format={fmt}', str(svg)], env=env, capture_output=True, check=True)
                if not result.stdout.startswith(signature):
                    raise RuntimeError(f'{mode} runtime produced an invalid {fmt} export')
            env['DARAH_RUNTIME_SELF_TEST'] = '1'
            result = subprocess.run([str(appdir / 'AppRun')], env=env, capture_output=True, text=True, check=True)
            if 'not found' in result.stdout + result.stderr:
                raise RuntimeError(f'Unresolved native dependency in {mode} mode')
            if mode == 'bundled' and 'darah-glibc/libc.so.6' not in result.stdout:
                raise RuntimeError('The bundled runtime did not resolve its private libc')
        print('PASS: private glibc/loader, native dependency resolution, and PNG/PDF export (auto and bundled runtime).')



def atomic_copy_artifact(source, target):
    """Publish a completed artifact by replacing its directory entry."""
    target = Path(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile("wb", dir=target.parent,
                prefix=f".{target.name}.", suffix=".tmp", delete=False) as output:
            temporary = Path(output.name)
            with Path(source).open("rb") as input_file:
                shutil.copyfileobj(input_file, output)
            output.flush()
            os.fsync(output.fileno())
        shutil.copystat(source, temporary)
        os.replace(temporary, target)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)

CONSOLIDATED_RELEASE_API = 1
PAGES = 'https://citrineio.github.io/darah-eda/'


def source_directory(path):
    source = Path(path).expanduser().resolve()
    data = tomllib.loads((source / 'Cargo.toml').read_text())['package']
    if data['name'] != 'darah-eda':
        raise RuntimeError('Run from the Darah source folder, or pass its path.')
    return source


def prepare_release_notes(source, version):
    """Keep in-app history and release notes in sync, including resumed releases."""
    help_path = source / 'src/help.rs'
    help_text = help_path.read_text()
    section = re.search(r'(?m)^(?P<indent>[ \t]*)Self::Changes\s*=>\s*&\[\s*\n', help_text)
    if not section:
        raise RuntimeError("Cannot locate What's new in src/help.rs; no version files were changed.")
    entries = help_text[section.end():].split('],', 1)[0]
    headings = re.findall(r'\(\s*"v(\d+\.\d+\.\d+) · ', entries)
    if not headings:
        raise RuntimeError("Cannot read What's new headings; no version files were changed.")
    if tuple(map(int, headings[0].split('.'))) > tuple(map(int, version.split('.'))):
        raise RuntimeError("What's new already contains a newer version; no version files were changed.")
    if version in headings and headings[0] != version:
        raise RuntimeError("The requested version is not the newest What's new entry; no version files were changed.")
    notes = source / f'docs/releases/{version}.md'
    if version == '0.7.4':
        title = 'Portable Linux AppImage'
        body = ('AppImage builds include a private glibc runtime and its matching loader, '
                'windowing libraries, and the PNG/PDF renderer. The bundled runtime supports '
                'systems whose glibc is older than the build host.')
    else:
        title = 'Maintenance release'
        body = 'Maintenance release. See the release notes for details.'
    if headings[0] != version:
        # JSON escaping is valid for these ordinary Rust string literals.
        entry = section['indent'] + '    (' + json.dumps('v' + version + ' · ' + title, ensure_ascii=False)
        entry += ', ' + json.dumps(body, ensure_ascii=False) + '),\n'
        help_text = help_text[:section.end()] + entry + help_text[section.end():]
        help_path.write_text(help_text)
    if not notes.is_file():
        notes.parent.mkdir(parents=True, exist_ok=True)
        notes.write_text(f'Darah EDA v{version}\n\n{body}\n')
    return notes


def sync_flatpak_metadata(source, version):
    from datetime import datetime, timezone
    from xml.etree import ElementTree
    path = source / 'packaging/flatpak/io.github.citrineio.DarahEDA.metainfo.xml'
    if not path.is_file(): return
    original = path.read_text()
    tree = ElementTree.fromstring(original)
    releases = tree.find('releases')
    if releases is None: raise RuntimeError('The Flatpak AppStream metadata needs a releases section.')
    entries = list(releases)
    if entries and entries[0].get('version') == version: return
    if any(item.get('version') == version for item in entries):
        raise RuntimeError('The Flatpak version is not the newest AppStream release.')
    date = datetime.now(timezone.utc).date().isoformat()
    entry = f'    <release version="{version}" date="{date}">\n      <description><p>Darah EDA {version}.</p></description>\n    </release>\n'
    updated, count = re.subn(r'(<releases>\s*\n)', lambda match: match[1] + entry, original, count=1)
    if count != 1: raise RuntimeError('Cannot update the Flatpak AppStream releases section.')
    ElementTree.fromstring(updated)
    path.write_text(updated)


def set_version(source, version):
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version):
        raise RuntimeError('Use a plain release version such as 0.7.4.')
    manifest = source / 'Cargo.toml'
    text, count = re.subn(r'(?ms)(^\[package\]\s*\n.*?^version\s*=\s*")[^"]+(")',
        lambda m: m[1] + version + m[2], manifest.read_text(), count=1)
    if count != 1: raise RuntimeError('Cannot locate the Cargo package version.')
    lock = source / 'Cargo.lock'
    lock_text, count = re.subn(r'(?m)(^name = "darah-eda"\nversion = ")[^"]+(")',
        lambda m: m[1] + version + m[2], lock.read_text(), count=1)
    if count != 1: raise RuntimeError('Cannot locate Darah in Cargo.lock.')
    tomllib.loads(text); tomllib.loads(lock_text)
    prepare_release_notes(source, version)
    sync_flatpak_metadata(source, version)
    manifest.write_text(text); lock.write_text(lock_text)


def fingerprint(source):
    files = [source / name for name in ['Cargo.toml', 'Cargo.lock', 'build.rs']]
    for directory in ['src', 'packaging']:
        files.extend(sorted((source / directory).rglob('*')))
    sha = hashlib.sha256()
    for path in sorted(set(files)):
        if path.is_file():
            sha.update(str(path.relative_to(source)).encode()); sha.update(b'\0')
            sha.update(path.read_bytes()); sha.update(b'\0')
    sha.update(Path(__file__).read_bytes())
    sha.update(os.environ.get('RUSTFLAGS', '').encode())
    sha.update(os.environ.get('CARGO_ENCODED_RUSTFLAGS', '').encode())
    sha.update(os.environ.get('DARAH_UPDATE_BASE_URL', PAGES).encode())
    return sha.hexdigest()


def sha256_file(path):
    with Path(path).open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def write_json(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile('w', dir=path.parent, delete=False) as file:
        temporary = Path(file.name)
        json.dump(data, file, indent=2); file.write('\n')
    try: temporary.replace(path)
    finally: temporary.unlink(missing_ok=True)


def build_dependencies():
    required = ['cargo', 'rustc', 'readelf', 'ldd', 'rsvg-convert']
    missing = [name for name in required if shutil.which(name) is None]
    if any(name in missing for name in ['cargo', 'rustc']):
        raise RuntimeError('The Rust toolchain is missing. Install Rust with rustup first.')
    installed = run('/sbin/ldconfig', '-p')
    missing.extend(name for name in WINDOW_LIBS if name not in installed)
    if not any(Path(p).is_dir() for p in ['/usr/share/fonts/truetype/dejavu', '/usr/share/fonts/dejavu']):
        missing.append('DejaVu fonts')
    if missing:
        packages = ['librsvg2-bin', 'binutils', 'fonts-dejavu-core', 'libx11-6', 'libx11-xcb1',
            'libxcb1', 'libxcursor1', 'libxrandr2', 'libxi6', 'libxinerama1', 'libxrender1',
            'libxfixes3', 'libxkbcommon0', 'libxkbcommon-x11-0', 'libwayland-client0',
            'libwayland-cursor0', 'libwayland-egl1']
        if not shutil.which('apt-get') or (os.geteuid() != 0 and not shutil.which('sudo')):
            raise RuntimeError('Missing build dependencies: ' + ', '.join(missing))
        command = ([] if os.geteuid() == 0 else ['sudo']) + ['apt-get', 'install', '-y', *packages]
        print('Installing missing build dependencies.', flush=True)
        subprocess.run(command, check=True)


def stage_appdir(source, binary, directory):
    bin_dir = directory / 'usr/bin'
    bin_dir.mkdir(parents=True)
    shutil.copy2(binary, bin_dir / 'darah-eda')
    renderer = shutil.which('rsvg-convert')
    if not renderer: raise RuntimeError('rsvg-convert is missing from the build machine.')
    shutil.copy2(Path(renderer).resolve(), bin_dir / 'rsvg-convert')
    desktop = source / 'packaging/darah-eda.desktop'
    icon = source / 'packaging/darah-eda.svg'
    apps = directory / 'usr/share/applications'; apps.mkdir(parents=True)
    icons = directory / 'usr/share/icons/hicolor/scalable/apps'; icons.mkdir(parents=True)
    shutil.copy2(desktop, apps / desktop.name); shutil.copy2(desktop, directory / desktop.name)
    shutil.copy2(icon, icons / icon.name); shutil.copy2(icon, directory / icon.name)
    (directory / '.DirIcon').symlink_to(icon.name)
    if (source / 'LICENSE').is_file():
        license_dir = directory / 'usr/share/licenses/darah-eda'; license_dir.mkdir(parents=True)
        shutil.copy2(source / 'LICENSE', license_dir / 'LICENSE')


def make_appimage(source, binary, version, arch):
    target = source / f'target/dist/{version}/darah-eda-{version}-linux-{arch}.AppImage'
    work_root = source / 'target/release-tools'; work_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='appimage-', dir=work_root) as name:
        work = Path(name); appdir = work / 'Darah.AppDir'
        stage_appdir(source, binary, appdir)
        bundle(appdir)
        packed = work / target.name
        package(appdir, packed, source / 'target/appimage/tools')
        verify(packed)
        atomic_copy_artifact(packed, target)
    return target


def build_project(source, version=None, force=False):
    import platform
    source = source_directory(source)
    if version: set_version(source, version)
    version = tomllib.loads((source / 'Cargo.toml').read_text())['package']['version']
    arch = platform.machine()
    if arch not in TOOL_SHA256: raise RuntimeError(f'Unsupported build architecture: {arch}')
    state = source / 'target/release-tools'; state.mkdir(parents=True, exist_ok=True)
    image = source / f'target/dist/{version}/darah-eda-{version}-linux-{arch}.AppImage'
    record_path = image.parent / 'build.json'
    with (state / 'lock').open('a') as lock:
        try: fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError: raise RuntimeError('Another Darah build or release is running.')
        before = fingerprint(source)
        if not force and image.is_file() and record_path.is_file():
            record = json.loads(record_path.read_text())
            if record.get('fingerprint') == before and record.get('sha256') == sha256_file(image):
                print(f'Reusing verified build: {image}', flush=True)
                return image
        build_dependencies()
        env = os.environ.copy()
        env.update(DARAH_PACKAGE_KIND='appimage', DARAH_UPDATE_BASE_URL=os.environ.get('DARAH_UPDATE_BASE_URL', PAGES))
        flags = env.get('CARGO_ENCODED_RUSTFLAGS')
        flags = flags.split('\x1f') if flags else shlex.split(env.get('RUSTFLAGS', ''))
        cargo_home = Path(env.get('CARGO_HOME', Path.home() / '.cargo')).resolve()
        flags.extend([f'--remap-path-prefix={source}=/usr/src/darah-eda', f'--remap-path-prefix={cargo_home}=/cargo'])
        env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
        print(f'Testing and building Darah {version} ({arch}).', flush=True)
        subprocess.run(['cargo', '+stable', 'test', '--locked'], cwd=source, env=env, check=True)
        subprocess.run(['cargo', '+stable', 'build', '--release', '--locked'], cwd=source, env=env, check=True)
        cargo_target = Path(env.get('CARGO_TARGET_DIR', source / 'target'))
        if not cargo_target.is_absolute(): cargo_target = source / cargo_target
        image = make_appimage(source, cargo_target / 'release/darah-eda', version, arch)
        if fingerprint(source) != before: raise RuntimeError('Source files changed during the build; rerun the release command.')
        write_json(record_path, {'version': version, 'architecture': arch, 'fingerprint': before,
            'sha256': sha256_file(image), 'checks': 'cargo tests, native resolution, PNG/PDF in auto and bundled runtime'})
        print(f'READY: {image}', flush=True)
        return image


FLATPAK_ID = 'io.github.citrineio.DarahEDA'
FLATHUB = 'https://flathub.org/repo/flathub.flatpakrepo'


def flatpak_dependencies(source):
    import platform
    missing = [name for name in ['flatpak', 'flatpak-builder'] if not shutil.which(name)]
    if missing:
        if not shutil.which('apt-get') or (os.geteuid() != 0 and not shutil.which('sudo')):
            raise RuntimeError('Missing Flatpak build tools: ' + ', '.join(missing))
        prefix = [] if os.geteuid() == 0 else ['sudo']
        subprocess.run([*prefix, 'apt-get', 'install', '-y', 'flatpak', 'flatpak-builder'], check=True)
    manifest_path = source / f'packaging/flatpak/{FLATPAK_ID}.json'
    if not manifest_path.is_file():
        raise RuntimeError('The selected source project is missing its Flatpak manifest: ' + str(manifest_path))
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('id') != FLATPAK_ID:
        raise RuntimeError('The Flatpak manifest has an unexpected application ID.')
    branch = manifest['runtime-version']
    arch = platform.machine()
    if arch not in TOOL_SHA256: raise RuntimeError(f'Unsupported Flatpak architecture: {arch}')
    subprocess.run(['flatpak', 'remote-add', '--user', '--if-not-exists', 'flathub', FLATHUB], check=True)
    refs = [f'runtime/{name}/{arch}/{branch}' for name in
        [manifest['runtime'], manifest['sdk'], *manifest.get('sdk-extensions', [])]]
    missing = [ref for ref in refs if subprocess.run(['flatpak', 'info', '--user', ref],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode]
    if missing:
        print('Installing the Flatpak runtime and SDK dependencies (first setup may download several GB).', flush=True)
        subprocess.run(['flatpak', 'install', '--user', '--assumeyes', 'flathub', *missing], check=True)
    return manifest


def build_flatpak(source, force=False):
    import platform
    source = source_directory(source)
    version = tomllib.loads((source / 'Cargo.toml').read_text())['package']['version']
    arch = platform.machine()
    if arch not in TOOL_SHA256: raise RuntimeError(f'Unsupported Flatpak architecture: {arch}')
    artifact = source / f'target/dist/{version}/darah-eda-{version}-linux-{arch}.flatpak'
    record_path = artifact.parent / 'flatpak-build.json'
    state = source / 'target/flatpak'; state.mkdir(parents=True, exist_ok=True)
    with (state / 'build.lock').open('a') as lock:
        try: fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError: raise RuntimeError('Another Darah Flatpak build is running.')
        before = fingerprint(source)
        if not force and artifact.is_file() and artifact.stat().st_size and record_path.is_file():
            record = json.loads(record_path.read_text())
            if record.get('fingerprint') == before and record.get('sha256') == sha256_file(artifact):
                print(f'Reusing verified Flatpak: {artifact}', flush=True)
                return artifact
        flatpak_dependencies(source)
        with tempfile.TemporaryDirectory(prefix='staging-', dir=state) as name:
            work = Path(name); copied = work / 'source'; (copied / '.cargo').mkdir(parents=True)
            for item in ['Cargo.toml', 'Cargo.lock', 'build.rs', 'LICENSE']:
                shutil.copy2(source / item, copied / item)
            for directory in ['src', 'packaging']:
                shutil.copytree(source / directory, copied / directory)
            sync_flatpak_metadata(copied, version)
            manifest = work / f'{FLATPAK_ID}.json'
            shutil.copy2(source / f'packaging/flatpak/{FLATPAK_ID}.json', manifest)
            with (copied / '.cargo/config.toml').open('w') as config:
                subprocess.run(['cargo', '+stable', 'vendor', '--locked', 'vendor'], cwd=copied,
                    stdout=config, check=True)
            build = work / 'build'; repository = work / 'repo'
            subprocess.run(['flatpak-builder', '--user', '--disable-rofiles-fuse',
                f'--state-dir={state / "builder-state"}', f'--repo={repository}',
                '--default-branch=stable', str(build), str(manifest)], cwd=source, check=True)
            # --runtime mounts the production platform rather than the SDK.
            result = subprocess.run(['flatpak', 'build', '--runtime', str(build),
                '/usr/bin/ldd', '/app/bin/darah-eda'], capture_output=True, text=True, check=True)
            if 'not found' in result.stdout + result.stderr:
                raise RuntimeError('The Flatpak app has unresolved native dependencies in its production runtime.')
            svg = b'<svg xmlns="http://www.w3.org/2000/svg" width="160" height="80"><rect width="160" height="80" fill="#12685b"/><text x="8" y="44" fill="white" font-size="16">Darah Flatpak</text></svg>'
            for fmt, signature in [('png', b'\x89PNG\r\n\x1a\n'), ('pdf', b'%PDF-')]:
                result = subprocess.run(['flatpak', 'build', '--runtime', str(build),
                    '/app/bin/rsvg-convert', f'--format={fmt}'], input=svg, capture_output=True, check=True)
                if not result.stdout.startswith(signature):
                    raise RuntimeError(f'The Flatpak runtime produced an invalid {fmt} export.')
            bundle = work / artifact.name
            subprocess.run(['flatpak', 'build-bundle', f'--arch={arch}', f'--runtime-repo={FLATHUB}',
                str(repository), str(bundle), FLATPAK_ID, 'stable'], check=True)
            if not bundle.is_file() or not bundle.stat().st_size:
                raise RuntimeError('Flatpak did not produce a nonempty bundle.')
            if fingerprint(source) != before:
                raise RuntimeError('Source files changed during the Flatpak build; rerun the release command.')
            atomic_copy_artifact(bundle, artifact)
        write_json(record_path, {'version': version, 'architecture': arch,
            'fingerprint': before, 'sha256': sha256_file(artifact),
            'checks': 'native dependency resolution and PNG/PDF export in the production Flatpak runtime'})
        print(f'PASS: Flatpak native dependencies and PNG/PDF export.\nREADY: {artifact}', flush=True)
        return artifact


def build_release(source, version=None, force=False):
    source = source_directory(source)
    if version: set_version(source, version)
    # Set up missing Flatpak dependencies before spending time on either build.
    flatpak_dependencies(source)
    appimage = build_project(source, force=force)
    flatpak = build_flatpak(source, force=force)
    return [appimage, flatpak]


def main():
    import sys
    argv = sys.argv[1:]
    if argv[:1] == ['build']: argv = argv[1:]
    parser = argparse.ArgumentParser(description='Build, test, and package Darah. This never publishes.')
    parser.add_argument('source', nargs='?', default='.', type=Path)
    parser.add_argument('--version', help='set a release version before building')
    parser.add_argument('--force', action='store_true', help='rebuild even when a verified matching build is cached')
    parser.add_argument('--appimage-only', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    if args.appimage_only: build_project(args.source, args.version, args.force)
    else: build_release(args.source, args.version, args.force)


if __name__ == '__main__':
    try: main()
    except Exception as error:
        print(f'Release stopped: {error}', file=sys.stderr)
        sys.exit(1)
