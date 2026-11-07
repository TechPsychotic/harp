#!/bin/sh
# Installs a Harp Linux bundle for the current user, with a menu entry that
# desktops recognise as a music player (categories, audio MIME types, icons,
# window class). Usage: scripts/install-linux.sh <bundle dir or harp-linux-x64.tar.gz>
# libmpv is needed at run time (Fedora: mpv-libs, Debian/Ubuntu: libmpv2).
set -eu
SRC=${1:?usage: install-linux.sh <bundle dir or tar.gz>}
HERE=$(cd "$(dirname "$0")/.." && pwd)
D=$HOME/.local/opt/harp
rm -rf "$D" && mkdir -p "$D" "$HOME/.local/bin" "$HOME/.local/share/applications"
if [ -d "$SRC" ]; then cp -r "$SRC"/. "$D"/; else tar -xzf "$SRC" -C "$D"; fi
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
