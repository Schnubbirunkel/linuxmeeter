/** Mirrors crates/lm-protocol (serde-renamed to camelCase at the IPC boundary). */

export type BusId = "A1" | "A2" | "A3" | "A4" | "B1" | "B2" | "B3";
/** Every bus a mixer can have, in display order; a profile uses a subset (AppState.buses). */
export const BUS_IDS: BusId[] = ["A1", "A2", "A3", "A4", "B1", "B2", "B3"];
export const isHardwareBus = (id: BusId): boolean => id.startsWith("A");

export type StripKind = "virtual" | "hardware";

export interface GateParams {
  enabled: boolean;
  thresholdDb: number;
  attackMs: number;
  releaseMs: number;
  holdMs: number;
}

export interface CompParams {
  enabled: boolean;
  thresholdDb: number;
  ratio: number;
  attackMs: number;
  releaseMs: number;
  makeupDb: number;
}

export interface LimiterParams {
  enabled: boolean;
  thresholdDb: number;
  releaseMs: number;
}

export type EqBandKind = "low_shelf" | "peak" | "high_shelf" | "low_cut" | "high_cut";
/** Panel bands the 16-band plugin leaves room for (lm_protocol::MAX_EQ_BANDS). */
export const MAX_EQ_BANDS = 12;

export interface EqBand {
  kind: EqBandKind;
  freqHz: number;
  gainDb: number;
  q: number;
}

export interface EqParams {
  enabled: boolean;
  /** The EQ panel's bands, 0..MAX_EQ_BANDS. */
  bands: EqBand[];
  /** The voice-color pad's four bands. */
  color: EqBand[];
}

export interface StripState {
  id: number;
  kind: StripKind;
  label: string;
  /** Stable PipeWire identity (node.name) for hardware strips. */
  hwKey: string | null;
  online: boolean;
  gainDb: number;
  mute: boolean;
  solo: boolean;
  routes: Record<BusId, boolean>;
  gate: GateParams;
  comp: CompParams;
  eq: EqParams;
}

export interface BusState {
  id: BusId;
  label: string;
  targetHwKey: string | null;
  online: boolean;
  gainDb: number;
  mute: boolean;
  limiter: LimiterParams;
}

export interface DeviceInfo {
  nodeName: string;
  description: string;
  mediaClass: string;
  serial: number;
  channels: number;
}

export interface AppState {
  strips: StripState[];
  buses: BusState[];
  devices: DeviceInfo[];
  profileName: string;
  /** Every saved profile name, sorted; includes profileName. */
  profiles: string[];
  /** True while linuxmeeter's System strip is the system default output. */
  takeDefaultOutput: boolean;
}

/** One batched meter update, ~30/s. Channel layout: [peakL, peakR, rmsL, rmsR] in dBFS. */
export interface MeterFrame {
  seq: number;
  /** Keyed by strip id order in AppState.strips. */
  strips: number[][];
  /** Keyed by bus order in AppState.buses. */
  buses: number[][];
}

export type MeterKey = `s:${number}` | `b:${BusId}`;
