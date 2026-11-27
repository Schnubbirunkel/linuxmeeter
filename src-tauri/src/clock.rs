//! Clock helper: spot devices that drop out because they follow another
//! device's clock, and let them set the clock instead.
//!
//! When linuxmeeter links, say, a wireless USB headset's mic to the onboard
//! speakers, PipeWire runs both on one clock: one device drives, the other
//! follows with adaptive resampling. Some devices (wireless dongles above
//! all) cannot follow cleanly; their error counter in `pw-top` climbs by
//! hundreds a second and they sound like a robot.
//!
//! Detection reads `pw-top -b` every few seconds. There are two fixes, both a
//! WirePlumber rule in a file linuxmeeter owns
//! (`~/.config/wireplumber/wireplumber.conf.d/60-linuxmeeter-clock.conf`):
//!
//! - **lead**: every node of that sound card gets a high `priority.driver`, so
//!   it sets the clock instead of following;
//! - **buffer**: when the device it follows already sets the clock on purpose
//!   (priority ≥ `LEADER_PRIORITY` — a second leader would only tie with it and
//!   one of the two would drop out again), the follower gets a larger ALSA
//!   period and headroom instead, which lets it follow cleanly.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lm_protocol::AppState;
use serde::{Deserialize, Serialize};

/// How often pw-top is sampled.
const INTERVAL: Duration = Duration::from_secs(10);
/// New errors per interval that count as "dropping out" (the headset that
/// prompted this made hundreds a second).
const THRESHOLD: u64 = 20;
const LEADER_PRIORITY: u32 = 3000;
/// ALSA period size and headroom (frames) for the buffer fix.
const BUFFER_FRAMES: u32 = 1024;

/// What the rule file does for a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fix {
    /// It sets the clock.
    Lead,
    /// It follows, with a larger buffer.
    Buffer,
}

/// One node's line in `pw-top -b`.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeStat {
    pub name: String,
    pub errors: u64,
    /// Listed under another node ("+"): it follows that driver's clock.
    pub follower: bool,
    /// The driver it is listed under (followers only).
    pub driver: Option<String>,
}

/// The nodes of the last complete block of `pw-top -b -n N` output.
pub fn parse_pw_top(text: &str) -> Vec<NodeStat> {
    let mut blocks: Vec<Vec<NodeStat>> = Vec::new();
    let mut driver: Option<String> = None;
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
        let mark = t[t.len() - 2];
        let follower = mark == "+";
        if mark != "+" && mark != "=" {
            driver = Some(name.clone());
        }
        block.push(NodeStat {
            name,
            errors,
            follower,
            driver: if follower { driver.clone() } else { None },
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
    /// The device it follows (human name), if known.
    pub follows: Option<String>,
    /// The fix to offer: `Buffer` when what it follows sets the clock on
    /// purpose, else `Lead`.
    pub remedy: Fix,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockInfo {
    pub problems: Vec<Problem>,
    /// Cards that set the clock by linuxmeeter's rule.
    pub leaders: Vec<String>,
    /// Cards that follow with a larger buffer by linuxmeeter's rule.
    pub buffered: Vec<String>,
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
                    let fixed = entries();
                    let leaders: Vec<String> = fixed
                        .iter()
                        .filter(|(_, f)| *f == Fix::Lead)
                        .map(|(c, _)| c.clone())
                        .collect();
                    let cards: Vec<String> = fixed.into_iter().map(|(c, _)| c).collect();
                    let driver_leads = |driver: &str| {
                        card_of(driver).is_some_and(|c| leaders.iter().any(|l| l == c))
                            || driver_priority(driver).is_some_and(|p| p >= LEADER_PRIORITY as i64)
                    };
                    let found = detect(&stats, &last, &app, &cards, &driver_leads);
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
            buffered: buffered(),
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

/// `priority.driver` of a node, from `pw-dump` (only asked when a problem
/// is found).
fn driver_priority(node_name: &str) -> Option<i64> {
    let out = std::process::Command::new("pw-dump").output().ok()?;
    let objects: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).ok()?;
    objects
        .iter()
        .filter_map(|o| o.get("info")?.get("props"))
        .find(|p| p.get("node.name").and_then(|n| n.as_str()) == Some(node_name))
        .and_then(|p| p.get("priority.driver"))
        .and_then(|v| v.as_i64())
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
/// clock and gained at least THRESHOLD errors since the last sample. Cards in
/// `fixed` already have a rule. `driver_leads(node)` says whether a driver
/// sets the clock on purpose (then the remedy is a buffer, not a second leader).
fn detect(
    stats: &[NodeStat],
    last: &HashMap<String, u64>,
    app: &AppState,
    fixed: &[String],
    driver_leads: &dyn Fn(&str) -> bool,
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
        if fixed.iter().any(|l| l == card) || out.iter().any(|p| p.card == card) {
            continue;
        }
        let describe = |n: &str| {
            app.devices
                .iter()
                .find(|d| d.node_name == n)
                .map(|d| d.description.clone())
                .unwrap_or_else(|| n.to_string())
        };
        let follows = s.driver.as_deref().map(describe);
        let remedy = match s.driver.as_deref() {
            Some(d) if driver_leads(d) => Fix::Buffer,
            _ => Fix::Lead,
        };
        let nodes: Vec<String> = app
            .devices
            .iter()
            .map(|d| d.node_name.clone())
            .filter(|n| card_of(n) == Some(card))
            .collect();
        out.push(Problem {
            card: card.to_string(),
            description: describe(&s.name),
            nodes,
            errors: new,
            follows,
            remedy,
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

/// Cards in linuxmeeter's rule file and what it does for each
/// (`# card:` = lead, `# buffer:` = larger buffer).
pub fn entries() -> Vec<(String, Fix)> {
    let text = std::fs::read_to_string(rule_path()).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if let Some(c) = l.strip_prefix("# card: ") {
                Some((c.trim().to_string(), Fix::Lead))
            } else {
                l.strip_prefix("# buffer: ")
                    .map(|c| (c.trim().to_string(), Fix::Buffer))
            }
        })
        .collect()
}

/// Cards that set the clock by linuxmeeter's rule.
pub fn leaders() -> Vec<String> {
    entries()
        .into_iter()
        .filter(|(_, f)| *f == Fix::Lead)
        .map(|(c, _)| c)
        .collect()
}

/// Cards that follow with a larger buffer by linuxmeeter's rule.
pub fn buffered() -> Vec<String> {
    entries()
        .into_iter()
        .filter(|(_, f)| *f == Fix::Buffer)
        .map(|(c, _)| c)
        .collect()
}

/// The rule file for `entries`.
pub fn rule_text(entries: &[(String, Fix)]) -> String {
    let mut s = String::from(
        "# Written by linuxmeeter (Settings → Clock): sound cards that set the\n\
         # clock (card) or follow with a larger buffer (buffer); edit in linuxmeeter.\n",
    );
    for (c, f) in entries {
        let tag = match f {
            Fix::Lead => "card",
            Fix::Buffer => "buffer",
        };
        s += &format!("# {tag}: {c}\n");
    }
    s += "monitor.alsa.rules = [\n";
    for (c, f) in entries {
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
        let props = match f {
            Fix::Lead => format!(
                "        priority.driver = {LEADER_PRIORITY}\n        session.suspend-timeout-seconds = 0\n"
            ),
            Fix::Buffer => format!(
                "        api.alsa.period-size = {BUFFER_FRAMES}\n        api.alsa.headroom = {BUFFER_FRAMES}\n"
            ),
        };
        s += &format!(
            "  {{\n    matches = [ {{ node.name = \"~alsa_.*{pattern}.*\" }} ]\n    actions = {{\n      update-props = {{\n{props}      }}\n    }}\n  }}\n"
        );
    }
    s += "]\n";
    s
}

/// Set (Some) or remove (None) the rule for a card, removing the file when
/// no card is left.
pub fn set_fix(card: &str, fix: Option<Fix>) -> Result<(), String> {
    let mut list = entries();
    list.retain(|(c, _)| c != card);
    if let Some(f) = fix {
        list.push((card.to_string(), f));
    }
    let path = rule_path();
    if list.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, rule_text(&list))
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
    use lm_protocol::{
        BusId, BusState, DeviceInfo, EqParams, LimiterParams, StripKind, StripState,
    };

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
            eq: EqParams::default(),
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
        let found = detect(&stats, &last, &app(), &[], &|_| false);
        assert_eq!(found.len(), 1, "{found:?}");
        let p = &found[0];
        assert_eq!(p.card, "usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00");
        assert_eq!(p.description, "Razer BlackShark V2 Pro");
        assert_eq!(p.errors, 510);
        assert_eq!(p.nodes.len(), 2, "input and output of the headset");
        assert_eq!(p.remedy, Fix::Lead);
        assert_eq!(
            p.follows.as_deref(),
            Some("Razer BlackShark V2 Pro"),
            "listed under the headset output"
        );
        // The onboard card gained 2 errors: under the threshold.
        // Already a leader: not reported again.
        assert!(detect(
            &stats,
            &last,
            &app(),
            std::slice::from_ref(&p.card),
            &|_| false
        )
        .is_empty());
        // No earlier sample: nothing to compare.
        assert!(detect(&stats, &HashMap::new(), &app(), &[], &|_| false).is_empty());
        // What it follows sets the clock on purpose: a buffer, not a tie.
        let found = detect(&stats, &last, &app(), &[], &|d| {
            d.starts_with("alsa_output.usb-1532")
        });
        assert_eq!(found[0].remedy, Fix::Buffer);
    }

    #[test]
    fn rule_matches_every_node_of_the_card() {
        let text = rule_text(&[(
            "usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00".into(),
            Fix::Lead,
        )]);
        assert!(text.contains("# card: usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00"));
        assert!(
            text.contains(
                r#"node.name = "~alsa_.*usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.*""#
            ),
            "{text}"
        );
        assert!(text.contains("priority.driver = 3000"));
    }

    /// The user's case: the wireless lavalier follows the headset mic, which
    /// leads by rule. The buffer rule touches only the lavalier.
    #[test]
    fn buffer_rule_sets_period_and_headroom_only() {
        let lav = "usb-Shenzhen_jiayz_photo_industrial_Ltd_BY-V10_433037373930332E-01";
        let text = rule_text(&[
            (
                "usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00".into(),
                Fix::Lead,
            ),
            (lav.into(), Fix::Buffer),
        ]);
        assert!(text.contains(&format!("# buffer: {lav}")));
        let lav_rule = &text[text.find(&format!("~alsa_.*{lav}")).unwrap()..];
        assert!(lav_rule.contains("api.alsa.period-size = 1024"));
        assert!(lav_rule.contains("api.alsa.headroom = 1024"));
        assert!(!lav_rule.contains("priority.driver"), "{text}");
    }

    #[test]
    fn followers_know_their_driver() {
        let s = parse_pw_top(SAMPLE);
        let pci = s
            .iter()
            .find(|x| x.name == "alsa_output.pci-0000_2d_00.4.analog-stereo")
            .unwrap();
        assert_eq!(
            pci.driver.as_deref(),
            Some("alsa_output.usb-1532_Razer_BlackShark_V2_Pro_2.4_O001000007-00.analog-stereo")
        );
        assert!(s
            .iter()
            .find(|x| x.name == "lm.meter.s2")
            .unwrap()
            .driver
            .is_none());
    }
}
