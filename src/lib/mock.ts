/** Fake backend: realistic AppState + synthesized 30 fps meter stream.
 *
 * The simulation honors live mixer state (gain, mute, solo, routing), so bus
 * meters actually respond when you pull a fader or punch a route button —
 * design iteration feels like the real app.
 */
import type { AppState, BusId, MeterFrame, StripState } from "./types";
import type { Ipc, Target } from "./ipc";
import { powerSum } from "./utils/db";
import { BUS_IDS, isHardwareBus } from "./types";

type SimKind = "speech" | "music" | "steady";

interface Sim {
  kind: SimKind;
  base: number;
  spread: number;
  phase: number;
  noise: number;
  out: number;
}

function makeSim(kind: SimKind, base: number, spread: number, phase: number): Sim {
  return { kind, base, spread, phase, noise: 0, out: -90 };
}

function simLevel(s: Sim, t: number, dt: number): number {
  s.noise = Math.max(-3, Math.min(3, (s.noise + (Math.random() - 0.5) * dt * 40) * 0.985));
  let db: number;
  if (s.kind === "speech") {
    const active = Math.sin(t * 0.55 + s.phase) + Math.sin(t * 1.7 + s.phase * 2) * 0.5 > -0.1;
    db = active ? s.base + Math.sin(t * 6 + s.phase) * s.spread * 0.4 + s.noise * 2 : -90;
  } else if (s.kind === "music") {
    const beat = Math.pow(Math.max(0, Math.sin(t * Math.PI * 2 * 1.9 + s.phase)), 10) * 7;
    db = s.base + beat + Math.sin(t * 0.23 + s.phase) * s.spread * 0.5 + s.noise;
  } else {
    db = s.base + Math.sin(t * 0.4 + s.phase) * s.spread * 0.5 + s.noise;
  }
  s.out += (db - s.out) * Math.min(1, dt * (db > s.out ? 30 : 9));
  return s.out;
}

const defaultEq = () => ({
  enabled: false,
  bands: [
    { kind: "low_shelf" as const, freqHz: 100, gainDb: 0, q: 0.7 },
    { kind: "peak" as const, freqHz: 400, gainDb: 0, q: 1 },
    { kind: "peak" as const, freqHz: 2500, gainDb: 0, q: 1 },
    { kind: "high_shelf" as const, freqHz: 8000, gainDb: 0, q: 0.7 },
  ],
  color: [
    { kind: "low_shelf" as const, freqHz: 300, gainDb: 0, q: 0.7 },
    { kind: "peak" as const, freqHz: 400, gainDb: 0, q: 0.8 },
    { kind: "peak" as const, freqHz: 3000, gainDb: 0, q: 0.8 },
    { kind: "high_shelf" as const, freqHz: 3500, gainDb: 0, q: 0.7 },
  ],
});
const defaultGate = () => ({ enabled: false, thresholdDb: -40, attackMs: 20, releaseMs: 100, holdMs: 50 });
const defaultComp = () => ({ enabled: false, thresholdDb: -18, ratio: 4, attackMs: 20, releaseMs: 100, makeupDb: 0 });
const defaultLimiter = () => ({ enabled: true, thresholdDb: -1, releaseMs: 50 });

const routes = (on: BusId[]): Record<BusId, boolean> =>
  Object.fromEntries(BUS_IDS.map((b) => [b, on.includes(b)])) as Record<BusId, boolean>;

const state: AppState = {
  profileName: "Default",
  profiles: ["Default", "Gaming", "Recording"],
  takeDefaultOutput: false,
  strips: [
    { id: 1, kind: "hardware", label: "Mic", hwKey: "alsa_input.usb-ZOOM_Corporation_UAC-2", online: true, gainDb: -4, mute: false, solo: false, routes: routes(["A2", "B1"]), gate: defaultGate(), comp: defaultComp(), eq: defaultEq() },
    { id: 2, kind: "hardware", label: "Line In", hwKey: "alsa_input.pci-0000_31_00.4.analog-stereo", online: true, gainDb: -12, mute: false, solo: false, routes: routes(["A1"]), gate: defaultGate(), comp: defaultComp(), eq: defaultEq() },
    { id: 3, kind: "virtual", label: "System", hwKey: null, online: true, gainDb: 0, mute: false, solo: false, routes: routes(["A1", "A2"]), gate: defaultGate(), comp: defaultComp(), eq: defaultEq() },
    { id: 4, kind: "virtual", label: "Music", hwKey: null, online: true, gainDb: -3.2, mute: false, solo: false, routes: routes(["A1", "A2", "B1"]), gate: defaultGate(), comp: defaultComp(), eq: defaultEq() },
    { id: 5, kind: "virtual", label: "Chat", hwKey: null, online: true, gainDb: -6, mute: false, solo: false, routes: routes(["A2", "B1"]), gate: defaultGate(), comp: defaultComp(), eq: defaultEq() },
  ],
  buses: [
    { id: "A1", label: "Speakers", targetHwKey: "alsa_output.pci-0000_2f_00.1.hdmi-stereo", online: true, gainDb: 0, mute: false, limiter: defaultLimiter() },
    { id: "A2", label: "Headphones", targetHwKey: "alsa_output.usb-ZOOM_Corporation_UAC-2", online: true, gainDb: -2.5, mute: false, limiter: defaultLimiter() },
    { id: "B1", label: "Stream Mic", targetHwKey: null, online: true, gainDb: 0, mute: false, limiter: defaultLimiter() },
    { id: "B2", label: "Aux", targetHwKey: null, online: true, gainDb: -6, mute: false, limiter: defaultLimiter() },
  ],
  devices: [
    { nodeName: "alsa_input.usb-ZOOM_Corporation_UAC-2", description: "ZOOM UAC-2", mediaClass: "Audio/Source", serial: 67, channels: 2 },
    { nodeName: "alsa_input.pci-0000_31_00.4.analog-stereo", description: "Onboard Analog", mediaClass: "Audio/Source", serial: 63, channels: 2 },
    { nodeName: "alsa_input.usb-046d_HD_Pro_Webcam_C920", description: "C920 Webcam Mic", mediaClass: "Audio/Source", serial: 45, channels: 2 },
    { nodeName: "alsa_output.pci-0000_2f_00.1.hdmi-stereo", description: "HDMI / DisplayPort", mediaClass: "Audio/Sink", serial: 65, channels: 2 },
    { nodeName: "alsa_output.usb-ZOOM_Corporation_UAC-2", description: "ZOOM UAC-2", mediaClass: "Audio/Sink", serial: 66, channels: 2 },
  ],
};

const sims = new Map<number, Sim>([
  [1, makeSim("speech", -24, 9, 1.7)],
  [2, makeSim("steady", -36, 3, 4.1)],
  [3, makeSim("steady", -30, 6, 2.3)],
  [4, makeSim("music", -11, 4, 0.4)],
  [5, makeSim("speech", -22, 8, 3.6)],
]);

function findStrip(id: number): StripState | undefined {
  return state.strips.find((s) => s.id === id);
}

let mockAutostart = false;
let mockNotifications = true;
let mockMicTone = true;
let mockChannel: "stable" | "edge" = "stable";
const updateSubs = new Set<(text: string) => void>();
const mockShortcuts: Record<string, string> = { "toggle-mute-mics": "Meta+M" };

function emitState(): void {
  const snap = structuredClone(state);
  for (const cb of stateSubs) cb(snap);
}

const meterSubs = new Set<(f: MeterFrame) => void>();
const stateSubs = new Set<(s: AppState) => void>();
let seq = 0;
let timer: ReturnType<typeof setInterval> | undefined;
let lastTick = 0;

function startSim(): void {
  if (timer) return;
  lastTick = performance.now();
  timer = setInterval(() => {
    const now = performance.now();
    const t = now / 1000;
    const dt = Math.min(0.1, (now - lastTick) / 1000);
    lastTick = now;

    const anySolo = state.strips.some((s) => s.solo);
    const levels = new Map<number, number>();
    const stripsPre: number[][] = [];
    const stripFrames: number[][] = state.strips.map((s) => {
      const raw = simLevel(sims.get(s.id)!, t, dt);
      stripsPre.push(s.online ? [raw + 0.5, raw - 0.5, raw - 3, raw - 3] : [-90, -90, -90, -90]);
      const audible = !s.mute && (!anySolo || s.solo) && s.online;
      const lvl = audible && s.gainDb !== -Infinity ? raw + s.gainDb : -90;
      levels.set(s.id, lvl);
      const jitter = () => Math.random() * 1.2;
      return [lvl + 0.6 + jitter(), lvl - 0.6 + jitter(), lvl - 2.5, lvl - 2.8];
    });
    const busesPre: number[][] = [];
    const busFrames: number[][] = state.buses.map((b) => {
      const inputs = state.strips.filter((s) => s.routes[b.id]).map((s) => levels.get(s.id)!);
      const pre = powerSum(inputs);
      busesPre.push([pre + 0.4, pre - 0.4, pre - 2.5, pre - 2.7]);
      // The limiter holds the output at its threshold.
      const out = b.mute ? -90 : b.limiter.enabled ? Math.min(pre + b.gainDb, b.limiter.thresholdDb) : pre + b.gainDb;
      return [out + 0.4, out - 0.4, out - 2.5, out - 2.7];
    });

    const frame: MeterFrame = { seq: seq++, strips: stripFrames, buses: busFrames, stripsPre, busesPre };
    for (const cb of meterSubs) cb(frame);
  }, 33);
}

function target(t: Target): { gainDb: number; mute: boolean } | undefined {
  return "strip" in t ? findStrip(t.strip) : state.buses.find((b) => b.id === t.bus);
}

export const mockIpc: Ipc = {
  getState: () => Promise.resolve(structuredClone(state)),
  setGain: (t, db) => {
    const o = target(t);
    if (o) o.gainDb = db;
    return Promise.resolve();
  },
  setMute: (t, mute) => {
    const o = target(t);
    if (o) o.mute = mute;
    return Promise.resolve();
  },
  setSolo: (stripId, solo) => {
    const s = findStrip(stripId);
    if (s) s.solo = solo;
    return Promise.resolve();
  },
  setRoute: (stripId, bus, on) => {
    const s = findStrip(stripId);
    if (s) s.routes[bus] = on;
    return Promise.resolve();
  },
  setGateParams: (stripId, params) => {
    const s = findStrip(stripId);
    if (s) s.gate = { ...params };
    return Promise.resolve();
  },
  setCompParams: (stripId, params) => {
    const s = findStrip(stripId);
    if (s) s.comp = { ...params };
    return Promise.resolve();
  },
  setEqParams: (stripId, params) => {
    const s = findStrip(stripId);
    if (s) s.eq = JSON.parse(JSON.stringify(params));
    return Promise.resolve();
  },
  setLimiterParams: (bus, params) => {
    const b = state.buses.find((x) => x.id === bus);
    if (b) b.limiter = { ...params };
    return Promise.resolve();
  },
  setStripDevice: (stripId, hwKey) => {
    const s = findStrip(stripId);
    if (s) s.hwKey = hwKey;
    return Promise.resolve();
  },
  setBusTarget: (bus, hwKey) => {
    const b = state.buses.find((x) => x.id === bus);
    if (b) b.targetHwKey = hwKey;
    return Promise.resolve();
  },
  setDefaultOutput: (on) => {
    state.takeDefaultOutput = on;
    return Promise.resolve();
  },
  rename: (t, label) => {
    const clean = label.trim().slice(0, 32);
    if (!clean) return Promise.resolve();
    const o = "strip" in t ? findStrip(t.strip) : state.buses.find((b) => b.id === t.bus);
    if (o) o.label = clean;
    emitState();
    return Promise.resolve();
  },
  loadProfile: (name) => {
    if (!state.profiles.includes(name)) state.profiles = [...state.profiles, name].sort();
    state.profileName = name;
    emitState();
    return Promise.resolve();
  },
  saveProfileAs: (name) => {
    const clean = name.trim();
    if (!clean) return Promise.resolve();
    if (!state.profiles.includes(clean)) state.profiles = [...state.profiles, clean].sort();
    state.profileName = clean;
    emitState();
    return Promise.resolve();
  },
  deleteProfile: (name) => {
    if (name !== state.profileName) state.profiles = state.profiles.filter((p) => p !== name);
    emitState();
    return Promise.resolve();
  },
  getNotifications: () => Promise.resolve(mockNotifications),
  getMicTone: () => Promise.resolve(mockMicTone),
  setMicTone: (on) => {
    mockMicTone = on;
    return Promise.resolve();
  },
  setNotifications: (on) => {
    mockNotifications = on;
    return Promise.resolve();
  },
  getExePath: () => Promise.resolve("/usr/bin/linuxmeeter"),
  addStrip: (kind) => {
    const id = Math.max(0, ...state.strips.map((s) => s.id)) + 1;
    const hw = kind === "hardware";
    const strip: StripState = {
      id,
      kind,
      label: hw ? `Mic ${id}` : `Virtual ${id}`,
      hwKey: hw ? (state.devices.find((d) => d.mediaClass === "Audio/Source")?.nodeName ?? null) : null,
      online: true,
      gainDb: 0,
      mute: false,
      solo: false,
      routes: routes(hw ? [] : ["A1"]),
      gate: defaultGate(),
      comp: defaultComp(),
      eq: defaultEq(),
    };
    const at = hw ? state.strips.filter((s) => s.kind === "hardware").length : state.strips.length;
    state.strips.splice(at, 0, strip);
    sims.set(id, makeSim(hw ? "speech" : "music", -20, 6, Math.random() * 6));
    emitState();
    return Promise.resolve();
  },
  removeStrip: (stripId) => {
    state.strips = state.strips.filter((s) => s.id !== stripId);
    emitState();
    return Promise.resolve();
  },
  addBus: (hardware) => {
    const id = BUS_IDS.find((b) => isHardwareBus(b) === hardware && !state.buses.some((x) => x.id === b));
    if (!id) return Promise.resolve();
    const bus = {
      id,
      label: hardware ? `Speakers ${id}` : `Virtual Mic ${id}`,
      targetHwKey: hardware ? (state.devices.find((d) => d.mediaClass === "Audio/Sink")?.nodeName ?? null) : null,
      online: true,
      gainDb: 0,
      mute: false,
      limiter: defaultLimiter(),
    };
    state.buses.push(bus);
    state.buses.sort((a, b) => BUS_IDS.indexOf(a.id) - BUS_IDS.indexOf(b.id));
    emitState();
    return Promise.resolve();
  },
  removeBus: (bus) => {
    state.buses = state.buses.filter((b) => b.id !== bus);
    for (const s of state.strips) s.routes[bus] = false;
    emitState();
    return Promise.resolve();
  },
  getShortcuts: () => Promise.resolve({ available: true, bindings: { ...mockShortcuts } }),
  exportSettings: () => Promise.resolve("/home/you/linuxmeeter-settings.json"),
  importSettings: () => Promise.resolve([...state.profiles]),
  setShortcut: (action, keys) => {
    for (const [a, k] of Object.entries(mockShortcuts)) if (keys && k === keys) delete mockShortcuts[a];
    if (keys) mockShortcuts[action] = keys;
    else delete mockShortcuts[action];
    return Promise.resolve(keys);
  },
  onShortcutsChanged: () => () => {},
  getBuildInfo: () => Promise.resolve(["0.4.1.r60", "4cc88db95a37bcc53fb88e3edf709bb539fa5e9f"] as [string, string]),
  getUpdateChannel: () => Promise.resolve(mockChannel),
  checkUpdate: (channel) => {
    mockChannel = channel;
    return new Promise((r) =>
      setTimeout(
        () =>
          r({
            channel,
            platform: "arch",
            currentVersion: "0.4.1.r60",
            currentSha: "4cc88db95a37bcc53fb88e3edf709bb539fa5e9f",
            available: true,
            latest: channel === "edge" ? "0.4.1.r63 (9f3c2aa)" : "0.5.0",
            releaseUrl: "https://github.com/Schnubbirunkel/linuxmeeter/releases",
            asset: { name: "linuxmeeter.pkg.tar.zst", url: "", digest: null },
          }),
        400,
      ),
    );
  },
  installUpdate: () => {
    for (const [ms, text] of [[0, "Downloading…"], [800, "Installing… (enter your password)"]] as const)
      setTimeout(() => updateSubs.forEach((cb) => cb(text)), ms);
    return new Promise((_, reject) => setTimeout(() => reject("installation cancelled"), 2000));
  },
  onUpdateProgress: (cb) => {
    updateSubs.add(cb);
    return () => updateSubs.delete(cb);
  },
  openReleasePage: (url) => {
    window.open(url, "_blank");
    return Promise.resolve();
  },
  getAutostart: () => Promise.resolve(mockAutostart),
  setAutostart: (on) => {
    mockAutostart = on;
    return Promise.resolve();
  },
  onMeters: (cb) => {
    meterSubs.add(cb);
    startSim();
    return () => meterSubs.delete(cb);
  },
  onStateChanged: (cb) => {
    stateSubs.add(cb);
    return () => stateSubs.delete(cb);
  },
};
