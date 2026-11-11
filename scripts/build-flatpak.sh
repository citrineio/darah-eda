#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
app=io.github.citrineio.DarahEDA
architecture=$(uname -m)
case "$architecture" in
    x86_64|aarch64) ;;
    *) echo "Unsupported Flatpak build architecture: $architecture" >&2; exit 1 ;;
esac

version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
mkdir -p target/flatpak
work=$(mktemp -d "$PWD/target/flatpak/staging-XXXXXX")
trap 'rm -rf -- "$work"' EXIT

mkdir -p "$work/source/.cargo"
cp -- Cargo.toml Cargo.lock build.rs LICENSE "$work/source/"
cp -R -- packaging src "$work/source/"
cp -- "packaging/flatpak/$app.json" "$work/$app.json"
(
    cd "$work/source"
    cargo +stable vendor --locked vendor > .cargo/config.toml
)

flatpak-builder --user --install-deps-from=flathub --state-dir="$work/state" \
    --repo="$work/repo" --default-branch=stable "$work/build" "$work/$app.json"

# Check the renderer against the production runtime before handing off the bundle.
flatpak build --runtime \
    "$work/build" /app/bin/rsvg-convert --version
bundle="darah-eda-$version-linux-$architecture.flatpak"
flatpak build-bundle --arch="$architecture" \
    --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo \
    "$work/repo" "$work/$bundle" "$app" stable
mv -- "$work/$bundle" "target/flatpak/$bundle"
printf '%s\n' "$PWD/target/flatpak/$bundle"
