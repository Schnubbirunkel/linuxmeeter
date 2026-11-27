Fork of [dslapelis/linuxmeeter](https://github.com/dslapelis/linuxmeeter).

## New in 0.5.2

- **Volume keys per profile**: Settings → Volume keys picks which hardware output the keyboard's volume keys and a headset's volume wheel control in the current profile — onboard sound, USB headset, HDMI … — or, as before, whatever the desktop uses. Switch profile and the keys follow. With DEFAULT OUT on, the keys used to move linuxmeeter's System strip; now they can turn the headset itself up and down, with KDE's volume display. On KDE Plasma linuxmeeter takes the keys over from Plasma's volume control while a profile has a main output, and gives them back when it has none or linuxmeeter quits (also on logout). `linuxmeeter ctl volume-keys <output|default>` and `ctl device-volume up|down|mute` do the same from scripts or other desktops.

## New in 0.5.1

- **Clock helper for crackling / robot voice**: when devices play or record together, one sets the clock and the others follow. Some — wireless USB headsets above all — cannot follow cleanly and crackle or sound like a robot. linuxmeeter now notices (it reads the same error counters as `pw-top`), shows a banner and a desktop notification, and with one click lets that device set the clock instead (a WirePlumber rule; PipeWire and linuxmeeter restart). Settings → Clock lists these devices and can undo it.

## New in 0.5.0

- **Phone remote**: Settings → Phone remote → scan the QR code. Your phone's browser becomes a remote: faders, mute, "microphones live/muted" and profiles for every strip and bus, with live level meters. No app needed. Only devices on your home network that scanned the code (it contains a random key) can connect; "New code" locks out every phone that had the old one.
- **Updates from inside the app**: Settings → Updates shows your version, checks GitHub, and installs with one click (Arch and Debian; asks for your password, then restarts linuxmeeter). Choose *Releases* or *Every push* — the latter follows every change pushed to main as soon as it is built.

## New in 0.4.1

- **Fix: the microphone warning tone did not play** on real hardware. The tone's stream was closed after a fixed 0.8 s — before a sound card waking from standby (or a Bluetooth headset) was ready. It now stays until the tone has actually played.
- **Bus limiter ON/OFF button** next to its knob. Moving the knob no longer switches the limiter back on.
- **Fix: buses were about 2 dB quieter than set.** LSP's limiter defaults (automatic level regulation and gain boost) changed the level even far below the threshold. Below the threshold the limiter is now transparent; it only acts on peaks above it.

## New in 0.4.0

- **Debian package**: `linuxmeeter_0.4.0_amd64.deb` for Debian 13 (trixie). Built, installed with apt and smoke-tested on trixie with PipeWire, WirePlumber and the real LSP plugins.
- **Microphone warning tone**: two short notes on your speakers when the microphones are muted (falling) or go live again (rising) — from the tray, a shortcut, the command line or the M button. Played straight to the speakers, never into a virtual mic. Can be turned off in Settings.
- **Fix: stereo could collapse to mono (+6 dB)**: when PipeWire reported no channel names on a route, every output was linked to every input. Channels are now always declared and paired correctly.
- **Fix: profile-switch notifications on KDE**: Plasma silently swallowed notifications that replaced an expired one, so later switches showed nothing.

## In 0.3.0

Add/remove strips and buses (up to 16 strips, A1–A4, B1–B3), devices visible in KDE's volume applet, EQ with up to 12 points incl. low/high cut, gain-reduction displays for limiter, compressor and gate, global shortcuts set inside the app, tray menu (mute all mics, profiles), backup export/import.

## Install

**Arch Linux**

    sudo pacman -U linuxmeeter-*-x86_64.pkg.tar.zst

**Debian 13 (trixie)**

    sudo apt install ./linuxmeeter_*_amd64.deb

Both pull in PipeWire, WirePlumber, `lsp-plugins-lv2` and the WebKitGTK / tray libraries.
