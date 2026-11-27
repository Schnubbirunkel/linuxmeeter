/** The ONLY file that talks to the backend. Everything else imports `ipc`.
 *
 * In a browser (no Tauri runtime) or with VITE_MOCK=1 the mock backend is
 * used, so `pnpm dev` gives a fully animated console for design iteration.
 */
import type { AppState, BusId, StripKind, CompParams, EqParams, GateParams, LimiterParams, MeterFrame } from "./types";
import { mockIpc } from "./mock";

export type Target = { strip: number } | { bus: BusId };

export interface ClockProblem {
  card: string;
  description: string;
  nodes: string[];
  errors: number;
  /** The device it follows, if known. */
  follows: string | null;
  /** "buffer" when what it follows sets the clock on purpose (a second leader would tie). */
  remedy: ClockFix;
}
export type ClockFix = "lead" | "buffer";
export interface ClockInfo {
  problems: ClockProblem[];
  leaders: string[];
  buffered: string[];
  available: boolean;
}

export interface RemoteInfo {
  enabled: boolean;
  port: number;
  url: string;
  qrSvg: string;
  clients: number;
  error: string | null;
}

export type UpdateChannel = "stable" | "edge";
export interface UpdateAsset {
  name: string;
  url: string;
  digest: string | null;
}
export interface UpdateInfo {
  channel: UpdateChannel;
  platform: "arch" | "debian" | "unsupported";
  currentVersion: string;
  currentSha: string;
  available: boolean;
  latest: string;
  releaseUrl: string;
  asset: UpdateAsset | null;
}

export interface ShortcutInfo {
  available: boolean;
  bindings: Record<string, string>;
}

export interface Ipc {
  getState(): Promise<AppState>;
  setGain(target: Target, db: number): Promise<void>;
  setMute(target: Target, mute: boolean): Promise<void>;
  setSolo(stripId: number, solo: boolean): Promise<void>;
  setRoute(stripId: number, bus: BusId, on: boolean): Promise<void>;
  setGateParams(stripId: number, params: GateParams): Promise<void>;
  setCompParams(stripId: number, params: CompParams): Promise<void>;
  setEqParams(stripId: number, params: EqParams): Promise<void>;
  setLimiterParams(bus: BusId, params: LimiterParams): Promise<void>;
  /** Output EQ of an A bus (speaker / headphone correction). */
  setBusEqParams(bus: BusId, params: EqParams): Promise<void>;
  setStripDevice(stripId: number, hwKey: string): Promise<void>;
  setBusTarget(bus: BusId, hwKey: string): Promise<void>;
  setDefaultOutput(on: boolean): Promise<void>;
  /** Rename a strip or bus — also renames the PipeWire device other programs see. */
  rename(target: Target, label: string): Promise<void>;
  loadProfile(name: string): Promise<void>;
  saveProfileAs(name: string): Promise<void>;
  deleteProfile(name: string): Promise<void>;
  getNotifications(): Promise<boolean>;
  /** Warning tone when the microphones are muted / go live. */
  getMicTone(): Promise<boolean>;
  setMicTone(on: boolean): Promise<void>;
  /** The active profile's main output for the volume keys (null = the desktop's default). */
  setVolumeDevice(node: string | null): Promise<void>;
  setNotifications(on: boolean): Promise<void>;
  /** Absolute path of the app binary, for `<exe> ctl …` shortcut commands. */
  getExePath(): Promise<string>;
  addStrip(kind: StripKind): Promise<void>;
  removeStrip(stripId: number): Promise<void>;
  /** Adds the first free A bus (hardware) or B bus (virtual mic). */
  addBus(hardware: boolean): Promise<void>;
  removeBus(bus: BusId): Promise<void>;
  /** In-app global shortcuts (KDE). `available` is false outside Plasma. */
  getShortcuts(): Promise<ShortcutInfo>;
  /** Bind keys ("Ctrl+Alt+M", null = remove) to a ctl action; resolves to the stored keys. */
  setShortcut(action: string, keys: string | null): Promise<string | null>;
  /** Save all profiles + app settings to a file the user picks; resolves to its path (null = cancelled). */
  exportSettings(): Promise<string | null>;
  /** Restore such a file; resolves to the restored profile names (null = cancelled). */
  importSettings(): Promise<string[] | null>;
  /** Bindings changed outside the app (System Settings). */
  onShortcutsChanged(cb: () => void): () => void;
  /** This build: [version, commit sha]. */
  getBuildInfo(): Promise<[string, string]>;
  getUpdateChannel(): Promise<UpdateChannel>;
  /** Checks GitHub (and remembers the channel). */
  checkUpdate(channel: UpdateChannel): Promise<UpdateInfo>;
  /** Download, install (password prompt), restart. Resolves only on failure paths. */
  installUpdate(platform: UpdateInfo["platform"], asset: UpdateAsset): Promise<void>;
  onUpdateProgress(cb: (text: string) => void): () => void;
  openReleasePage(url: string): Promise<void>;
  /** Clock helper: devices dropping out because they follow another clock. */
  getClock(): Promise<ClockInfo | null>;
  dismissClock(card: string): Promise<void>;
  /** Give `card` a clock fix (null = remove it). Restarts PipeWire and the app. */
  clockFix(card: string, fix: ClockFix | null): Promise<void>;
  onClockProblem(cb: (p: ClockProblem) => void): () => void;
  /** Phone remote: state, on/off, new key. */
  getRemote(): Promise<RemoteInfo>;
  setRemote(enabled: boolean): Promise<RemoteInfo>;
  newRemoteKey(): Promise<RemoteInfo>;
  getAutostart(): Promise<boolean>;
  setAutostart(on: boolean): Promise<void>;
  /** Subscribe to meter frames; returns unsubscribe. */
  onMeters(cb: (frame: MeterFrame) => void): () => void;
  /** Backend-initiated state changes (device hotplug, external volume change…). */
  onStateChanged(cb: (state: AppState) => void): () => void;
}

export const IS_TAURI = "__TAURI_INTERNALS__" in window;
export const IS_MOCK = !!import.meta.env.VITE_MOCK || !IS_TAURI;

function tauriIpc(): Ipc {
  // Dynamic import keeps @tauri-apps/api out of the browser mock path.
  const core = import("@tauri-apps/api/core");
  const event = import("@tauri-apps/api/event");
  const invoke = async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
    (await core).invoke<T>(cmd, args);

  return {
    getState: () => invoke("get_state"),
    setGain: (target, db) => invoke("set_gain", { target, db }),
    setMute: (target, mute) => invoke("set_mute", { target, mute }),
    setSolo: (stripId, solo) => invoke("set_solo", { stripId, solo }),
    setRoute: (stripId, bus, on) => invoke("set_route", { stripId, bus, on }),
    setGateParams: (stripId, params) => invoke("set_gate_params", { stripId, params }),
    setCompParams: (stripId, params) => invoke("set_comp_params", { stripId, params }),
    setEqParams: (stripId, params) => invoke("set_eq_params", { stripId, params }),
    setBusEqParams: (bus, params) => invoke("set_bus_eq_params", { bus, params }),
    setLimiterParams: (bus, params) => invoke("set_limiter_params", { bus, params }),
    setStripDevice: (stripId, hwKey) => invoke("set_strip_device", { stripId, hwKey }),
    setBusTarget: (bus, hwKey) => invoke("set_bus_target", { bus, hwKey }),
    setDefaultOutput: (on) => invoke("set_default_output", { on }),
    rename: (target, label) => invoke("rename", { target, label }),
    loadProfile: (name) => invoke("load_profile", { name }),
    saveProfileAs: (name) => invoke("save_profile_as", { name }),
    deleteProfile: (name) => invoke("delete_profile", { name }),
    getNotifications: () => invoke("get_notifications"),
    getMicTone: () => invoke("get_mic_tone"),
    setMicTone: (on) => invoke("set_mic_tone", { on }),
    setVolumeDevice: (node) => invoke("set_volume_device", { node }),
    setNotifications: (on) => invoke("set_notifications", { on }),
    getExePath: () => invoke("get_exe_path"),
    addStrip: (kind) => invoke("add_strip", { kind }),
    removeStrip: (stripId) => invoke("remove_strip", { stripId }),
    addBus: (hardware) => invoke("add_bus", { hardware }),
    removeBus: (bus) => invoke("remove_bus", { bus }),
    getShortcuts: () => invoke("get_shortcuts"),
    setShortcut: (action, keys) => invoke("set_shortcut", { action, keys }),
    exportSettings: () => invoke("export_settings"),
    importSettings: () => invoke("import_settings"),
    onShortcutsChanged: (cb) => {
      const un = event.then((e) => e.listen("shortcuts_changed", () => cb()));
      return () => void un.then((f) => f());
    },
    getBuildInfo: () => invoke("get_build_info"),
    getUpdateChannel: () => invoke("get_update_channel"),
    checkUpdate: (channel) => invoke("check_update", { channel }),
    installUpdate: (platform, asset) => invoke("install_update", { platform, asset }),
    onUpdateProgress: (cb) => {
      const un = event.then((e) => e.listen<string>("update_progress", (ev) => cb(ev.payload)));
      return () => void un.then((f) => f());
    },
    openReleasePage: (url) => invoke("open_release_page", { url }),
    getClock: () => invoke("get_clock"),
    dismissClock: (card) => invoke("dismiss_clock", { card }),
    clockFix: (card, fix) => invoke("clock_fix", { card, fix }),
    onClockProblem: (cb) => {
      const un = event.then((e) => e.listen<ClockProblem>("clock_problem", (ev) => cb(ev.payload)));
      return () => void un.then((f) => f());
    },
    getRemote: () => invoke("get_remote"),
    setRemote: (enabled) => invoke("set_remote", { enabled }),
    newRemoteKey: () => invoke("new_remote_key"),
    getAutostart: () => invoke("get_autostart"),
    setAutostart: (on) => invoke("set_autostart", { on }),
    onMeters: (cb) => {
      const un = event.then((e) => e.listen<MeterFrame>("meters", (ev) => cb(ev.payload)));
      return () => void un.then((f) => f());
    },
    onStateChanged: (cb) => {
      const un = event.then((e) => e.listen<AppState>("state_changed", (ev) => cb(ev.payload)));
      return () => void un.then((f) => f());
    },
  };
}

export const ipc: Ipc = IS_MOCK ? mockIpc : tauriIpc();
