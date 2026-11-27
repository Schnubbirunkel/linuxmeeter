//! Global shortcuts set inside the app, through KDE's shortcut service
//! (`org.kde.kglobalaccel`, Plasma 5 and 6, X11 and Wayland).
//!
//! Each binding is registered as an action of the component "linuxmeeter";
//! the action's unique name is the `ctl` command it runs ("toggle-mute
//! strip:3"), so a key press goes through exactly the same path as
//! `linuxmeeter ctl toggle-mute strip:3`. The bindings also show up (and can
//! be changed) in System Settings → Shortcuts; changes made there come back
//! through `yourShortcutsChanged` and are saved here too.
//!
//! Keys are stored as text ("Ctrl+Alt+M") and sent to KDE as Qt key codes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MsgType;
use zbus::MatchRule;

const SERVICE: &str = "org.kde.kglobalaccel";
const PATH: &str = "/kglobalaccel";
const IFACE: &str = "org.kde.KGlobalAccel";
const COMPONENT: &str = "linuxmeeter";

// kglobalaccel setShortcut flags.
const SET_PRESENT: u32 = 2;
const NO_AUTOLOADING: u32 = 4;

// ---- key text <-> Qt key codes ---------------------------------------------

const SHIFT: i32 = 0x0200_0000;
const CTRL: i32 = 0x0400_0000;
const ALT: i32 = 0x0800_0000;
const META: i32 = 0x1000_0000;
const KEYPAD: i32 = 0x2000_0000;
const MODS: [(&str, i32); 5] = [
    ("Meta", META),
    ("Ctrl", CTRL),
    ("Alt", ALT),
    ("Shift", SHIFT),
    ("Num", KEYPAD),
];

/// Named (non-character) keys, by the names the settings panel produces.
const NAMED: &[(&str, i32)] = &[
    ("Esc", 0x0100_0000),
    ("Tab", 0x0100_0001),
    ("Backspace", 0x0100_0003),
    ("Return", 0x0100_0004),
    ("Enter", 0x0100_0005),
    ("Ins", 0x0100_0006),
    ("Del", 0x0100_0007),
    ("Pause", 0x0100_0008),
    ("Print", 0x0100_0009),
    ("Home", 0x0100_0010),
    ("End", 0x0100_0011),
    ("Left", 0x0100_0012),
    ("Up", 0x0100_0013),
    ("Right", 0x0100_0014),
    ("Down", 0x0100_0015),
    ("PgUp", 0x0100_0016),
    ("PgDown", 0x0100_0017),
    ("ScrollLock", 0x0100_0026),
    ("Space", 0x20),
    ("Plus", 0x2b),
    ("Comma", 0x2c),
    ("Minus", 0x2d),
    ("Period", 0x2e),
    ("Slash", 0x2f),
    ("Semicolon", 0x3b),
    ("Equal", 0x3d),
    ("Asterisk", 0x2a),
    ("Apostrophe", 0x27),
    ("BracketLeft", 0x5b),
    ("Backslash", 0x5c),
    ("BracketRight", 0x5d),
    ("QuoteLeft", 0x60),
    ("Volume Down", 0x0100_0070),
    ("Volume Mute", 0x0100_0071),
    ("Volume Up", 0x0100_0072),
    ("Media Play", 0x0100_0080),
    ("Media Stop", 0x0100_0081),
    ("Media Previous", 0x0100_0082),
    ("Media Next", 0x0100_0083),
    ("Media Pause", 0x0100_0085),
    ("Toggle Media Play/Pause", 0x0100_0086),
    ("Microphone Mute", 0x0100_0113),
];

/// "Ctrl+Alt+M" -> Qt key code with modifier bits.
pub fn parse_keys(text: &str) -> Option<i32> {
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last()?;
    let mut code = 0;
    for m in mods {
        code |= MODS.iter().find(|(n, _)| n.eq_ignore_ascii_case(m))?.1;
    }
    let base = if let Some((_, c)) = NAMED.iter().find(|(n, _)| n.eq_ignore_ascii_case(key)) {
        *c
    } else if let Some(n) = key
        .strip_prefix(['F', 'f'])
        .and_then(|n| n.parse::<i32>().ok())
        .filter(|n| (1..=35).contains(n))
    {
        0x0100_0030 + n - 1
    } else if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) {
        key.to_ascii_uppercase().as_bytes()[0] as i32
    } else {
        return None;
    };
    Some(code | base)
}

/// Qt key code -> "Ctrl+Alt+M"; None for keys this table does not know.
pub fn format_keys(code: i32) -> Option<String> {
    let base = code & !(SHIFT | CTRL | ALT | META | KEYPAD);
    let name = if let Some((n, _)) = NAMED.iter().find(|(_, c)| *c == base) {
        n.to_string()
    } else if (0x0100_0030..0x0100_0030 + 35).contains(&base) {
        format!("F{}", base - 0x0100_0030 + 1)
    } else if (b'0' as i32..=b'9' as i32).contains(&base)
        || (b'A' as i32..=b'Z' as i32).contains(&base)
    {
        (base as u8 as char).to_string()
    } else {
        return None;
    };
    let mut out: Vec<String> = MODS
        .iter()
        .filter(|(_, bit)| code & bit != 0)
        .map(|(n, _)| n.to_string())
        .collect();
    out.push(name);
    Some(out.join("+"))
}

// ---- KDE service -------------------------------------------------------------

/// A shortcut's action id: [component, action, component name, action name].
fn action_id(action: &str) -> Vec<String> {
    vec![
        COMPONENT.into(),
        action.into(),
        "linuxmeeter".into(),
        describe(action),
    ]
}

/// The name System Settings shows: "Volume up strip:3", "Mute all microphones".
/// Built from the action (ids, not labels) so it stays right after a rename.
pub fn describe(action: &str) -> String {
    let (verb, target) = action.split_once(' ').unwrap_or((action, ""));
    let verb = match verb {
        "toggle-mute" => "Mute/unmute",
        "volume-up" => "Volume up",
        "volume-down" => "Volume down",
        "toggle-mute-mics" => "Mute all microphones",
        "profile-next" => "Next profile",
        "profile-prev" => "Previous profile",
        "device-volume" => match target {
            "up" => return "Volume up (profile's main output)".into(),
            "down" => return "Volume down (profile's main output)".into(),
            _ => return "Mute (profile's main output)".into(),
        },
        other => other,
    };
    format!("{verb} {target}").trim().to_string()
}

/// QKeySequence over D-Bus: a struct of four key codes.
fn sequence(code: i32) -> (Vec<i32>,) {
    (vec![code, 0, 0, 0],)
}

pub struct Shortcuts {
    conn: Option<Connection>,
    /// Friendly names of the actions registered this session (for setInactive).
    registered: Mutex<BTreeMap<String, String>>,
}

impl Shortcuts {
    /// Connect to KDE's shortcut service, register the saved bindings, and
    /// call `on_press(action)` for every key press. Without the service (not
    /// KDE) everything is a no-op and `available()` is false.
    pub fn start(
        on_press: impl Fn(&str) + Send + 'static,
        on_changed: impl Fn() + Send + 'static,
    ) -> Arc<Self> {
        let conn = Connection::session().ok().filter(|c| {
            let has = |method: &str| -> bool {
                c.call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    method,
                    &(),
                )
                .ok()
                .and_then(|m| m.body().deserialize::<Vec<String>>().ok())
                .is_some_and(|names| names.iter().any(|n| n == SERVICE))
            };
            has("ListNames") || has("ListActivatableNames")
        });
        if conn.is_none() {
            tracing::info!("KDE global shortcut service not found; in-app shortcuts disabled");
        }
        let me = Arc::new(Self {
            conn,
            registered: Mutex::new(BTreeMap::new()),
        });
        let Some(conn) = me.conn.clone() else {
            return me;
        };

        for (action, keys) in crate::settings::get().shortcuts {
            if let Err(e) = me.bind(&action, Some(&keys)) {
                tracing::warn!("shortcut {action} ({keys}): {e}");
            }
        }

        // Key presses: emitted on /component/linuxmeeter.
        let press_conn = conn.clone();
        std::thread::Builder::new()
            .name("lm-shortcuts".into())
            .spawn(move || {
                let rule = MatchRule::builder()
                    .msg_type(MsgType::Signal)
                    .interface("org.kde.kglobalaccel.Component")
                    .and_then(|b| b.member("globalShortcutPressed"))
                    .map(|b| b.build());
                let Ok(rule) = rule else { return };
                let Ok(iter) = MessageIterator::for_match_rule(rule, &press_conn, None) else {
                    return;
                };
                for msg in iter.flatten() {
                    if let Ok((component, action, _ts)) =
                        msg.body().deserialize::<(String, String, i64)>()
                    {
                        if component == COMPONENT {
                            on_press(&action);
                        }
                    }
                }
            })
            .expect("spawn shortcut listener");

        // Changed in System Settings: keep app.toml in step.
        std::thread::Builder::new()
            .name("lm-shortcuts-sync".into())
            .spawn(move || {
                let rule = MatchRule::builder()
                    .msg_type(MsgType::Signal)
                    .interface(IFACE)
                    .and_then(|b| b.member("yourShortcutsChanged"))
                    .map(|b| b.build());
                let Ok(rule) = rule else { return };
                let Ok(iter) = MessageIterator::for_match_rule(rule, &conn, None) else {
                    return;
                };
                for msg in iter.flatten() {
                    let Ok((id, keys)) =
                        msg.body().deserialize::<(Vec<String>, Vec<(Vec<i32>,)>)>()
                    else {
                        continue;
                    };
                    if id.first().map(String::as_str) != Some(COMPONENT) || id.len() < 2 {
                        continue;
                    }
                    let first = keys
                        .iter()
                        .filter_map(|(k,)| k.first().copied())
                        .find(|k| *k != 0);
                    let text = first.and_then(format_keys);
                    let _ = crate::settings::update(|s| match &text {
                        Some(t) => {
                            s.shortcuts.insert(id[1].clone(), t.clone());
                        }
                        None => {
                            s.shortcuts.remove(&id[1]);
                        }
                    });
                    on_changed();
                }
            })
            .expect("spawn shortcut sync");
        me
    }

    pub fn available(&self) -> bool {
        self.conn.is_some()
    }

    fn call<B>(&self, method: &str, body: &B) -> Result<zbus::message::Message, String>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let conn = self
            .conn
            .as_ref()
            .ok_or("KDE's global shortcut service is not running")?;
        conn.call_method(Some(SERVICE), PATH, Some(IFACE), method, body)
            .map_err(|e| e.to_string())
    }

    /// Bind `keys` to `action` (None removes it). Returns the stored key text.
    fn bind(&self, action: &str, keys: Option<&str>) -> Result<Option<String>, String> {
        let id = action_id(action);
        let Some(keys) = keys else {
            let _ = self.call(
                "setShortcutKeys",
                &(
                    id.clone(),
                    Vec::<(Vec<i32>,)>::new(),
                    SET_PRESENT | NO_AUTOLOADING,
                ),
            );
            let _ = self.call("unregister", &(COMPONENT, action));
            self.registered.lock().expect("lock").remove(action);
            return Ok(None);
        };
        let code = parse_keys(keys)
            .ok_or_else(|| format!("\"{keys}\" is not a key this app understands"))?;
        self.call("doRegister", &(id.clone(),))?;
        let reply = self.call(
            "setShortcutKeys",
            &(id, vec![sequence(code)], SET_PRESENT | NO_AUTOLOADING),
        )?;
        let got: Vec<(Vec<i32>,)> = reply.body().deserialize().map_err(|e| e.to_string())?;
        if !got.iter().any(|(k,)| k.first() == Some(&code)) {
            return Err(format!("{keys} is already used by another shortcut"));
        }
        self.registered
            .lock()
            .expect("lock")
            .insert(action.into(), describe(action));
        Ok(format_keys(code))
    }

    /// Set (or clear) a binding and save it.
    pub fn set(&self, action: &str, keys: Option<&str>) -> Result<Option<String>, String> {
        // Another action may already hold these keys: free them first.
        if let Some(k) = keys {
            let taken: Vec<String> = crate::settings::get()
                .shortcuts
                .iter()
                .filter(|(a, v)| a.as_str() != action && parse_keys(v) == parse_keys(k))
                .map(|(a, _)| a.clone())
                .collect();
            for a in taken {
                let _ = self.bind(&a, None);
                let _ = crate::settings::update(|s| {
                    s.shortcuts.remove(&a);
                });
            }
        }
        let stored = self.bind(action, keys)?;
        crate::settings::update(|s| match &stored {
            Some(k) => {
                s.shortcuts.insert(action.into(), k.clone());
            }
            None => {
                s.shortcuts.remove(action);
            }
        })?;
        Ok(stored)
    }

    // ---- other components' shortcuts (the volume keys) ------------------------

    /// Key sequences another component's action is bound to.
    pub fn foreign_keys(&self, component: &str, action: &str) -> Vec<Vec<i32>> {
        let id = vec![
            component.to_string(),
            action.to_string(),
            String::new(),
            String::new(),
        ];
        self.call("shortcutKeys", &(id,))
            .ok()
            .and_then(|m| m.body().deserialize::<Vec<(Vec<i32>,)>>().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|(k,)| k)
            .filter(|k| k.iter().any(|c| *c != 0))
            .collect()
    }

    /// Rebind another component's action (empty = no keys).
    pub fn set_foreign(
        &self,
        component: &str,
        action: &str,
        keys: &[Vec<i32>],
    ) -> Result<(), String> {
        let id = vec![
            component.to_string(),
            action.to_string(),
            String::new(),
            String::new(),
        ];
        let keys: Vec<(Vec<i32>,)> = keys.iter().map(|k| (k.clone(),)).collect();
        self.call("setForeignShortcutKeys", &(id, keys)).map(|_| ())
    }

    /// Register one of our actions on exact key sequences (not saved in app.toml).
    pub fn bind_sequences(&self, action: &str, keys: &[Vec<i32>]) -> Result<(), String> {
        let id = action_id(action);
        self.call("doRegister", &(id.clone(),))?;
        let keys: Vec<(Vec<i32>,)> = keys.iter().map(|k| (k.clone(),)).collect();
        self.call("setShortcutKeys", &(id, keys, SET_PRESENT | NO_AUTOLOADING))?;
        self.registered
            .lock()
            .expect("lock")
            .insert(action.into(), describe(action));
        Ok(())
    }

    /// Drop one of our actions.
    pub fn unbind(&self, action: &str) {
        let _ = self.bind(action, None);
    }

    /// Plasma's volume OSD, showing `percent` (Plasma 6, then Plasma 5 form).
    pub fn osd_volume(&self, percent: i32) {
        let Some(conn) = &self.conn else { return };
        let call = |body: &dyn Fn() -> Result<zbus::message::Message, zbus::Error>| body().is_ok();
        let six = || {
            conn.call_method(
                Some("org.kde.plasmashell"),
                "/org/kde/osdService",
                Some("org.kde.osdService"),
                "volumeChanged",
                &(percent, 100i32),
            )
        };
        let five = || {
            conn.call_method(
                Some("org.kde.plasmashell"),
                "/org/kde/osdService",
                Some("org.kde.osdService"),
                "volumeChanged",
                &(percent,),
            )
        };
        if !call(&six) {
            let _ = call(&five);
        }
    }

    /// Release the keys while the app is not running (KDE keeps the bindings).
    pub fn deactivate(&self) {
        for action in self.registered.lock().expect("lock").keys() {
            let _ = self.call("setInactive", &(action_id(action),));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for k in [
            "Ctrl+Alt+M",
            "Meta+F9",
            "Shift+Up",
            "Volume Mute",
            "Ctrl+Num+Plus",
            "Alt+5",
            "Microphone Mute",
            "F12",
        ] {
            let code = parse_keys(k).unwrap_or_else(|| panic!("{k}"));
            assert_eq!(format_keys(code).as_deref(), Some(k));
        }
    }

    #[test]
    fn qt_codes_match_qt() {
        assert_eq!(parse_keys("Ctrl+M"), Some(0x0400_0000 | 0x4d));
        assert_eq!(parse_keys("F1"), Some(0x0100_0030));
        assert_eq!(
            parse_keys("Meta+Shift+Down"),
            Some(0x1000_0000 | 0x0200_0000 | 0x0100_0015)
        );
        assert_eq!(parse_keys("ctrl+alt+m"), parse_keys("Ctrl+Alt+M"));
    }

    #[test]
    fn names_for_system_settings() {
        assert_eq!(describe("volume-up strip:3"), "Volume up strip:3");
        assert_eq!(describe("toggle-mute-mics"), "Mute all microphones");
    }

    #[test]
    fn rejects_junk() {
        assert_eq!(parse_keys(""), None);
        assert_eq!(parse_keys("Ctrl+"), None);
        assert_eq!(parse_keys("Hyper+M"), None);
        assert_eq!(parse_keys("F99"), None);
        assert_eq!(parse_keys("Ä"), None);
    }
}
