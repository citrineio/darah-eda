#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
package=${1:-"$project_dir/target/debian/darah-eda.deb"}
if [[ ! -f "$package" ]]; then
    echo "Package does not exist: $package" >&2
    exit 1
fi

# APT's _apt user needs access to the package and every parent directory.
staging_dir=$(mktemp -d /tmp/darah-install-XXXXXX)
trap 'rm -rf -- "$staging_dir"' EXIT
chmod 755 "$staging_dir"
install -m 644 -- "$package" "$staging_dir/darah-eda.deb"
sudo apt install "$staging_dir/darah-eda.deb"
