#!/bin/sh
# Installe TextCraft pour l'utilisateur courant (sans droits administrateur) :
#   - le programme dans ~/.local/bin/textcraft
#   - l'icône et le raccourci dans le menu des applications
#
# Utilisation, depuis le dossier qui contient ce script et le fichier « textcraft » :
#   sh install.sh
# Si le fichier « textcraft » n'est pas là mais que Rust est installé, le script le
# fabrique depuis les sources (dossier parent).

set -e
here="$(cd "$(dirname "$0")" && pwd)"
bin="$here/textcraft"
[ -f "$bin" ] && chmod +x "$bin" 2>/dev/null || true

if [ ! -x "$bin" ]; then
    if command -v cargo >/dev/null 2>&1 && [ -f "$here/../Cargo.toml" ]; then
        echo "Fabrication de TextCraft (quelques minutes la première fois)…"
        (cd "$here/.." && cargo build --release)
        bin="$here/../target/release/textcraft"
    else
        echo "Le fichier « textcraft » est introuvable à côté de ce script." >&2
        exit 1
    fi
fi

mkdir -p "$HOME/.local/bin" \
         "$HOME/.local/share/applications" \
         "$HOME/.local/share/icons/hicolor/256x256/apps"

install -m 755 "$bin" "$HOME/.local/bin/textcraft"
"$HOME/.local/bin/textcraft" --write-icon "$HOME/.local/share/icons/hicolor/256x256/apps/textcraft.png"
sed "s|@BIN@|$HOME/.local/bin/textcraft|" "$here/textcraft.desktop" > "$HOME/.local/share/applications/textcraft.desktop"
chmod 644 "$HOME/.local/share/applications/textcraft.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$HOME/.local/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" || true

echo "TextCraft est installé : cherchez « TextCraft » dans le menu des applications,"
echo "ou tapez « textcraft » dans un terminal."
