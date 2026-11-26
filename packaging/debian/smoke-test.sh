#!/usr/bin/env bash
# Desktop-like smoke test of an installed linuxmeeter: PipeWire + WirePlumber,
# the app under Xvfb, a tone played into the Chat strip and recorded from the
# B1 virtual microphone, through the real LSP plugins. Run inside
# dbus-run-session. Exits non-zero if the tone does not arrive.
set -u
T=$(mktemp -d)
export XDG_RUNTIME_DIR=$T/run XDG_CONFIG_HOME=$T/config DISPLAY=:99
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME/pipewire/pipewire.conf.d"
chmod 700 "$XDG_RUNTIME_DIR"
# Stand-in sound card and microphone (the mixer only adopts alsa_* nodes).
cat > "$XDG_CONFIG_HOME/pipewire/pipewire.conf.d/10-smoke.conf" <<'CONF'
context.objects = [
  { factory = adapter args = { factory.name = support.null-audio-sink node.name = "alsa_output.smoke" node.description = "Smoke Speakers" media.class = Audio/Sink audio.position = [ FL FR ] object.linger = true } }
  { factory = adapter args = { factory.name = support.null-audio-sink node.name = "alsa_input.smoke" node.description = "Smoke Mic" media.class = Audio/Source audio.position = [ FL FR ] object.linger = true } }
]
CONF
# No bluetooth in a container; its seat monitoring needs logind, which is missing.
mkdir -p "$XDG_CONFIG_HOME/wireplumber/bluetooth.lua.d" "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d"
echo "-- disabled for the smoke test" > "$XDG_CONFIG_HOME/wireplumber/bluetooth.lua.d/90-enable-all.lua"   # WirePlumber 0.4
printf 'wireplumber.profiles = {\n  main = {\n    monitor.bluez = disabled\n    monitor.bluez-midi = disabled\n  }\n}\n' \
  > "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d/90-smoke.conf"                                          # WirePlumber 0.5
python3 - "$T/tone.wav" <<'PY'
import math, struct, sys, wave
w = wave.open(sys.argv[1], "wb"); w.setnchannels(2); w.setsampwidth(2); w.setframerate(48000)
w.writeframes(b"".join(struct.pack("<hh", *(2 * [int(16384 * math.sin(2 * math.pi * 1000 * i / 48000))])) for i in range(48000 * 8)))
w.close()
PY

Xvfb :99 >/dev/null 2>&1 &
pipewire >"$T/pipewire.log" 2>&1 &
sleep 1
wireplumber >"$T/wireplumber.log" 2>&1 &
sleep 2
linuxmeeter >"$T/app.log" 2>&1 &
for _ in $(seq 60); do linuxmeeter ctl list >/dev/null 2>&1 && break; sleep 0.5; done

status=0
echo "--- mixer"; linuxmeeter ctl list || status=1
echo "--- devices other programs see"
pw-cli ls Node | grep -E 'node.(name|description) = "lm\.(strip|bus)\.[0-9a-z]+"' -A0 | sed 's/^\s*/  /'

pw-record --target lm.bus.b1 --rate 48000 --channels 2 "$T/rec.wav" >/dev/null 2>&1 &
REC=$!
sleep 1
pw-play --target lm.strip.4 "$T/tone.wav" >/dev/null 2>&1 &
PLAY=$!
sleep 3
pw-link -l > "$T/links.txt" 2>&1
sleep 2
kill $PLAY $REC 2>/dev/null; sleep 1

python3 - "$T/rec.wav" <<'PY' || status=1
import math, struct, sys, wave
w = wave.open(sys.argv[1]); n = w.getnframes(); data = w.readframes(n)
fmt = {2: "h", 4: "f"}[w.getsampwidth()]
vals = struct.unpack("<%d%s" % (len(data) // w.getsampwidth(), fmt), data)
scale = 32768.0 if fmt == "h" else 1.0
peak = max((abs(v) / scale for v in vals), default=0)
db = 20 * math.log10(peak) if peak > 0 else -120
print(f"--- tone at B1: peak {db:.1f} dBFS over {n} frames")
# -6 dBFS in; the bus limiter sits at -1 dBFS, so expect roughly -6.
sys.exit(0 if -12 < db < 0 else 1)
PY

# Microphone warning tone: muting the mics plays two notes on the speakers.
pw-record -P stream.capture.sink=true --target alsa_output.smoke --rate 48000 --channels 2 "$T/beep.wav" >/dev/null 2>&1 &
REC=$!
sleep 1
linuxmeeter ctl toggle-mute-mics || status=1
sleep 1.5
kill $REC 2>/dev/null; sleep 0.5
python3 - "$T/beep.wav" <<'PY' || status=1
import math, struct, sys, wave
w = wave.open(sys.argv[1]); data = w.readframes(w.getnframes())
vals = struct.unpack("<%dh" % (len(data) // 2), data)
peak = max((abs(v) / 32768 for v in vals), default=0)
db = 20 * math.log10(peak) if peak > 0 else -120
print(f"--- warning tone on the speakers: peak {db:.1f} dBFS")
sys.exit(0 if -24 < db < -6 else 1)
PY

if [ $status -ne 0 ]; then
  echo "--- app log"; tail -40 "$T/app.log"
  echo "--- pipewire log"; tail -20 "$T/pipewire.log"
  echo "--- links while playing"; cat "$T/links.txt"
fi
pkill -f "linuxmeeter$" ; pkill wireplumber; pkill -x pipewire; pkill Xvfb
exit $status
