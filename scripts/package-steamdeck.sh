#!/usr/bin/env bash
set -euo pipefail
project_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$project_root"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml)
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Invalid workspace version' >&2; exit 1; }
bundle=${1:-WindowDeck.flatpak}
[[ -f "$bundle" ]] || { printf 'Missing bundle: %s\n' "$bundle" >&2; exit 1; }
mkdir -p target
stage=$(mktemp -d "$project_root/target/steamdeck-package.XXXXXXXX")
package=WindowDeck-$version-steamdeck-x86_64
mkdir "$stage/$package"
cp -- "$bundle" "$stage/$package/WindowDeck.flatpak"
cp -- scripts/install-steamdeck.sh packaging/steamdeck/windowdeck packaging/steamdeck/README.md "$stage/$package/"
chmod 755 "$stage/$package/install-steamdeck.sh" "$stage/$package/windowdeck"
git rev-parse HEAD > "$stage/$package/SOURCE_COMMIT.txt"
(cd "$stage/$package" && sha256sum WindowDeck.flatpak install-steamdeck.sh windowdeck README.md SOURCE_COMMIT.txt > SHA256SUMS.txt)
tar -czf "$stage/$package.tar.gz" -C "$stage" "$package"
printf '%s\n' "$stage/$package.tar.gz"
