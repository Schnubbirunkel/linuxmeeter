//! Shared types between the audio engine, the Tauri shell, and (via serde)
//! the frontend. Serialization is camelCase to match the TypeScript mirror in
//! `src/lib/types.ts` — keep the two in sync.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BusId {
    A1,
    A2,
    A3,
    A4,
    B1,
    B2,
    B3,
}

impl BusId {
    /// Every bus a mixer can have; a profile uses any subset (`AppState.buses`).
    pub const ALL: [BusId; 7] = [
        BusId::A1,
        BusId::A2,
        BusId::A3,
        BusId::A4,
        BusId::B1,
        BusId::B2,
        BusId::B3,
    ];

    /// A buses play to a hardware device; B buses are virtual microphones.
    pub fn is_hardware(&self) -> bool {
        matches!(self, BusId::A1 | BusId::A2 | BusId::A3 | BusId::A4)
    }

    /// Lowercase node-name fragment: `lm.bus.a1`.
    pub fn node_base(&self) -> String {
        format!("lm.bus.{}", format!("{self:?}").to_lowercase())
    }
}

pub type StripId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StripKind {
    /// Appears as an Audio/Sink other apps can play into.
    Virtual,
    /// Captures from a hardware (or any existing) source node.
    Hardware,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub hold_ms: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -40.0,
            attack_ms: 20.0,
            release_ms: 100.0,
            hold_ms: 50.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

impl Default for CompParams {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 20.0,
            release_ms: 100.0,
            makeup_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqBandKind {
    LowShelf,
    Peak,
    HighShelf,
    /// High-pass: removes everything below the frequency (rumble, plosives).
    LowCut,
    /// Low-pass: removes everything above the frequency (hiss).
    HighCut,
}

impl EqBandKind {
    /// LSP para_equalizer `ft_N` filter-type value (0 = off).
    pub fn lsp_filter_type(&self) -> f32 {
        match self {
            EqBandKind::Peak => 1.0,      // Bell
            EqBandKind::LowCut => 2.0,    // Hi-pass
            EqBandKind::HighShelf => 3.0, // Hi-shelf
            EqBandKind::HighCut => 4.0,   // Lo-pass
            EqBandKind::LowShelf => 5.0,  // Lo-shelf
        }
    }
}

/// EQ bands the panel can add.
pub const MAX_EQ_BANDS: usize = 12;
/// Bands of the LSP para_equalizer_x16 plugin: the panel's bands, then the
/// four voice-color bands in the last four slots.
pub const EQ_SLOTS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqBand {
    pub kind: EqBandKind,
    pub freq_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqParams {
    pub enabled: bool,
    /// The EQ panel's bands, 0..=MAX_EQ_BANDS.
    pub bands: Vec<EqBand>,
    /// The voice-color pad's four bands. Older profiles kept them in
    /// `bands[4..8]`; `migrate` moves them here.
    #[serde(default)]
    pub color: Vec<EqBand>,
}

impl EqParams {
    /// The voice-color pad's bands (voice-tuned corners), independent of
    /// the EQ panel's bands.
    pub fn color_defaults() -> [EqBand; 4] {
        [
            EqBand {
                kind: EqBandKind::LowShelf,
                freq_hz: 300.0,
                gain_db: 0.0,
                q: 0.7,
            },
            EqBand {
                kind: EqBandKind::Peak,
                freq_hz: 400.0,
                gain_db: 0.0,
                q: 0.8,
            },
            EqBand {
                kind: EqBandKind::Peak,
                freq_hz: 3000.0,
                gain_db: 0.0,
                q: 0.8,
            },
            EqBand {
                kind: EqBandKind::HighShelf,
                freq_hz: 3500.0,
                gain_db: 0.0,
                q: 0.7,
            },
        ]
    }
}

impl EqParams {
    pub fn panel_defaults() -> Vec<EqBand> {
        vec![
            EqBand {
                kind: EqBandKind::LowShelf,
                freq_hz: 100.0,
                gain_db: 0.0,
                q: 0.7,
            },
            EqBand {
                kind: EqBandKind::Peak,
                freq_hz: 400.0,
                gain_db: 0.0,
                q: 1.0,
            },
            EqBand {
                kind: EqBandKind::Peak,
                freq_hz: 2500.0,
                gain_db: 0.0,
                q: 1.0,
            },
            EqBand {
                kind: EqBandKind::HighShelf,
                freq_hz: 8000.0,
                gain_db: 0.0,
                q: 0.7,
            },
        ]
    }

    /// Bring older layouts up to date: 8 bands in one list (4 panel + 4
    /// color) or 4 bands (no color pad yet) become `bands` + `color`; extra
    /// bands beyond what the plugin has are dropped.
    pub fn migrate(&mut self) {
        if self.color.len() != 4 {
            if self.color.is_empty() && self.bands.len() >= 8 {
                self.color = self.bands.drain(4..8).collect();
                self.bands.truncate(4);
            } else {
                self.color = Self::color_defaults().to_vec();
            }
        }
        self.bands.truncate(MAX_EQ_BANDS);
    }

    /// What each LSP slot carries: panel bands first, color bands in the
    /// last four slots, None = slot off.
    pub fn slots(&self) -> [Option<EqBand>; EQ_SLOTS] {
        let mut out = [None; EQ_SLOTS];
        for (i, b) in self.bands.iter().take(MAX_EQ_BANDS).enumerate() {
            out[i] = Some(*b);
        }
        for (i, b) in self.color.iter().take(EQ_SLOTS - MAX_EQ_BANDS).enumerate() {
            out[MAX_EQ_BANDS + i] = Some(*b);
        }
        out
    }
}

impl Default for EqParams {
    fn default() -> Self {
        Self {
            enabled: false,
            bands: Self::panel_defaults(),
            color: Self::color_defaults().to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimiterParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub release_ms: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_db: -1.0,
            release_ms: 50.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StripState {
    pub id: StripId,
    pub kind: StripKind,
    pub label: String,
    /// Stable PipeWire identity (`node.name`) of the hardware source, for Hardware strips.
    pub hw_key: Option<String>,
    pub online: bool,
    pub gain_db: f32,
    pub mute: bool,
    pub solo: bool,
    pub routes: BTreeMap<BusId, bool>,
    pub gate: GateParams,
    pub comp: CompParams,
    /// `default` so profiles written before the EQ existed still load.
    #[serde(default)]
    pub eq: EqParams,
}

impl StripState {
    /// Base node name; virtual strips' sink node carries this name directly.
    pub fn node_base(&self) -> String {
        format!("lm.strip.{}", self.id)
    }

    /// Node that carries Props (volume/mute/filter params): the capture side.
    pub fn control_node(&self) -> String {
        match self.kind {
            StripKind::Virtual => self.node_base(),
            StripKind::Hardware => format!("{}.cap", self.node_base()),
        }
    }

    /// Node whose output ports feed the routing matrix.
    pub fn out_node(&self) -> String {
        format!("{}.out", self.node_base())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusState {
    pub id: BusId,
    pub label: String,
    /// `node.name` of the hardware sink this bus feeds (A buses). None = virtual source (B buses).
    pub target_hw_key: Option<String>,
    pub online: bool,
    pub gain_db: f32,
    pub mute: bool,
    pub limiter: LimiterParams,
}

impl BusState {
    pub fn node_base(&self) -> String {
        self.id.node_base()
    }

    /// Mix-input node (also carries Props for gain/mute/limiter).
    pub fn in_node(&self) -> String {
        format!("{}.in", self.node_base())
    }

    /// Node whose output ports carry the post-limiter signal (meter tap point).
    pub fn tap_node(&self) -> String {
        if self.target_hw_key.is_some() {
            format!("{}.out", self.node_base())
        } else {
            self.node_base()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub node_name: String,
    pub description: String,
    pub media_class: String,
    pub serial: u64,
    pub channels: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub strips: Vec<StripState>,
    pub buses: Vec<BusState>,
    pub devices: Vec<DeviceInfo>,
    pub profile_name: String,
    /// Every saved profile name, sorted; always includes `profile_name`.
    #[serde(default)]
    pub profiles: Vec<String>,
    /// True while linuxmeeter's System strip is the system default output.
    pub take_default_output: bool,
}

/// One batched meter update, ~30/s. Channel layout: [peakL, peakR, rmsL, rmsR]
/// in dBFS; entries follow `AppState.strips` / `AppState.buses` order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterFrame {
    pub seq: u32,
    pub strips: Vec<[f32; 4]>,
    pub buses: Vec<[f32; 4]>,
    /// Each strip's input before gain and dynamics (gate/compressor display).
    #[serde(default)]
    pub strips_pre: Vec<[f32; 4]>,
    /// Each bus's mix before gain and limiter (limiter gain-reduction display).
    #[serde(default)]
    pub buses_pre: Vec<[f32; 4]>,
}

/// A strip or a bus, as sent by the frontend: `{"strip": 3}` or `{"bus": "A1"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Strip { strip: StripId },
    Bus { bus: BusId },
}

/// Commands into the engine thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineCommand {
    SetGain {
        target: Target,
        db: f32,
    },
    SetMute {
        target: Target,
        mute: bool,
    },
    SetSolo {
        strip: StripId,
        solo: bool,
    },
    SetRoute {
        strip: StripId,
        bus: BusId,
        on: bool,
    },
    SetGateParams {
        strip: StripId,
        params: GateParams,
    },
    SetCompParams {
        strip: StripId,
        params: CompParams,
    },
    SetEqParams {
        strip: StripId,
        params: EqParams,
    },
    SetLimiterParams {
        bus: BusId,
        params: LimiterParams,
    },
    /// Point a hardware strip at a different capture device (module reload).
    SetStripDevice {
        strip: StripId,
        hw_key: String,
    },
    /// Point an A-bus at a different output device (module reload).
    SetBusTarget {
        bus: BusId,
        hw_key: String,
    },
    /// Make the System strip the system-wide default output (VoiceMeeter
    /// style: all desktop audio flows through the mixer), or restore the
    /// previous hardware default.
    SetDefaultOutput {
        on: bool,
    },
    /// Start/stop the 30 Hz meter drain. Every frame costs a script evaluation
    /// in the webview, so a mixer sitting in the tray stops paying for meters
    /// nobody can see. Audio is unaffected either way.
    SetMetersEnabled {
        on: bool,
    },
    /// Rename a strip or bus. The label is also the `node.description` other
    /// programs see, so this reloads the module (node names stay identical).
    Rename {
        target: Target,
        label: String,
    },
    /// Move a fader by `step` of its travel (0.05 = 5 %), clamped. Unlike
    /// `SetGain` this emits a `State` event: it comes from outside the UI
    /// (global shortcuts), so the window has to be told.
    NudgeGain {
        target: Target,
        step: f32,
    },
    /// Flip mute; emits `State`.
    ToggleMute {
        target: Target,
    },
    /// Save the current profile, then switch to `name` (created from the
    /// current mixer if it does not exist yet).
    LoadProfile {
        name: String,
    },
    /// Copy the current mixer into a new profile `name` and switch to it.
    SaveProfileAs {
        name: String,
    },
    /// Delete a saved profile (never the active one).
    DeleteProfile {
        name: String,
    },
    /// Switch to the next (`step` = 1) or previous (-1) profile, wrapping.
    CycleProfile {
        step: i32,
    },
    /// Add an input strip. Hardware strips capture `hw_key` (default: the
    /// system default microphone, else the first unused input).
    AddStrip {
        kind: StripKind,
        hw_key: Option<String>,
    },
    RemoveStrip {
        strip: StripId,
    },
    /// Add the first free A bus (`hardware`, plays to the default output
    /// device) or B bus (virtual microphone).
    AddBus {
        hardware: bool,
    },
    RemoveBus {
        bus: BusId,
    },
    /// Restore a backup: write each (profile name, TOML text), then switch to
    /// `active` without saving the running mixer over it first.
    ImportProfiles {
        files: Vec<(String, String)>,
        active: String,
    },
    /// Emit a `State` now (e.g. after gain changes from the phone remote,
    /// which the desktop window should follow).
    RequestState,
    /// Play the microphone warning tone: rising (mics live) or falling (muted).
    Beep {
        rising: bool,
    },
    /// Mute or unmute every hardware input strip at once; emits `State`.
    SetMicsMute {
        mute: bool,
    },
    Shutdown,
}

/// Longest strip/bus label; keeps names readable in other programs' device lists.
pub const MAX_LABEL_LEN: usize = 32;

/// Trim, drop control characters, cap the length. `None` if nothing is left.
pub fn clean_label(label: &str) -> Option<String> {
    let s: String = label
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL_LEN)
        .collect();
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Profile names become file names: no path separators, no leading dot.
pub fn clean_profile_name(name: &str) -> Option<String> {
    let s: String = name
        .trim()
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .take(48)
        .collect();
    let s = s.trim().trim_start_matches('.').trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Fader taper, mirrored from `src/lib/utils/db.ts` — keep the two in sync.
/// Piecewise-linear: ~55 % of the travel covers −12..+12 dB.
pub mod taper {
    const BREAKPOINTS: [(f32, f32); 6] = [
        (0.0, -72.0),
        (0.08, -50.0),
        (0.3, -30.0),
        (0.55, -12.0),
        (0.78, 0.0),
        (1.0, 12.0),
    ];
    pub const MIN_DB: f32 = -72.0;
    pub const MAX_DB: f32 = 12.0;

    pub fn pos_to_db(p: f32) -> f32 {
        if p <= 0.0 {
            return MIN_DB;
        }
        let p = p.min(1.0);
        for w in BREAKPOINTS.windows(2) {
            let ((p0, d0), (p1, d1)) = (w[0], w[1]);
            if p <= p1 {
                return d0 + (d1 - d0) * (p - p0) / (p1 - p0);
            }
        }
        MAX_DB
    }

    pub fn db_to_pos(db: f32) -> f32 {
        if db.is_nan() || db <= MIN_DB {
            return 0.0;
        }
        let db = db.min(MAX_DB);
        for w in BREAKPOINTS.windows(2) {
            let ((p0, d0), (p1, d1)) = (w[0], w[1]);
            if db <= d1 {
                return p0 + (p1 - p0) * (db - d0) / (d1 - d0);
            }
        }
        1.0
    }

    /// Move `db` by `step` of fader travel; result snapped to 0.1 dB.
    pub fn nudge(db: f32, step: f32) -> f32 {
        let p = (db_to_pos(db) + step).clamp(0.0, 1.0);
        // Round the position first so repeated nudges land on the same grid
        // (0 %, 5 %, 10 % ...) instead of drifting with the 0.1 dB rounding.
        let p = (p * 1000.0).round() / 1000.0;
        if p <= 0.0 {
            return MIN_DB;
        }
        (pos_to_db(p) * 10.0).round() / 10.0
    }
}

/// Events out of the engine thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEvent {
    /// Full snapshot; sent on startup and after any state-affecting change.
    State(AppState),
    Meters(MeterFrame),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(id: StripId, kind: StripKind) -> StripState {
        StripState {
            id,
            kind,
            label: "Test".into(),
            hw_key: None,
            online: true,
            gain_db: 0.0,
            mute: false,
            solo: false,
            routes: BusId::ALL.iter().map(|b| (*b, false)).collect(),
            gate: GateParams::default(),
            comp: CompParams::default(),
            eq: EqParams::default(),
        }
    }

    fn bus(id: BusId, target: Option<&str>) -> BusState {
        BusState {
            id,
            label: "Test".into(),
            target_hw_key: target.map(String::from),
            online: true,
            gain_db: 0.0,
            mute: false,
            limiter: LimiterParams::default(),
        }
    }

    // ---- node naming: the identity profiles, routes, and taps all key on ----

    #[test]
    fn virtual_strip_controls_its_sink_node_directly() {
        let s = strip(3, StripKind::Virtual);
        assert_eq!(s.node_base(), "lm.strip.3");
        assert_eq!(
            s.control_node(),
            "lm.strip.3",
            "the sink node carries Props"
        );
        assert_eq!(s.out_node(), "lm.strip.3.out");
    }

    /// A hardware strip's Props live on the capture side, not the base name —
    /// writing to the wrong node makes every fader and DSP knob a no-op.
    #[test]
    fn hardware_strip_controls_its_capture_node() {
        let s = strip(1, StripKind::Hardware);
        assert_eq!(s.control_node(), "lm.strip.1.cap");
        assert_eq!(s.out_node(), "lm.strip.1.out");
    }

    #[test]
    fn bus_node_names_are_lowercase() {
        assert_eq!(BusId::A1.node_base(), "lm.bus.a1");
        assert_eq!(BusId::B2.node_base(), "lm.bus.b2");
    }

    /// A buses meter after their output node; B buses ARE the source node the
    /// world sees, so that is where the post-limiter signal is tapped.
    #[test]
    fn bus_tap_point_depends_on_bus_kind() {
        assert_eq!(
            bus(BusId::A1, Some("alsa_output.hdmi")).tap_node(),
            "lm.bus.a1.out"
        );
        assert_eq!(bus(BusId::B1, None).tap_node(), "lm.bus.b1");
        assert_eq!(bus(BusId::A1, Some("x")).in_node(), "lm.bus.a1.in");
    }

    #[test]
    fn strip_node_names_are_unique_per_strip() {
        let a = strip(1, StripKind::Virtual);
        let b = strip(2, StripKind::Virtual);
        assert_ne!(a.out_node(), b.out_node());
        assert_ne!(a.control_node(), b.control_node());
    }

    // ---- IPC contract with src/lib/types.ts --------------------------------

    /// `Target` is untagged: the frontend sends `{"strip": 3}` / `{"bus": "A1"}`.
    /// If this drifts, every command silently fails to deserialize.
    #[test]
    fn target_serializes_untagged() {
        assert_eq!(
            serde_json::to_string(&Target::Strip { strip: 3 }).unwrap(),
            r#"{"strip":3}"#
        );
        assert_eq!(
            serde_json::to_string(&Target::Bus { bus: BusId::A1 }).unwrap(),
            r#"{"bus":"A1"}"#
        );

        assert_eq!(
            serde_json::from_str::<Target>(r#"{"strip":7}"#).unwrap(),
            Target::Strip { strip: 7 }
        );
        assert_eq!(
            serde_json::from_str::<Target>(r#"{"bus":"B2"}"#).unwrap(),
            Target::Bus { bus: BusId::B2 }
        );
    }

    /// The TypeScript mirror expects camelCase. These are the exact keys
    /// `src/lib/types.ts` reads.
    #[test]
    fn app_state_uses_camel_case_keys() {
        let json = serde_json::to_value(AppState {
            strips: vec![strip(1, StripKind::Hardware)],
            buses: vec![bus(BusId::A1, Some("alsa_output.hdmi"))],
            devices: vec![DeviceInfo {
                node_name: "alsa_output.hdmi".into(),
                description: "HDMI".into(),
                media_class: "Audio/Sink".into(),
                serial: 42,
                channels: 2,
            }],
            profile_name: "Default".into(),
            profiles: vec!["Default".into()],
            take_default_output: true,
        })
        .expect("serialize");

        assert!(json.get("profileName").is_some());
        assert!(json.get("takeDefaultOutput").is_some());
        assert!(
            json.get("profile_name").is_none(),
            "snake_case would break the frontend"
        );

        let s = &json["strips"][0];
        for key in ["hwKey", "gainDb", "kind", "routes", "gate", "comp", "eq"] {
            assert!(s.get(key).is_some(), "strip is missing {key}");
        }
        assert!(json["buses"][0].get("targetHwKey").is_some());
        assert!(json["devices"][0].get("nodeName").is_some());
        assert!(json["devices"][0].get("mediaClass").is_some());
    }

    /// StripKind is snake_case on the wire: types.ts declares "virtual" | "hardware".
    #[test]
    fn strip_kind_is_lowercase_on_the_wire() {
        assert_eq!(serde_json::to_value(StripKind::Virtual).unwrap(), "virtual");
        assert_eq!(
            serde_json::to_value(StripKind::Hardware).unwrap(),
            "hardware"
        );
    }

    #[test]
    fn eq_band_kind_matches_the_typescript_union() {
        assert_eq!(
            serde_json::to_value(EqBandKind::LowShelf).unwrap(),
            "low_shelf"
        );
        assert_eq!(serde_json::to_value(EqBandKind::Peak).unwrap(), "peak");
        assert_eq!(
            serde_json::to_value(EqBandKind::HighShelf).unwrap(),
            "high_shelf"
        );
    }

    #[test]
    fn dsp_params_use_camel_case_keys() {
        let gate = serde_json::to_value(GateParams::default()).unwrap();
        for key in ["enabled", "thresholdDb", "attackMs", "releaseMs", "holdMs"] {
            assert!(gate.get(key).is_some(), "gate is missing {key}");
        }
        let comp = serde_json::to_value(CompParams::default()).unwrap();
        for key in ["thresholdDb", "ratio", "attackMs", "releaseMs", "makeupDb"] {
            assert!(comp.get(key).is_some(), "comp is missing {key}");
        }
        let lim = serde_json::to_value(LimiterParams::default()).unwrap();
        for key in ["enabled", "thresholdDb", "releaseMs"] {
            assert!(lim.get(key).is_some(), "limiter is missing {key}");
        }
    }

    #[test]
    fn meter_frame_round_trips() {
        let f = MeterFrame {
            seq: 9,
            strips: vec![[-6.0, -6.5, -9.0, -9.2]],
            buses: vec![[0.0; 4]],
            strips_pre: vec![[-3.0; 4]],
            buses_pre: vec![],
        };
        let back: MeterFrame = serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(back, f);
    }

    /// Commands are externally tagged; this is what src-tauri forwards.
    #[test]
    fn engine_commands_round_trip() {
        let cmds = vec![
            EngineCommand::SetGain {
                target: Target::Strip { strip: 1 },
                db: -6.0,
            },
            EngineCommand::SetRoute {
                strip: 1,
                bus: BusId::B1,
                on: true,
            },
            EngineCommand::SetEqParams {
                strip: 2,
                params: EqParams::default(),
            },
            EngineCommand::SetDefaultOutput { on: true },
            EngineCommand::SetMetersEnabled { on: false },
            EngineCommand::Rename {
                target: Target::Bus { bus: BusId::B1 },
                label: "Discord".into(),
            },
            EngineCommand::NudgeGain {
                target: Target::Strip { strip: 2 },
                step: -0.05,
            },
            EngineCommand::ToggleMute {
                target: Target::Strip { strip: 2 },
            },
            EngineCommand::LoadProfile {
                name: "Gaming".into(),
            },
            EngineCommand::CycleProfile { step: 1 },
            EngineCommand::AddStrip {
                kind: StripKind::Hardware,
                hw_key: None,
            },
            EngineCommand::RemoveStrip { strip: 3 },
            EngineCommand::AddBus { hardware: true },
            EngineCommand::RemoveBus { bus: BusId::A3 },
            EngineCommand::Shutdown,
        ];
        for cmd in cmds {
            let text = serde_json::to_string(&cmd).expect("serialize");
            serde_json::from_str::<EngineCommand>(&text).expect("deserialize");
        }
    }

    #[test]
    fn labels_are_cleaned() {
        assert_eq!(clean_label("  Game  ").as_deref(), Some("Game"));
        assert_eq!(clean_label("a\nb").as_deref(), Some("ab"));
        assert_eq!(clean_label("   "), None);
        assert_eq!(clean_label(&"x".repeat(100)).unwrap().len(), MAX_LABEL_LEN);
    }

    #[test]
    fn profile_names_cannot_escape_the_directory() {
        assert_eq!(
            clean_profile_name("../etc/passwd").as_deref(),
            Some("etcpasswd")
        );
        assert_eq!(clean_profile_name(" Gaming ").as_deref(), Some("Gaming"));
        assert_eq!(clean_profile_name("..."), None);
    }

    /// Must match `src/lib/utils/db.ts` so a shortcut step and a fader drag agree.
    #[test]
    fn taper_matches_the_frontend_breakpoints() {
        assert_eq!(taper::pos_to_db(0.78), 0.0);
        assert_eq!(taper::pos_to_db(0.55), -12.0);
        assert_eq!(taper::db_to_pos(0.0), 0.78);
        assert_eq!(taper::db_to_pos(-100.0), 0.0);
        assert_eq!(taper::db_to_pos(40.0), 1.0);
        for i in 1..=100 {
            let p = i as f32 / 100.0;
            assert!((taper::db_to_pos(taper::pos_to_db(p)) - p).abs() < 1e-4);
        }
    }

    #[test]
    fn nudges_move_five_percent_and_clamp() {
        assert_eq!(taper::nudge(0.0, 0.05), 2.7); // 0.78 -> 0.83
        assert_eq!(taper::nudge(0.0, -0.05), -2.6); // 0.78 -> 0.73
        assert_eq!(taper::nudge(12.0, 0.05), 12.0);
        assert_eq!(taper::nudge(-72.0, -0.05), -72.0);
        assert_eq!(taper::nudge(f32::NEG_INFINITY, 0.05), -58.3); // 0 -> 0.05
                                                                  // Down then up returns to the start.
        let mut db = 0.0;
        for _ in 0..7 {
            db = taper::nudge(db, -0.05);
        }
        for _ in 0..7 {
            db = taper::nudge(db, 0.05);
        }
        assert!(db.abs() < 0.05, "drifted to {db}");
    }

    // ---- DSP defaults -----------------------------------------------------

    #[test]
    fn eq_defaults_to_four_panel_bands_and_the_color_corners() {
        let eq = EqParams::default();
        assert_eq!(eq.bands.len(), 4);
        assert_eq!(eq.color, EqParams::color_defaults().to_vec());
        assert!(!eq.enabled);
    }

    /// Profiles from before this change keep panel and color bands in one
    /// list of eight; the color pad must keep its settings.
    #[test]
    fn eq_migration_splits_the_old_eight_band_layout() {
        let mut old = EqParams {
            enabled: true,
            bands: EqParams::default().bands,
            color: vec![],
        };
        let mut color = EqParams::color_defaults();
        color[2].gain_db = 7.5;
        old.bands.extend(color);
        old.migrate();
        assert_eq!(old.bands.len(), 4);
        assert_eq!(old.color[2].gain_db, 7.5);

        let mut pre_pad = EqParams {
            enabled: false,
            bands: EqParams::panel_defaults(),
            color: vec![],
        };
        pre_pad.migrate();
        assert_eq!(pre_pad.color, EqParams::color_defaults().to_vec());

        let mut current = EqParams::default();
        current.bands.push(current.bands[0]);
        let before = current.clone();
        current.migrate();
        assert_eq!(
            current, before,
            "a current profile with five bands is left alone"
        );
    }

    /// The plugin has 16 slots: panel bands from the start, color bands in
    /// the last four, everything else switched off.
    #[test]
    fn eq_slots_put_color_bands_last_and_switch_the_rest_off() {
        let mut eq = EqParams::default();
        eq.bands.push(EqBand {
            kind: EqBandKind::LowCut,
            freq_hz: 80.0,
            gain_db: 0.0,
            q: 0.7,
        });
        let slots = eq.slots();
        assert_eq!(slots[4].map(|b| b.kind), Some(EqBandKind::LowCut));
        assert!(slots[5..MAX_EQ_BANDS].iter().all(Option::is_none));
        assert_eq!(
            slots[MAX_EQ_BANDS..]
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
            EqParams::color_defaults().to_vec()
        );
    }

    #[test]
    fn eq_panel_bands_span_the_spectrum() {
        let eq = EqParams::default();
        assert_eq!(eq.bands[0].kind, EqBandKind::LowShelf);
        assert_eq!(eq.bands[3].kind, EqBandKind::HighShelf);
        let freqs: Vec<f32> = eq.bands[..4].iter().map(|b| b.freq_hz).collect();
        assert!(
            freqs.windows(2).all(|w| w[0] < w[1]),
            "bands must ascend: {freqs:?}"
        );
    }

    /// These are LSP para_equalizer `ft_N` port values; a wrong number silently
    /// selects the wrong filter shape.
    #[test]
    fn lsp_filter_types_are_the_documented_values() {
        assert_eq!(EqBandKind::Peak.lsp_filter_type(), 1.0);
        assert_eq!(EqBandKind::HighShelf.lsp_filter_type(), 3.0);
        assert_eq!(EqBandKind::LowShelf.lsp_filter_type(), 5.0);
        assert_eq!(EqBandKind::LowCut.lsp_filter_type(), 2.0);
        assert_eq!(EqBandKind::HighCut.lsp_filter_type(), 4.0);
    }

    /// The limiter is the last thing before hardware and the virtual mic, so it
    /// defaults to on with headroom.
    #[test]
    fn limiter_defaults_to_enabled_with_headroom() {
        let l = LimiterParams::default();
        assert!(l.enabled);
        assert!(l.threshold_db < 0.0);
    }

    #[test]
    fn gate_and_comp_default_to_bypassed() {
        assert!(!GateParams::default().enabled);
        assert!(!CompParams::default().enabled);
    }

    /// Routes are a BTreeMap keyed by BusId; declaration order is the order
    /// they are shown and saved in, so A buses must come before B buses.
    #[test]
    fn bus_ids_sort_hardware_before_virtual() {
        let mut sorted = BusId::ALL;
        sorted.sort();
        assert_eq!(sorted, BusId::ALL);
        assert_eq!(BusId::ALL.iter().filter(|b| b.is_hardware()).count(), 4);
        assert_eq!(BusId::A3.node_base(), "lm.bus.a3");
        assert_eq!(serde_json::to_value(BusId::B3).unwrap(), "B3");
    }
}
