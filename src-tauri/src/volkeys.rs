//! The volume keys (keyboard, a headset's wheel) control the profile's main
//! hardware output instead of the default device — which, with "DEFAULT OUT"
//! on, is linuxmeeter's own System strip.
//!
//! On KDE the keys belong to Plasma's volume control (kglobalaccel component
//! "kmix"). While a profile names a main output, linuxmeeter takes them over:
//! it remembers Plasma's bindings, clears them, and binds the same keys to its
//! own `device-volume up|down|mute` actions. Without a main output — and when
//! linuxmeeter quits — Plasma gets them back.

use std::sync::{Arc, Mutex};

use crate::shortcuts::Shortcuts;

const KMIX: &str = "kmix";
/// (Plasma's action, ours, Qt key code if Plasma had none bound)
const ACTIONS: [(&str, &str, i32); 3] = [
    ("increase_volume", "device-volume up", 0x0100_0072), // Qt::Key_VolumeUp
    ("decrease_volume", "device-volume down", 0x0100_0070), // Qt::Key_VolumeDown
    ("mute", "device-volume mute", 0x0100_0071),          // Qt::Key_VolumeMute
];

pub struct VolumeKeys {
    sc: Arc<Shortcuts>,
    taken: Mutex<bool>,
}

impl VolumeKeys {
    pub fn new(sc: Arc<Shortcuts>) -> Self {
        Self {
            sc,
            taken: Mutex::new(false),
        }
    }

    /// Hold the keys while `want` (the profile has a main output), else give
    /// them back. Cheap when nothing changes.
    pub fn sync(&self, want: bool) {
        if !self.sc.available() {
            return;
        }
        let mut taken = self.taken.lock().expect("lock");
        if want && !*taken {
            self.take();
            *taken = true;
        } else if !want && *taken {
            self.give_back();
            *taken = false;
        }
    }

    fn take(&self) {
        let saved = crate::settings::get().kmix_volume_keys;
        for (kmix, ours, default) in ACTIONS {
            let current = self.sc.foreign_keys(KMIX, kmix);
            // Plasma's own keys if it has them; else what we saved last time
            // (linuxmeeter did not get to give them back); else the usual key.
            let keys = if !current.is_empty() {
                let keep = current.clone();
                let _ = crate::settings::update(|s| {
                    s.kmix_volume_keys.insert(kmix.to_string(), keep);
                });
                current
            } else if let Some(k) = saved.get(kmix).filter(|k| !k.is_empty()) {
                k.clone()
            } else {
                vec![vec![default, 0, 0, 0]]
            };
            if let Err(e) = self.sc.set_foreign(KMIX, kmix, &[]) {
                tracing::warn!("could not take {kmix} from Plasma: {e}");
            }
            if let Err(e) = self.sc.bind_sequences(ours, &keys) {
                tracing::warn!("could not bind {ours}: {e}");
            }
        }
        tracing::info!("volume keys now control the profile's main output");
    }

    fn give_back(&self) {
        let saved = crate::settings::get().kmix_volume_keys;
        for (kmix, ours, default) in ACTIONS {
            self.sc.unbind(ours);
            let keys = saved
                .get(kmix)
                .cloned()
                .unwrap_or_else(|| vec![vec![default, 0, 0, 0]]);
            if let Err(e) = self.sc.set_foreign(KMIX, kmix, &keys) {
                tracing::warn!("could not give {kmix} back to Plasma: {e}");
            }
        }
        let _ = crate::settings::update(|s| s.kmix_volume_keys.clear());
        tracing::info!("volume keys back with the desktop");
    }
}

/// `wpctl` arguments for a step on node `id`.
pub fn wpctl_args(target: &str, step: &str) -> Vec<String> {
    match step {
        "up" => vec![
            "set-volume".into(),
            "-l".into(),
            "1.0".into(),
            target.to_string(),
            "5%+".into(),
        ],
        "down" => vec!["set-volume".into(), target.to_string(), "5%-".into()],
        _ => vec!["set-mute".into(), target.to_string(), "toggle".into()],
    }
}

/// "Volume: 0.45" / "Volume: 0.45 [MUTED]" -> (45, muted)
pub fn parse_wpctl_volume(text: &str) -> Option<(i32, bool)> {
    let v: f32 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some(((v * 100.0).round() as i32, text.contains("MUTED")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpctl_steps() {
        assert_eq!(
            wpctl_args("42", "up"),
            ["set-volume", "-l", "1.0", "42", "5%+"]
        );
        assert_eq!(wpctl_args("42", "down"), ["set-volume", "42", "5%-"]);
        assert_eq!(wpctl_args("42", "mute"), ["set-mute", "42", "toggle"]);
    }

    #[test]
    fn wpctl_volume_parses() {
        assert_eq!(parse_wpctl_volume("Volume: 0.45\n"), Some((45, false)));
        assert_eq!(
            parse_wpctl_volume("Volume: 1.00 [MUTED]\n"),
            Some((100, true))
        );
        assert_eq!(parse_wpctl_volume("nope"), None);
    }
}
