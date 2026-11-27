//! Clock helper: spot devices that drop out because they follow another
//! device's clock, and let them set the clock instead.
//!
//! When linuxmeeter links, say, a wireless USB headset's mic to the onboard
//! speakers, PipeWire runs both on one clock: one device drives, the other
//! follows with adaptive resampling. Some devices (wireless dongles above
//! all) cannot follow cleanly; their error counter in `pw-top` climbs by
//! hundreds a second and they sound like a robot.
//!
//! Detection reads `pw-top -b` every few seconds. The fix is a WirePlumber
//! rule giving every node of that sound card a high `priority.driver`, in a
//! file linuxmeeter owns:
//! `~/.config/wireplumber/wireplumber.conf.d/60-linuxmeeter-clock.conf`.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lm_protocol::AppState;
use serde::Serialize;

/// How often pw-top is sampled.
const INTERVAL: Duration = Duration::from_secs(10);
/// New errors per interval that count as "dropping out" (the headset that
/// prompted this made hundreds a second).
const THRESHOLD: u64 = 20;
const LEADER_PRIORITY: u32 = 3000;

/// One node's line in `pw-top -b`.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeStat {
    pub name: String,
    pub errors: u64,
    /// Listed under another node ("+"): it follows that driver's clock.
    pub follower: bool,
}

/// The nodes of the last complete block of `pw-top -b -n N` output.
pub fn parse_pw_top(text: &str) -> Vec<NodeStat> {
    let mut blocks: Vec<Vec<NodeStat>> = Vec::new();
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.first() == Some(&"S") && t.get(1) == Some(&"ID") {
            blocks.push(Vec::new());
            continue;
        }
        // S ID QUANT RATE WAIT BUSY W/Q B/Q ERR [FORMAT…] [+|=] NAME
        if t.len() < 10 {
            continue;
        }
        let Some(block) = blocks.last_mut() else {
            continue;
        };
        let Ok(errors) = t[8].parse::<u64>() else {
            continue;
        };
        let name = t[t.len() - 1].to_string();
        let follower = t[t.len() - 2] == "+";
        block.push(NodeStat {
            name,
            errors,
            follower,
        });
    }
    // The first block of a batch run only lists nodes; the last one has numbers.
    blocks.pop().unwrap_or_default()
}

/// "alsa_input.usb-1532_Razer…-00.mono-fallback" -> "usb-1532_Razer…-00":
/// the part every node of one sound card shares.
pub fn card_of(node_name: &str) -> Option<&str> {
    let rest = node_name
        .strip_prefix("alsa_input.")
        .or_else(|| node_name.strip_prefix("alsa_output."))?;
    Some(rest.rsplit_once('.').map_or(rest, |(card, _profile)| card))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// The card (see `card_of`).
    pub card: String,
    /// Human name of the device, for the banner.
    pub description: String,
    /// The nodes of that card linuxmeeter knows (input and output).
    pub nodes: Vec<String>,
    /// Errors in the last interval.
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockInfo {
    pub problems: Vec<Problem>,
    /// Cards that set the clock by linuxmeeter's rule.
    pub leaders: Vec<String>,
    /// pw-top is there (without it nothing is detected).
    pub available: bool,
}

pub struct Clock {
    problems: Mutex<Vec<Problem>>,
    dismissed: Mutex<BTreeSet<String>>,
    available: Mutex<bool>,
}

impl Clock {
    /// Start watching. `on_problem` runs once per newly found problem.
    pub fn start(
        state: Arc<std::sync::RwLock<Option<AppState>>>,
        on_problem: impl Fn(&Problem) + Send + 'static,
    ) -> Arc<Self> {
        let me = Arc::new(Self {
            problems: Mutex::new(Vec::new()),
            dismissed: Mutex::new(BTreeSet::new()),
            available: Mutex::new(true),
        });
        let watcher = me.clone();
        std::thread::Builder::new()
            .name("lm-clock".into())
            .spawn(move || {
                let mut last: HashMap<String, u64> = HashMap::new();
                loop {
                    std::thread::sleep(INTERVAL);
                    let Some(text) = run_pw_top() else {
                        *watcher.available.lock().expect("lock") = false;
                        tracing::info!("pw-top not available; clock helper off");
                        return;
                    };
                    let stats = parse_pw_top(&text);
                    let Some(app) = state.read().expect("state lock").clone() else {
                        continue;
                    };
                    let found = detect(&stats, &last, &app, &leaders());
                    last = stats.iter().map(|s| (s.name.clone(), s.errors)).collect();
                    let dismissed = watcher.dismissed.lock().expect("lock").clone();
                    let mut problems = watcher.problems.lock().expect("lock");
                    for p in found {
                        if dismissed.contains(&p.card) || problems.iter().any(|q| q.card == p.card)
                        {
                            continue;
                        }
                        tracing::warn!(
                            "{} drops out following another clock ({} errors)",
                            p.description,
                            p.errors
                        );
                        on_problem(&p);
                        problems.push(p);
                    }
                }
            })
            .expect("spawn clock watcher");
        me
    }

    pub fn info(&self) -> ClockInfo {
        ClockInfo {
            problems: self.problems.lock().expect("lock").clone(),
            leaders: leaders(),
            available: *self.available.lock().expect("lock"),
        }
    }

    /// Hide a problem for the rest of this session.
    pub fn dismiss(&self, card: &str) {
        self.dismissed
            .lock()
            .expect("lock")
            .insert(card.to_string());
        self.problems
            .lock()
            .expect("lock")
            .retain(|p| p.card != card);
    }
}

fn run_pw_top() -> Option<String> {
    let out = std::process::Command::new("pw-top")
        .args(["-b", "-n", "2"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Devices linuxmeeter uses (strip inputs, bus outputs) that follow another
/// clock and gained at least THRESHOLD errors since the last sample.
fn detect(
    stats: &[NodeStat],
    last: &HashMap<String, u64>,
    app: &AppState,
    leaders: &[String],
) -> Vec<Problem> {
    let used: BTreeSet<&str> = app
        .strips
        .iter()
        .filter_map(|s| s.hw_key.as_deref())
        .chain(app.buses.iter().filter_map(|b| b.target_hw_key.as_deref()))
        .collect();
    let mut out: Vec<Problem> = Vec::new();
    for s in stats {
        if !s.follower || !used.contains(s.name.as_str()) {
            continue;
        }
        let Some(before) = last.get(&s.name) else {
            continue;
        };
        let new = s.errors.saturating_sub(*before);
        if new < THRESHOLD {
            continue;
        }
        let Some(card) = card_of(&s.name) else {
            continue;
        };
        if leaders.iter().any(|l| l == card) || out.iter().any(|p| p.card == card) {
            continue;
        }
        let nodes: Vec<String> = app
            .devices
            .iter()
            .map(|d| d.node_name.clone())
            .filter(|n| card_of(n) == Some(card))
            .collect();
        let description = app
            .devices
            .iter()
            .find(|d| d.node_name == s.name)
            .map(|d| d.description.clone())
            .unwrap_or_else(|| s.name.clone());
        out.push(Problem {
            card: card.to_string(),
            description,
            nodes,
            errors: new,
        });
    }
    out
}

// ---- the WirePlumber rule ------------------------------------------------------

fn rule_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("wireplumber/wireplumber.conf.d/60-linuxmeeter-clock.conf")
}

/// Cards in linuxmeeter's rule file.
pub fn leaders() -> Vec<String> {
    let text = std::fs::read_to_string(rule_path()).unwrap_or_default();
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("# card: "))
        .map(|c| c.trim().to_string())
        .collect()
}

/// The rule file for `cards`: every node of each card drives the clock.
pub fn rule_text(cards: &[String]) -> String {
    let mut s = String::from(
        "# Written by linuxmeeter (Settings → Clock). These sound cards set the\n\
         # clock instead of following another device's; edit in linuxmeeter.\n",
    );
    for c in cards {
        s += &format!("# card: {c}\n");
    }
    s += "monitor.alsa.rules = [\n";
    for c in cards {
        // Same shape as the hand-written rule that fixed the headset this
        // was built for: "~alsa_.*<card>.*". Characters with a meaning in a
        // regex become "." (any character), so no escaping is needed.
        let pattern: String = c
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                    ch
                } else {
                    '.'
                }
            })
            .collect();
        s += &format!(
            "  {{\n    matches = [ {{ node.name = \"~alsa_.*{pattern}.*\" }} ]\n    actions = {{\n      update-props = {{\n        priority.driver = {LEADER_PRIORITY}\n        session.suspend-timeout-seconds = 0\n      }}\n    }}\n  }}\n"
        );
    }
    s += "]\n";
    s
}

/// Add or remove a card in the rule file (removing the file when empty).
pub fn set_leader(card: &str, lead: bool) -> Result<(), String> {
    let mut cards = leaders();
    cards.retain(|c| c != card);
    if lead {
        cards.push(card.to_string());
    }
    let path = rule_path();
    if cards.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, rule_text(&cards))
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Restart WirePlumber + PipeWire (so the rule applies) and then linuxmeeter
/// itself — it loses its connection when PipeWire restarts. Runs detached;
/// the caller exits right after.
pub fn restart_audio_and_app(pid: u32) -> Result<(), String> {
    let exe = crate::updater::exe_after_update();
    std::process::Command::new("sh")
        .args([
            "-c",
            "sleep 0.5; systemctl --user restart wireplumber pipewire pipewire-pulse 2>/dev/null \
             || systemctl --user restart wireplumber pipewire; sleep 2; exec \"$0\" --replace \"$1\"",
        ])
        .arg(exe)
        .arg(pid.to_string())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not restart PipeWire: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_protocol::{BusId, BusState, DeviceInfo, LimiterParams, StripKind, StripState};

    /// The user's own pw-top output (trimmed): the headset follows nothing,
    /// the onboard card follows it.
    const SAMPLE: &str = "\
S   ID  QUANT   RATE    WAIT    BUSY   W/Q   B/Q  ERR FORMAT           NAME
C   29      0      0    ---     ---   ---   ---     0                  Dummy-Driver
C   37      0      0    ---     ---   ---   ---     0                  alsa_output.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.analog-stereo
S   ID  QUANT   RATE    WAIT    BUSY   W/Q   B/Q  ERR FORMAT           NAME
I   29      0      0   0,0us   0,0us  ???   ???     0                  Dummy-Driver
R   37   1024  48000 339,3us   4,1us  0,02  0,00    0    S16LE 2 48000 alsa_output.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.analog-stereo
R   64      0      0   3,3us  26,3us  0,00  0,00   12    S32LE 2 48000  + alsa_output.pci-0000_2d_00.4.analog-stereo
R   95      0      0  31,1us   8,3us  0,00  0,00    0         F32P 2 0  + lm.strip.2
R  121      0      0   6,6us   5,2us  0,00  0,00    0    F32LE 2 48000  = lm.meter.s2
R   54   1024  48000 173,9us   0,8us  0,01  0,00 4510    S16LE 1 48000  + alsa_input.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.mono-fallback
S   53      0      0    ---     ---   ---   ---     0                  bluez_midi.server
";

    #[test]
    fn parses_the_last_block_with_followers() {
        let s = parse_pw_top(SAMPLE);
        let get = |n: &str| s.iter().find(|x| x.name == n).cloned().unwrap();
        assert!(
            !get("alsa_output.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.analog-stereo")
                .follower
        );
        let pci = get("alsa_output.pci-0000_2d_00.4.analog-stereo");
        assert!(pci.follower);
        assert_eq!(pci.errors, 12);
        assert!(
            !get("lm.meter.s2").follower,
            "= is a passive link, not a follower"
        );
        assert_eq!(
            get("alsa_input.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.mono-fallback")
                .errors,
            4510
        );
        assert_eq!(get("bluez_midi.server").errors, 0);
    }

    #[test]
    fn cards_group_input_and_output() {
        let a =
            card_of("alsa_input.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.mono-fallback");
        let b =
            card_of("alsa_output.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.analog-stereo");
        assert_eq!(
            a,
            Some("usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00")
        );
        assert_eq!(a, b);
        assert_eq!(card_of("bluez_output.AA_BB.1"), None);
        assert_eq!(card_of("lm.strip.2"), None);
    }

    fn app() -> AppState {
        let strip = StripState {
            id: 1,
            kind: StripKind::Hardware,
            label: "Mic".into(),
            hw_key: Some(
                "alsa_input.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.mono-fallback"
                    .into(),
            ),
            online: true,
            gain_db: 0.0,
            mute: false,
            solo: false,
            routes: Default::default(),
            gate: Default::default(),
            comp: Default::default(),
            eq: Default::default(),
        };
        let bus = BusState {
            id: BusId::A1,
            label: "Speakers".into(),
            target_hw_key: Some("alsa_output.pci-0000_2d_00.4.analog-stereo".into()),
            online: true,
            gain_db: 0.0,
            mute: false,
            limiter: LimiterParams::default(),
        };
        let dev = |n: &str, d: &str| DeviceInfo {
            node_name: n.into(),
            description: d.into(),
            media_class: "Audio/Source".into(),
            serial: 1,
            channels: 2,
            id: 1,
        };
        AppState {
            strips: vec![strip],
            buses: vec![bus],
            devices: vec![
                dev(
                    "alsa_input.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.mono-fallback",
                    "Razer BlackShark V2 Pro",
                ),
                dev(
                    "alsa_output.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.analog-stereo",
                    "Razer BlackShark V2 Pro",
                ),
                dev("alsa_output.pci-0000_2d_00.4.analog-stereo", "Onboard"),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn a_used_follower_with_rising_errors_is_a_problem() {
        let stats = parse_pw_top(SAMPLE);
        let mic = "alsa_input.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.mono-fallback";
        let last: HashMap<String, u64> = [
            (mic.to_string(), 4000),
            ("alsa_output.pci-0000_2d_00.4.analog-stereo".to_string(), 10),
        ]
        .into();
        let found = detect(&stats, &last, &app(), &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        let p = &found[0];
        assert_eq!(p.card, "usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00");
        assert_eq!(p.description, "Razer BlackShark V2 Pro");
        assert_eq!(p.errors, 510);
        assert_eq!(p.nodes.len(), 2, "input and output of the headset");
        // The onboard card gained 2 errors: under the threshold.
        // Already a leader: not reported again.
        assert!(detect(&stats, &last, &app(), std::slice::from_ref(&p.card)).is_empty());
        // No earlier sample: nothing to compare.
        assert!(detect(&stats, &HashMap::new(), &app(), &[]).is_empty());
    }

    #[test]
    fn rule_matches_every_node_of_the_card() {
        let text = rule_text(&["usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00".into()]);
        assert!(text.contains("# card: usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00"));
        assert!(
            text.contains(
                r#"node.name = "~alsa_.*usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.*""#
            ),
            "{text}"
        );
        assert!(text.contains("priority.driver = 3000"));
    }
}
