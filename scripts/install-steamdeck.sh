#!/usr/bin/env bash
set -euo pipefail

app_id=io.github.ik3rurru.WindowDeck
script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# Works both in the source checkout and in the extracted release package.
assets_dir=$script_dir
if [[ -d "$script_dir/../packaging/steamdeck" ]]; then
    assets_dir=$script_dir/../packaging/steamdeck
fi

usage() {
    printf 'Uso: bash install-steamdeck.sh [RUTA.flatpak | --shortcuts-only]\n'
}
if (( $# > 1 )); then usage >&2; exit 2; fi
case "${1:-}" in
    --help|-h) usage; exit 0 ;;
    --shortcuts-only) shortcuts_only=true ;;
    --*) usage >&2; exit 2 ;;
    *) shortcuts_only=false ;;
esac
command -v flatpak >/dev/null || { printf 'Falta Flatpak. Ejecuta este instalador en la Steam Deck.\n' >&2; exit 1; }
[[ -f "$assets_dir/windowdeck" ]] || { printf 'Extrae el paquete completo antes de instalar.\n' >&2; exit 1; }

if [[ $shortcuts_only == false ]]; then
    bundle=${1:-$script_dir/WindowDeck.flatpak}
    [[ -f "$bundle" ]] || { printf 'No existe el paquete: %s\n' "$bundle" >&2; exit 1; }
    bundle=$(CDPATH= cd -- "$(dirname -- "$bundle")" && printf '%s/%s' "$PWD" "$(basename -- "$bundle")")
    flatpak install --user --bundle "$bundle"
fi
# Flatpak exports the desktop entry only after a successful installation.
if flatpak info --user "$app_id" >/dev/null 2>&1; then
    install_scope=--user
elif [[ $shortcuts_only == true ]] && flatpak info --system "$app_id" >/dev/null 2>&1; then
    install_scope=--system
else
    printf 'WindowDeck no está instalado. No se han creado accesos.\n' >&2
    exit 1
fi
app_location=$(flatpak info "$install_scope" --show-location "$app_id")
desktop_entry=$app_location/export/share/applications/$app_id.desktop
[[ -f "$desktop_entry" ]] || { printf 'Falta la entrada de escritorio de WindowDeck.\n' >&2; exit 1; }

install -Dm0755 -- "$assets_dir/windowdeck" "$HOME/.local/bin/windowdeck"
desktop_dir=''
if command -v xdg-user-dir >/dev/null; then
    desktop_dir=$(xdg-user-dir DESKTOP)
elif [[ -d "$HOME/Desktop" ]]; then
    desktop_dir=$HOME/Desktop
fi
if [[ -n "$desktop_dir" && "$desktop_dir" != "$HOME" && "$desktop_dir" == /* ]]; then
    install -Dm0755 -- "$desktop_entry" "$desktop_dir/WindowDeck.desktop"
    printf 'Acceso directo: %s/WindowDeck.desktop\n' "$desktop_dir"
else
    printf 'Escritorio desactivado o no disponible; usa WindowDeck desde el menú.\n'
fi
printf 'Lanzador para Steam: %s/.local/bin/windowdeck\n' "$HOME"
printf 'En Steam, añade WindowDeck como producto que no es de Steam, sin forzar Proton.\n'
