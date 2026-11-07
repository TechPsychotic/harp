#!/bin/sh
# Installs a Harp Linux bundle for the current user, with a menu entry that
# desktops recognise as a music player (categories, audio MIME types, icons,
# window class). Usage: scripts/install-linux.sh <bundle dir or harp-linux-x64.tar.gz>
# libmpv is needed at run time (Fedora: mpv-libs, Debian/Ubuntu: libmpv2).
set -eu
SRC=${1:?usage: install-linux.sh <bundle dir or tar.gz>}
HERE=$(cd "$(dirname "$0")/.." && pwd)
D=$HOME/.local/opt/harp
# Unpack to a temporary folder first and only replace the old install once that worked.
NEW=$(mktemp -d "$HOME/.local/opt/.harp-new.XXXXXX" 2>/dev/null || { mkdir -p "$HOME/.local/opt" && mktemp -d "$HOME/.local/opt/.harp-new.XXXXXX"; })
trap 'rm -rf "$NEW"' EXIT
if [ -d "$SRC" ]; then cp -r "$SRC"/. "$NEW"/; else tar -xzf "$SRC" -C "$NEW"; fi
[ -x "$NEW/harp" ] || { echo "install-linux: no harp executable in $SRC; leaving the current install alone" >&2; exit 1; }
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
rm -rf "$D" && mv "$NEW" "$D"
# A bundled libmpv (if you have one extracted next to the app) is picked up via the launcher.
cat > "$HOME/.local/bin/harp" <<LAUNCH
#!/bin/sh
D="\$HOME/.local/opt/harp"
export LD_LIBRARY_PATH="\$D/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
exec "\$D/harp" "\$@"
LAUNCH
chmod +x "$HOME/.local/bin/harp"
cp -r "$HERE/dist/linux/icons/." "$HOME/.local/share/icons/" 2>/dev/null || { mkdir -p "$HOME/.local/share/icons" && cp -r "$HERE/dist/linux/icons/." "$HOME/.local/share/icons/"; }
sed "s#^Exec=harp#Exec=$HOME/.local/bin/harp#" "$HERE/dist/linux/dev.harp.harp.desktop" > "$HOME/.local/share/applications/dev.harp.harp.desktop"
rm -f "$HOME/.local/share/applications/harp.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "Harp installed in $D"
