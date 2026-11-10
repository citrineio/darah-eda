#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$project_dir"

configure_maintainer=false
case "${1:-}" in
    '') ;;
    --configure-maintainer) configure_maintainer=true ;;
    --help)
        echo 'Usage: bash scripts/build-deb.sh [--configure-maintainer]'
        echo 'DARAH_MAINTAINER overrides and updates the saved Name <email>.'
        exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
esac
if (( $# > 1 )); then
    echo 'Pass at most one option. See --help.' >&2
    exit 1
fi

valid_maintainer() {
    [[ "$1" != *$'\n'* && "$1" != *$'\r'* && "$1" =~ ^[^\<]+\ \<[^\ \<\>]+@[^\ \<\>]+\>$ ]]
}

maintainer_dir="${XDG_CONFIG_HOME:-$HOME/.config}/darah-eda"
maintainer_file="$maintainer_dir/package-maintainer"
saved_maintainer=''
if [[ -f "$maintainer_file" ]]; then
    IFS= read -r saved_maintainer < "$maintainer_file" || true
fi
maintainer=${DARAH_MAINTAINER:-}
if [[ -z "$maintainer" && "$configure_maintainer" == false ]]; then
    if [[ -n "$saved_maintainer" ]]; then
        maintainer=$saved_maintainer
    else
        # Recover the identity already supplied for a previous Darah build.
        candidate=$(dpkg-deb --field target/debian/darah-eda.deb Maintainer 2>/dev/null || true)
        if valid_maintainer "$candidate"; then
            maintainer=$candidate
        elif command -v dpkg-query >/dev/null 2>&1; then
            candidate=$(dpkg-query -W -f='${Maintainer}' darah-eda 2>/dev/null || true)
            if valid_maintainer "$candidate"; then maintainer=$candidate; fi
        fi
    fi
fi
if [[ -z "$maintainer" ]]; then
    if [[ ! -t 0 ]]; then
        echo 'Set DARAH_MAINTAINER to your real Name <email> before building.' >&2
        exit 1
    fi
    read -r -p 'Package maintainer (Name <email>): ' maintainer
fi
if ! valid_maintainer "$maintainer"; then
    echo 'Use a maintainer name followed by a real email in angle brackets.' >&2
    echo 'Set DARAH_MAINTAINER or run with --configure-maintainer to replace saved details.' >&2
    exit 1
fi

if [[ "$maintainer" != "$saved_maintainer" ]]; then
    (
        umask 077
        mkdir -p -- "$maintainer_dir"
        maintainer_temp=$(mktemp "$maintainer_dir/.package-maintainer-XXXXXX")
        trap 'rm -f -- "$maintainer_temp"' EXIT
        printf '%s\n' "$maintainer" > "$maintainer_temp"
        mv -f -- "$maintainer_temp" "$maintainer_file"
    )
fi

mkdir -p target/debian
DARAH_PACKAGE_KIND=deb cargo +stable deb --maintainer "$maintainer" --output target/debian/darah-eda.deb
dpkg-deb --field target/debian/darah-eda.deb Package Version Architecture Maintainer
echo 'Created target/debian/darah-eda.deb'
