#!/usr/bin/env bash
# Build a .deb from an already-built release binary (target/release/linuxmeeter).
# Run on the Debian release you are packaging for, from the repository root:
#
#   pnpm tauri build --no-bundle && packaging/debian/build-deb.sh 0.3.0
#
# Library dependencies come from dpkg-shlibdeps; the runtime pieces that are
# loaded rather than linked (PipeWire's filter-chain and SPA plugins, the LSP
# LV2 plugins, the tray library) are listed by hand.
set -euo pipefail

VERSION=${1:?usage: build-deb.sh VERSION}
ARCH=$(dpkg --print-architecture)
ROOT=$(pwd)
OUT=${OUT:-$ROOT/target/debian}
PKG=$OUT/linuxmeeter_${VERSION}_${ARCH}

rm -rf "$PKG"
install -Dm755 target/release/linuxmeeter "$PKG/usr/bin/linuxmeeter"
install -Dm644 packaging/linuxmeeter.desktop "$PKG/usr/share/applications/linuxmeeter.desktop"
install -Dm644 src-tauri/icons/icon.png "$PKG/usr/share/icons/hicolor/512x512/apps/linuxmeeter.png"
install -Dm644 src-tauri/icons/128x128.png "$PKG/usr/share/icons/hicolor/128x128/apps/linuxmeeter.png"
install -Dm644 src-tauri/icons/32x32.png "$PKG/usr/share/icons/hicolor/32x32/apps/linuxmeeter.png"
install -Dm644 LICENSE "$PKG/usr/share/doc/linuxmeeter/copyright"

# dpkg-shlibdeps wants a debian/control next to it.
WORK=$(mktemp -d)
mkdir -p "$WORK/debian"
printf 'Source: linuxmeeter\n\nPackage: linuxmeeter\nArchitecture: any\n' > "$WORK/debian/control"
SHLIBS=$(cd "$WORK" && dpkg-shlibdeps -O "$PKG/usr/bin/linuxmeeter" 2>/dev/null | sed -n 's/^shlibs:Depends=//p')
rm -rf "$WORK"

RUNTIME="pipewire, wireplumber, libspa-0.2-modules, lsp-plugins-lv2, libayatana-appindicator3-1"
SIZE=$(du -sk "$PKG/usr" | cut -f1)

mkdir -p "$PKG/DEBIAN"
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: linuxmeeter
Version: $VERSION
Architecture: $ARCH
Maintainer: linuxmeeter fork <noreply@users.noreply.github.com>
Installed-Size: $SIZE
Depends: $SHLIBS, $RUNTIME
Recommends: pipewire-pulse
Section: sound
Priority: optional
Homepage: https://github.com/Schnubbirunkel/linuxmeeter
Description: VoiceMeeter-style virtual audio mixer for PipeWire
 Route programs and microphones through gate, compressor and EQ into
 hardware outputs and virtual microphones, with profiles, global
 shortcuts and gain-reduction displays.
CONTROL

dpkg-deb --root-owner-group --build "$PKG" "$PKG.deb" >/dev/null
echo "$PKG.deb"
