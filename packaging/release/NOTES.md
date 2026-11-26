Fork of [dslapelis/linuxmeeter](https://github.com/dslapelis/linuxmeeter).

## New in 0.3.0

- **Add and remove strips and buses**: "+" beside Hardware / Virtual / Buses, × in a strip's header. Up to 16 strips; buses A1–A4 (outputs) and B1–B3 (virtual mics).
- **Virtual devices show up in KDE's volume applet** (they were hidden as "virtual devices").
- **EQ with up to 12 points** per strip: double-click the curve to add one; peak, shelves, low cut and high cut.
- **Gain-reduction displays**: under every bus limiter, and live in the dynamics panel (input level on the compressor curve, compressor reduction, gate open/closed).
- **Global shortcuts set inside the app** (Settings → Global shortcuts) for mute, ±5 %, mute all microphones and next/previous profile. On KDE Plasma they work everywhere, also with the window hidden.
- **Tray menu**: mute all microphones, switch profile.
- **Backup**: export / import every profile and setting as one file (Settings, or `linuxmeeter ctl export|import <file>`).

## Since 0.2.0

Rename strips and buses (the name other programs see changes too), profile switching, desktop notifications, `linuxmeeter ctl` for scripts.

## Install (Arch)

    sudo pacman -U linuxmeeter-*.pkg.tar.zst

Needs PipeWire (`pipewire-audio`) with WirePlumber, `lsp-plugins-lv2`, `webkit2gtk-4.1`, `gtk3`, `libayatana-appindicator`; pacman installs them.
