//! Remote control of a running mixer: `linuxmeeter ctl <command> …`.
//!
//! The running app listens on a Unix socket; the `ctl` subcommand connects,
//! sends its arguments as one JSON line and prints the one-line reply. Bind
//! the commands to keys in the desktop's shortcut settings (KDE: System
//! Settings → Keyboard → Shortcuts → Add New → Command) and they work no
//! matter which window has focus.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use lm_protocol::{AppState, EngineCommand, Target};

pub const HELP: &str = "\
usage: linuxmeeter ctl <command> [args]

  toggle-mute <channel>          mute / unmute
  mute <channel>                 mute
  unmute <channel>               unmute
  volume-up <channel> [percent]  raise the fader (default 5 % of its travel)
  volume-down <channel> [percent]
  rename <channel> <new name>    rename a strip or bus (also in other programs)
  profile <name>                 switch profile (a new name copies the current mixer)
  profile-next | profile-prev    cycle through saved profiles
  show                           bring the window up
  list                           channels and profiles

<channel> is a strip or bus name (as shown in the mixer, any case), a bus id
(A1, A2, B1, B2) or a strip number (strip:3). Names change when you rename a
strip; ids do not.";

pub fn socket_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => PathBuf::from(dir).join("linuxmeeter.sock"),
        None => std::env::temp_dir().join(format!(
            "linuxmeeter-{}.sock",
            std::env::var("USER").unwrap_or_else(|_| "user".into())
        )),
    }
}

/// Send `args` to the running instance; returns its reply line.
pub fn send(args: &[String]) -> Result<String, String> {
    let mut stream =
        UnixStream::connect(socket_path()).map_err(|_| "linuxmeeter is not running".to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = serde_json::to_string(args).map_err(|e| e.to_string())?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    Ok(reply.trim_end().to_string())
}

/// What the app does for one `ctl` request.
#[derive(Debug)]
pub enum Action {
    /// Forward to the engine. `notify` = the target to report on once the
    /// engine has applied it.
    Engine {
        cmd: EngineCommand,
        notify: Option<Target>,
        reply: String,
    },
    Show,
    Reply(String),
}

/// Strip or bus named by `spec`, with its display label.
pub fn resolve(spec: &str, state: &AppState) -> Result<(Target, String), String> {
    let spec = spec.trim();
    let lower = spec.to_lowercase();
    if let Some(b) = state
        .buses
        .iter()
        .find(|b| format!("{:?}", b.id).to_lowercase() == lower)
    {
        return Ok((Target::Bus { bus: b.id }, b.label.clone()));
    }
    let num = lower
        .strip_prefix("strip:")
        .or_else(|| lower.strip_prefix('#'))
        .unwrap_or(&lower);
    if let Ok(id) = num.parse::<u32>() {
        if let Some(s) = state.strips.iter().find(|s| s.id == id) {
            return Ok((Target::Strip { strip: s.id }, s.label.clone()));
        }
    }
    if let Some(s) = state
        .strips
        .iter()
        .find(|s| s.label.to_lowercase() == lower)
    {
        return Ok((Target::Strip { strip: s.id }, s.label.clone()));
    }
    if let Some(b) = state.buses.iter().find(|b| b.label.to_lowercase() == lower) {
        return Ok((Target::Bus { bus: b.id }, b.label.clone()));
    }
    Err(format!(
        "no strip or bus called \"{spec}\" (try `linuxmeeter ctl list`)"
    ))
}

pub fn parse(args: &[String], state: &AppState) -> Result<Action, String> {
    let (cmd, rest) = args.split_first().ok_or_else(|| HELP.to_string())?;
    let channel = |i: usize| -> Result<(Target, String), String> {
        let spec = rest
            .get(i)
            .ok_or_else(|| format!("`{cmd}` needs a channel name"))?;
        resolve(spec, state)
    };
    let engine = |cmd: EngineCommand, notify: Option<Target>, reply: String| {
        Ok(Action::Engine { cmd, notify, reply })
    };
    match cmd.as_str() {
        "toggle-mute" | "mute" | "unmute" => {
            let (target, label) = channel(0)?;
            let cmd = match cmd.as_str() {
                "toggle-mute" => EngineCommand::ToggleMute { target },
                c => EngineCommand::SetMute {
                    target,
                    mute: c == "mute",
                },
            };
            engine(cmd, Some(target), label)
        }
        "volume-up" | "volume-down" => {
            let (target, label) = channel(0)?;
            let percent: f32 = match rest.get(1) {
                Some(p) => p
                    .trim_end_matches('%')
                    .parse()
                    .ok()
                    .filter(|p: &f32| p.is_finite() && *p > 0.0 && *p <= 100.0)
                    .ok_or_else(|| format!("\"{p}\" is not a percentage between 0 and 100"))?,
                None => 5.0,
            };
            let sign = if cmd == "volume-up" { 1.0 } else { -1.0 };
            engine(
                EngineCommand::NudgeGain {
                    target,
                    step: sign * percent / 100.0,
                },
                Some(target),
                label,
            )
        }
        "rename" => {
            let (target, old) = channel(0)?;
            let label = lm_protocol::clean_label(&rest[1..].join(" "))
                .ok_or("`rename` needs a new name")?;
            engine(
                EngineCommand::Rename {
                    target,
                    label: label.clone(),
                },
                None,
                format!("{old} -> {label}"),
            )
        }
        "profile" => {
            let name = rest.join(" ");
            let name = lm_protocol::clean_profile_name(&name).ok_or("`profile` needs a name")?;
            engine(
                EngineCommand::LoadProfile { name: name.clone() },
                None,
                name,
            )
        }
        "profile-next" => engine(
            EngineCommand::CycleProfile { step: 1 },
            None,
            "next profile".into(),
        ),
        "profile-prev" => engine(
            EngineCommand::CycleProfile { step: -1 },
            None,
            "previous profile".into(),
        ),
        "show" => Ok(Action::Show),
        "list" => Ok(Action::Reply(list(state))),
        "help" | "--help" | "-h" => Ok(Action::Reply(HELP.into())),
        other => Err(format!("unknown command \"{other}\"\n\n{HELP}")),
    }
}

fn list(state: &AppState) -> String {
    let mut out = String::from("strips:\n");
    let level = |db: f32, mute: bool| {
        let pct = (lm_protocol::taper::db_to_pos(db) * 100.0).round();
        format!("{pct:>3.0} % {}", if mute { " muted" } else { "" })
    };
    for s in &state.strips {
        out += &format!(
            "  strip:{:<3} {}  {}\n",
            s.id,
            level(s.gain_db, s.mute),
            s.label
        );
    }
    out += "buses:\n";
    for b in &state.buses {
        out += &format!(
            "  {:<9} {}  {}\n",
            format!("{:?}", b.id),
            level(b.gain_db, b.mute),
            b.label
        );
    }
    out += "profiles:\n";
    for p in &state.profiles {
        out += &format!(
            "  {}{}\n",
            p,
            if *p == state.profile_name {
                "  (active)"
            } else {
                ""
            }
        );
    }
    out.trim_end().to_string()
}

/// Bind the socket. Fails if another instance is already listening.
pub fn listen() -> std::io::Result<UnixListener> {
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "already running",
        ));
    }
    let _ = std::fs::remove_file(&path); // stale socket from a crash
    UnixListener::bind(&path)
}

/// Serve requests forever; `handle` turns one request into a reply line.
pub fn serve(listener: UnixListener, handle: impl Fn(Vec<String>) -> String + Send + 'static) {
    std::thread::Builder::new()
        .name("lm-control".into())
        .spawn(move || {
            for conn in listener.incoming() {
                let Ok(mut conn) = conn else { continue };
                let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
                let mut line = String::new();
                if BufReader::new(&conn).read_line(&mut line).is_err() {
                    continue;
                }
                let reply = match serde_json::from_str::<Vec<String>>(&line) {
                    Ok(args) => handle(args),
                    Err(_) => "error: bad request".into(),
                };
                // Replies are one line on the wire; multi-line text is escaped.
                let _ = conn.write_all(format!("{}\n", reply.replace('\n', "\\n")).as_bytes());
            }
        })
        .expect("spawn control thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_protocol::{BusId, BusState, LimiterParams, StripKind, StripState};

    fn state() -> AppState {
        let strip = |id, label: &str| StripState {
            id,
            kind: StripKind::Virtual,
            label: label.into(),
            hw_key: None,
            online: true,
            gain_db: 0.0,
            mute: false,
            solo: false,
            routes: Default::default(),
            gate: Default::default(),
            comp: Default::default(),
            eq: Default::default(),
        };
        let bus = |id, label: &str| BusState {
            id,
            label: label.into(),
            target_hw_key: None,
            online: true,
            gain_db: 0.0,
            mute: false,
            limiter: LimiterParams::default(),
        };
        AppState {
            strips: vec![strip(1, "Mic"), strip(3, "Game Audio")],
            buses: vec![bus(BusId::A1, "Speakers"), bus(BusId::B1, "Stream Mic")],
            profile_name: "Default".into(),
            profiles: vec!["Default".into(), "Gaming".into()],
            ..Default::default()
        }
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn channels_resolve_by_name_bus_id_and_number() {
        let s = state();
        assert_eq!(
            resolve("game audio", &s).unwrap().0,
            Target::Strip { strip: 3 }
        );
        assert_eq!(resolve("a1", &s).unwrap().0, Target::Bus { bus: BusId::A1 });
        assert_eq!(
            resolve("strip:1", &s).unwrap().0,
            Target::Strip { strip: 1 }
        );
        assert_eq!(resolve("3", &s).unwrap().0, Target::Strip { strip: 3 });
        assert_eq!(
            resolve("Stream Mic", &s).unwrap().0,
            Target::Bus { bus: BusId::B1 }
        );
        assert!(resolve("nope", &s).is_err());
        assert!(resolve("strip:2", &s).is_err());
    }

    #[test]
    fn volume_steps_default_to_five_percent() {
        let s = state();
        match parse(&args(&["volume-down", "Mic"]), &s).unwrap() {
            Action::Engine {
                cmd: EngineCommand::NudgeGain { target, step },
                notify,
                ..
            } => {
                assert_eq!(target, Target::Strip { strip: 1 });
                assert_eq!(step, -0.05);
                assert_eq!(notify, Some(target));
            }
            other => panic!("{other:?}"),
        }
        match parse(&args(&["volume-up", "B1", "10%"]), &s).unwrap() {
            Action::Engine {
                cmd: EngineCommand::NudgeGain { step, .. },
                ..
            } => assert_eq!(step, 0.1),
            other => panic!("{other:?}"),
        }
        assert!(parse(&args(&["volume-up", "B1", "-3"]), &s).is_err());
        assert!(parse(&args(&["volume-up"]), &s).is_err());
    }

    #[test]
    fn mute_commands() {
        let s = state();
        assert!(matches!(
            parse(&args(&["toggle-mute", "speakers"]), &s).unwrap(),
            Action::Engine {
                cmd: EngineCommand::ToggleMute {
                    target: Target::Bus { bus: BusId::A1 }
                },
                ..
            }
        ));
        assert!(matches!(
            parse(&args(&["unmute", "mic"]), &s).unwrap(),
            Action::Engine {
                cmd: EngineCommand::SetMute { mute: false, .. },
                ..
            }
        ));
    }

    #[test]
    fn profile_names_may_contain_spaces() {
        let s = state();
        match parse(&args(&["profile", "Late", "Night"]), &s).unwrap() {
            Action::Engine {
                cmd: EngineCommand::LoadProfile { name },
                ..
            } => assert_eq!(name, "Late Night"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse(&args(&["profile-prev"]), &s).unwrap(),
            Action::Engine {
                cmd: EngineCommand::CycleProfile { step: -1 },
                ..
            }
        ));
    }

    #[test]
    fn rename_takes_the_rest_of_the_line() {
        match parse(&args(&["rename", "strip:3", "Game", "Chat"]), &state()).unwrap() {
            Action::Engine {
                cmd: EngineCommand::Rename { target, label },
                ..
            } => {
                assert_eq!(target, Target::Strip { strip: 3 });
                assert_eq!(label, "Game Chat");
            }
            other => panic!("{other:?}"),
        }
        assert!(parse(&args(&["rename", "mic"]), &state()).is_err());
    }

    #[test]
    fn unknown_commands_are_errors() {
        assert!(parse(&args(&["explode"]), &state()).is_err());
        assert!(parse(&[], &state()).is_err());
    }
}
