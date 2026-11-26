Fork of [dslapelis/linuxmeeter](https://github.com/dslapelis/linuxmeeter).

## New in 0.4.0

- **Debian package**: `linuxmeeter_0.4.0_amd64.deb` for Debian 13 (trixie). Built, installed with apt and smoke-tested on trixie with PipeWire, WirePlumber and the real LSP plugins.
- **Microphone warning tone**: two short notes on your speakers when the microphones are muted (falling) or go live again (rising) — from the tray, a shortcut, the command line or the M button. Played straight to the speakers, never into a virtual mic. Can be turned off in Settings.
- **Fix: stereo could collapse to mono (+6 dB)**: when PipeWire reported no channel names on a route, every output was linked to every input. Channels are now always declared and paired correctly.
- **Fix: profile-switch notifications on KDE**: Plasma silently swallowed notifications that replaced an expired one, so later switches showed nothing.

## In 0.3.0

Add/remove strips and buses (up to 16 strips, A1–A4, B1–B3), devices visible in KDE's volume applet, EQ with up to 12 points incl. low/high cut, gain-reduction displays for limiter, compressor and gate, global shortcuts set inside the app, tray menu (mute all mics, profiles), backup export/import.

## Install

**Arch Linux**

    sudo pacman -U linuxmeeter-0.4.0-1-x86_64.pkg.tar.zst

**Debian 13 (trixie)**

    sudo apt install ./linuxmeeter_0.4.0_amd64.deb

Both pull in PipeWire, WirePlumber, `lsp-plugins-lv2` and the WebKitGTK / tray libraries.
