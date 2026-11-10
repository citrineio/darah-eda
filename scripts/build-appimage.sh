#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$project_dir"
architecture=$(uname -m)
case "$architecture" in
    x86_64|aarch64) ;;
    *) echo "Unsupported AppImage build architecture: $architecture" >&2; exit 1 ;;
esac

linuxdeploy=${LINUXDEPLOY:-"./linuxdeploy-$architecture.AppImage"}
if [[ ! -x "$linuxdeploy" ]]; then
    echo "Set LINUXDEPLOY to an executable linuxdeploy for $architecture." >&2
    exit 1
fi
linuxdeploy=$(realpath -- "$linuxdeploy")
DARAH_PACKAGE_KIND=appimage cargo +stable build --release
mkdir -p target/appimage
staging_dir=$(mktemp -d "$project_dir/target/appimage/staging-XXXXXX")
trap 'rm -rf -- "$staging_dir"' EXIT

cd target/appimage
APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy"     --appdir "$staging_dir/Darah.AppDir"     --executable "$project_dir/target/release/darah-eda"     --desktop-file "$project_dir/packaging/darah-eda.desktop"     --icon-file "$project_dir/packaging/darah-eda.svg"     --output appimage
